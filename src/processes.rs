//! ポップアップに表示する、物理メモリ使用量の大きいプロセスの取得。

use std::{
    collections::HashMap,
    io::{self, Read},
    os::unix::io::AsRawFd,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// アプリ、または個別プロセスのメモリ使用量。
pub struct MemoryConsumer {
    /// アプリまたはプロセスの表示名。
    pub name: String,
    /// 合算したphysical footprint（物理メモリ使用量、バイト単位）。
    pub bytes: u64,
    /// 合算対象のプロセス数。
    pub process_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessMemory {
    pid: u32,
    name: String,
    bytes: u64,
    application: Option<AppBundle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppBundle {
    path: PathBuf,
    name: String,
}

/// `footprint` の出力からphysical footprintを取り出す。
pub(crate) fn parse_phys_footprint(output: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        let (label, value) = line.trim().split_once(':')?;
        if label.trim() != "phys_footprint" {
            return None;
        }
        let value = value.trim().replace(',', "");
        let mut parts = value.split_whitespace();
        let amount = parts.next()?.parse::<u64>().ok()?;
        match parts.next() {
            None | Some("B") | Some("byte") | Some("bytes") => Some(amount),
            Some("KB") => amount.checked_mul(1024),
            Some("MB") => amount.checked_mul(1024 * 1024),
            Some("GB") => amount.checked_mul(1024 * 1024 * 1024),
            Some(_) => None,
        }
    })
}

fn parse_footprint_header(line: &str) -> Option<(u32, String)> {
    let line = line.trim();
    let pid_start = line.rfind(" [")?;
    let tail = &line[pid_start + 2..];
    let pid_end = tail.find(']')?;
    let pid = tail[..pid_end].parse().ok()?;
    if !tail[pid_end + 1..].trim_start().starts_with(':') {
        return None;
    }
    let name = line[..pid_start].trim();
    (!name.is_empty()).then(|| (pid, name.to_string()))
}

fn parse_footprint_processes(output: &str) -> Vec<ProcessMemory> {
    let mut processes = Vec::new();
    let mut current = None;
    let mut bytes = None;

    for line in output.lines() {
        if let Some(header) = parse_footprint_header(line) {
            if let (Some((pid, name)), Some(bytes)) = (current.take(), bytes.take()) {
                processes.push(ProcessMemory {
                    pid,
                    name,
                    bytes,
                    application: None,
                });
            }
            current = Some(header);
        } else if bytes.is_none() {
            bytes = parse_phys_footprint(line);
        }
    }

    if let (Some((pid, name)), Some(bytes)) = (current, bytes) {
        processes.push(ProcessMemory {
            pid,
            name,
            bytes,
            application: None,
        });
    }
    processes
}

fn parse_process_ids_for_real_uid(output: &str, real_uid: u32) -> Vec<u32> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let process_uid = fields.next()?.parse::<u32>().ok()?;
            let pid = fields.next()?.parse::<u32>().ok()?;
            (process_uid == real_uid).then_some(pid)
        })
        .collect()
}

/// 実行ファイルのパスから最上位の`.app`バンドルを返す。
fn outermost_app_bundle(executable_path: &Path) -> Option<PathBuf> {
    let mut bundle = PathBuf::new();
    for component in executable_path.components() {
        bundle.push(component.as_os_str());
        if component.as_os_str().to_string_lossy().ends_with(".app") {
            return Some(bundle);
        }
    }
    None
}

fn display_name_for_bundle(path: &Path) -> String {
    file_manager_display_name(path)
        .and_then(|display_name| {
            Path::new(&display_name)
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.strip_suffix(".app").unwrap_or(name).to_string())
        })
        .filter(|name| !name.is_empty())
        .or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.strip_suffix(".app").unwrap_or(name).to_string())
        })
        .unwrap_or_else(|| "Unknown app".to_string())
}

fn is_system_path(path: &Path) -> bool {
    [
        "/System",
        "/usr/libexec",
        "/usr/sbin",
        "/usr/bin",
        "/sbin",
        "/bin",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}

fn readable_process_name(name: &str, pid: u32, path: Option<&Path>) -> String {
    let name = name.rsplit('/').next().unwrap_or(name);
    let category = if path.is_some_and(is_system_path) {
        "macOS"
    } else {
        "Process"
    };
    format!("{category} · {name} (PID {pid})")
}

fn attribute_processes(processes: &mut [ProcessMemory]) {
    let mut app_names = HashMap::<PathBuf, String>::new();
    for process in processes {
        let path = process_executable_path(process.pid).map(PathBuf::from);
        let app_path = path.as_deref().and_then(outermost_app_bundle);
        if let Some(app_path) = app_path {
            let name = app_names
                .entry(app_path.clone())
                .or_insert_with(|| display_name_for_bundle(&app_path))
                .clone();
            process.application = Some(AppBundle {
                path: app_path,
                name,
            });
        } else {
            process.name = readable_process_name(&process.name, process.pid, path.as_deref());
        }
    }
}

/// アプリごとに合算し、合算後のメモリ使用量順で上位3件に絞る。
fn top_three(processes: Vec<ProcessMemory>) -> Vec<MemoryConsumer> {
    #[derive(Hash, PartialEq, Eq)]
    enum GroupKey {
        Application(PathBuf),
        Process(u32),
    }

    let mut consumers = HashMap::<GroupKey, MemoryConsumer>::new();
    for process in processes {
        let (key, name) = match process.application {
            Some(application) => (GroupKey::Application(application.path), application.name),
            None => (GroupKey::Process(process.pid), process.name),
        };
        let consumer = consumers.entry(key).or_insert_with(|| MemoryConsumer {
            name,
            bytes: 0,
            process_count: 0,
        });
        consumer.bytes = consumer.bytes.saturating_add(process.bytes);
        consumer.process_count += 1;
    }

    let mut consumers = consumers.into_values().collect::<Vec<_>>();
    consumers.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.name.cmp(&right.name))
    });
    consumers.truncate(3);
    consumers
}

/// 現在のユーザーのプロセスから、アプリ単位のphysical footprint上位3件を返す。
/// 取得に失敗した場合は`None`を返す。一括取得はポップアップ専用の別スレッドで実行し、
/// ポップアップ終了時にキャンセルする。
pub fn scan_top_three(cancel: Option<&AtomicBool>) -> Option<Vec<MemoryConsumer>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ps_command = Command::new("ps");
    ps_command
        .args(["-axo", "ruid=,pid="])
        .stdout(Stdio::piped());
    let ps_child = spawn_with_suppressed_stderr(ps_command)?;
    let (ps_status, ps_output) = wait_for_child(ps_child, deadline, cancel)?;
    if !ps_status.success() {
        return None;
    }
    let pids =
        parse_process_ids_for_real_uid(&String::from_utf8_lossy(&ps_output), current_real_uid());
    if pids.is_empty() {
        return Some(Vec::new());
    }

    let mut command = Command::new("footprint");
    command.args(["--noCategories", "--format", "bytes"]);
    for pid in pids {
        command.arg("--pid").arg(pid.to_string());
    }
    command.stdout(Stdio::piped());
    let child = spawn_with_suppressed_stderr(command)?;
    let (status, output) = wait_for_child(child, deadline, cancel)?;
    let mut processes = parse_footprint_processes(&String::from_utf8_lossy(&output));
    if processes.is_empty() && !status.success() {
        return None;
    }
    attribute_processes(&mut processes);
    Some(top_three(processes))
}

fn current_real_uid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    // SAFETY: getuid reads the current process's real user ID and has no preconditions.
    unsafe { getuid() }
}

#[cfg(target_os = "macos")]
fn process_executable_path(pid: u32) -> Option<String> {
    macos::process_executable_path(pid)
}

#[cfg(not(target_os = "macos"))]
fn process_executable_path(_pid: u32) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn file_manager_display_name(path: &Path) -> Option<String> {
    macos::file_manager_display_name(path)
}

#[cfg(not(target_os = "macos"))]
fn file_manager_display_name(_path: &Path) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        ffi::{c_char, c_void, CStr, CString},
        os::unix::ffi::OsStrExt,
        path::Path,
    };

    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_pidpath(pid: i32, buffer: *mut c_void, buffersize: u32) -> i32;
    }

    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {}

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
        fn objc_autoreleasePoolPush() -> *mut c_void;
        fn objc_autoreleasePoolPop(context: *mut c_void);
    }

    struct AutoreleasePool(*mut c_void);

    impl Drop for AutoreleasePool {
        fn drop(&mut self) {
            unsafe { objc_autoreleasePoolPop(self.0) };
        }
    }

    pub(super) fn process_executable_path(pid: u32) -> Option<String> {
        let mut buffer = vec![0_u8; PROC_PIDPATHINFO_MAXSIZE];
        // SAFETY: `buffer` is writable and has the size supplied to `proc_pidpath`.
        let length = unsafe {
            proc_pidpath(
                i32::try_from(pid).ok()?,
                buffer.as_mut_ptr().cast(),
                u32::try_from(buffer.len()).ok()?,
            )
        };
        if length <= 0 {
            return None;
        }
        // SAFETY: proc_pidpath writes a null-terminated path into the supplied buffer.
        let path = unsafe { CStr::from_ptr(buffer.as_ptr().cast()) };
        Some(path.to_string_lossy().into_owned())
    }

    pub(super) fn file_manager_display_name(path: &Path) -> Option<String> {
        let pool = AutoreleasePool(unsafe { objc_autoreleasePoolPush() });
        let class_name = CString::new("NSFileManager").ok()?;
        let default_manager = CString::new("defaultManager").ok()?;
        let string_class_name = CString::new("NSString").ok()?;
        let string_with_utf8 = CString::new("stringWithUTF8String:").ok()?;
        let display_name_at_path = CString::new("displayNameAtPath:").ok()?;
        let utf8_string = CString::new("UTF8String").ok()?;
        let path = CString::new(path.as_os_str().as_bytes()).ok()?;

        let manager_class = unsafe { objc_getClass(class_name.as_ptr()) };
        let string_class = unsafe { objc_getClass(string_class_name.as_ptr()) };
        if manager_class.is_null() || string_class.is_null() {
            drop(pool);
            return None;
        }

        let manager = send0(manager_class, selector(&default_manager));
        let path_string = send_string(string_class, selector(&string_with_utf8), path.as_ptr());
        if manager.is_null() || path_string.is_null() {
            drop(pool);
            return None;
        }
        let name = send_object(manager, selector(&display_name_at_path), path_string);
        if name.is_null() {
            drop(pool);
            return None;
        }
        let utf8 = send0(name, selector(&utf8_string)).cast::<c_char>();
        if utf8.is_null() {
            drop(pool);
            return None;
        }
        // SAFETY: NSString's UTF8String returns a null-terminated buffer valid for the pool scope.
        let result = unsafe { CStr::from_ptr(utf8) }
            .to_string_lossy()
            .into_owned();
        drop(pool);
        Some(result)
    }

    fn selector(name: &CString) -> *mut c_void {
        unsafe { sel_registerName(name.as_ptr()) }
    }

    fn send0(receiver: *mut c_void, selector: *mut c_void) -> *mut c_void {
        // SAFETY: The selector's return type is an Objective-C object pointer.
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { send(receiver, selector) }
    }

    fn send_string(
        receiver: *mut c_void,
        selector: *mut c_void,
        value: *const c_char,
    ) -> *mut c_void {
        // SAFETY: NSString's class method accepts a null-terminated UTF-8 string.
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void =
            unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { send(receiver, selector, value) }
    }

    fn send_object(
        receiver: *mut c_void,
        selector: *mut c_void,
        value: *mut c_void,
    ) -> *mut c_void {
        // SAFETY: NSFileManager's selector accepts an NSString object pointer.
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void =
            unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { send(receiver, selector, value) }
    }
}

/// 補助プロセスの診断をポップアップの端末へ継承せずに起動する。
fn spawn_with_suppressed_stderr(mut command: Command) -> Option<Child> {
    command.stderr(Stdio::null()).spawn().ok()
}

fn wait_for_child(
    mut child: Child,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Option<(ExitStatus, Vec<u8>)> {
    let mut stdout = child.stdout.take()?;
    if set_nonblocking(stdout.as_raw_fd()).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    let mut output = Vec::new();
    loop {
        // パイプが満杯になると子プロセスが書き込みで停止し得るため、
        // try_waitだけでなく読み取り可能なstdoutを毎回排出する。
        if drain_available(&mut stdout, &mut output).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        if cancel.is_some_and(|flag| !flag.load(Ordering::Acquire)) || Instant::now() >= deadline {
            // kill後にwaitまで行い、キャンセル・timeout時にも子プロセスを残さない。
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = drain_available(&mut stdout, &mut output);
                return Some((status, output));
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn drain_available(reader: &mut impl Read, output: &mut Vec<u8>) -> io::Result<()> {
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => output.extend_from_slice(&buffer[..read]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

fn set_nonblocking(fd: i32) -> io::Result<()> {
    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;
    const O_NONBLOCK: i32 = 0x0004;
    unsafe extern "C" {
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    // SAFETY: `fd` is the live stdout descriptor owned by the child process handle.
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl receives the descriptor, command, and integer flags expected by macOS.
    if unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    /// footprint出力からphysical footprintの数値を取り出す。
    // why: footprint出力からメモリ量を正しく読み取るため。
    fn parses_physical_footprint() {
        assert_eq!(
            parse_phys_footprint("pid: 42\nphys_footprint: 1,234,567\n"),
            Some(1_234_567)
        );
    }

    #[test]
    /// macOSのfootprintが出力する単位付きphysical footprintをバイト数へ変換する。
    // why: macOSの単位付き値をバイト数へ換算するため。
    fn parses_physical_footprint_with_macos_unit() {
        assert_eq!(
            parse_phys_footprint("phys_footprint: 1,712 KB\n"),
            Some(1_753_088)
        );
    }

    #[test]
    /// `footprint`の出力から複数プロセスの名前とphysical footprintを読み取る。
    // why: 複数プロセスのPID・名前・値の対応が混ざらないため。
    fn parses_multiple_processes_from_footprint_snapshot() {
        let output = "\
======================================================================
Google Chrome Helper (Renderer) [101]: 64-bit Footprint: 1,250,000 bytes
======================================================================
Auxiliary data:
    phys_footprint: 1,250,000
======================================================================
Safari [201]: 64-bit Footprint: 2,500,000 bytes
======================================================================
Auxiliary data:
    phys_footprint: 2,500,000
";
        let processes = parse_footprint_processes(output);

        assert_eq!(
            processes
                .iter()
                .map(|process| (process.pid, process.name.as_str(), process.bytes))
                .collect::<Vec<_>>(),
            vec![
                (101, "Google Chrome Helper (Renderer)", 1_250_000),
                (201, "Safari", 2_500_000),
            ]
        );
    }

    #[test]
    /// 入れ子の補助プロセスも最上位のアプリバンドルに所属させる。
    // why: 入れ子のhelperを親アプリへ正しくまとめるため。
    fn resolves_nested_helper_to_outer_application_bundle() {
        let executable = Path::new(
            "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Helper",
        );
        assert_eq!(
            outermost_app_bundle(executable),
            Some(PathBuf::from("/Applications/Google Chrome.app"))
        );
    }

    #[test]
    /// `footprint`の対象を現在のユーザーのプロセスだけに絞る。
    // why: 実行ユーザー以外のPIDを集計へ混ぜないため。
    fn parses_only_current_user_process_ids() {
        assert_eq!(
            parse_process_ids_for_real_uid("501 101\n502 202\n501 303\ninvalid\n", 501),
            vec![101, 303]
        );
    }

    #[test]
    /// アプリバンドル外のシステムプロセスとコマンドを区別して表示する。
    // why: アプリ外の項目をアプリと誤認しないため。
    fn labels_unbundled_system_and_command_processes() {
        assert_eq!(
            readable_process_name(
                "WindowServer",
                77,
                Some(Path::new("/System/Library/PrivateFrameworks/WindowServer"))
            ),
            "macOS · WindowServer (PID 77)"
        );
        assert_eq!(
            readable_process_name("python3", 88, Some(Path::new("/opt/homebrew/bin/python3"))),
            "Process · python3 (PID 88)"
        );
    }

    #[test]
    /// アプリバンドルの表示名から`.app`拡張子を取り除く。
    // why: 画面上のアプリ名に不要な拡張子を出さないため。
    fn application_display_name_omits_app_extension() {
        assert_eq!(
            display_name_for_bundle(Path::new("/Applications/Example App.app")),
            "Example App"
        );
    }

    #[test]
    /// physical footprintがない出力は取得不能として扱う。
    // why: 値がないときにメモリ量を推測して表示しないため。
    fn missing_physical_footprint_is_unavailable() {
        assert_eq!(parse_phys_footprint("resident_size: 123\n"), None);
    }

    #[test]
    /// footprintの大きい順に並べ、表示対象を最大3件に制限する。
    // why: メモリ量の順位と表示件数の上限を守るため。
    fn sorts_and_limits_to_three_processes() {
        let processes = (0..4)
            .map(|index| ProcessMemory {
                pid: index,
                name: format!("p{index}"),
                bytes: index as u64 * 10,
                application: None,
            })
            .collect();
        let top = top_three(processes);
        assert_eq!(
            top.iter()
                .map(|consumer| consumer.name.as_str())
                .collect::<Vec<_>>(),
            vec!["p3", "p2", "p1"]
        );
    }

    #[test]
    /// 補助プロセスを含む同一アプリのメモリ量を合算してから上位3件を選ぶ。
    // why: 合算前にhelperが上位枠を分け合う誤りを防ぐため。
    fn aggregates_app_processes_before_selecting_top_three() {
        let processes = vec![
            ProcessMemory {
                pid: 10,
                name: "Google Chrome".to_string(),
                bytes: 1_000_000_000,
                application: Some(AppBundle {
                    path: PathBuf::from("/Applications/Google Chrome.app"),
                    name: "Google Chrome".to_string(),
                }),
            },
            ProcessMemory {
                pid: 11,
                name: "Google Chrome".to_string(),
                bytes: 1_500_000_000,
                application: Some(AppBundle {
                    path: PathBuf::from("/Applications/Google Chrome.app"),
                    name: "Google Chrome".to_string(),
                }),
            },
            ProcessMemory {
                pid: 12,
                name: "Safari".to_string(),
                bytes: 2_000_000_000,
                application: Some(AppBundle {
                    path: PathBuf::from("/Applications/Safari.app"),
                    name: "Safari".to_string(),
                }),
            },
            ProcessMemory {
                pid: 13,
                name: "Code".to_string(),
                bytes: 1_000_000_000,
                application: Some(AppBundle {
                    path: PathBuf::from("/Applications/Visual Studio Code.app"),
                    name: "Code".to_string(),
                }),
            },
            ProcessMemory {
                pid: 14,
                name: "Slack".to_string(),
                bytes: 500_000_000,
                application: Some(AppBundle {
                    path: PathBuf::from("/Applications/Slack.app"),
                    name: "Slack".to_string(),
                }),
            },
        ];

        let top = top_three(processes);
        assert_eq!(
            top.iter()
                .map(|consumer| {
                    (
                        consumer.name.as_str(),
                        consumer.bytes,
                        consumer.process_count,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("Google Chrome", 2_500_000_000, 2),
                ("Safari", 2_000_000_000, 1),
                ("Code", 1_000_000_000, 1),
            ]
        );
    }

    #[test]
    /// プロセスの入れ替わりで結果が空になってもpanicしない。
    // why: プロセスの入れ替わりで取得項目がなくてもpanicしないため。
    fn process_churn_can_produce_empty_result_without_panicking() {
        assert!(top_three(Vec::new()).is_empty());
    }

    #[test]
    /// 補助プロセスの診断をポップアップの標準エラーへ漏らさない。
    // why: 補助コマンドの診断文をポップアップ表示へ漏らさないため。
    fn child_diagnostics_do_not_escape_popup() {
        const CHILD_ENV: &str = "HERDR_ACTIVITY_MONITOR_STDERR_CHILD";
        const DIAGNOSTIC: &str = "herdr-test-child-diagnostic";
        if std::env::var_os(CHILD_ENV).is_some() {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "printf herdr-test-child-diagnostic >&2"]);
            let mut child = spawn_with_suppressed_stderr(command).expect("child must start");
            assert!(child.wait().expect("child must finish").success());
            return;
        }

        let output = Command::new(std::env::current_exe().expect("test binary path"))
            .args([
                "--exact",
                "processes::tests::child_diagnostics_do_not_escape_popup",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .output()
            .expect("child test must run");

        assert!(output.status.success(), "child test failed: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains(DIAGNOSTIC),
            "child diagnostics must not reach the popup: {stderr}"
        );
    }
}
