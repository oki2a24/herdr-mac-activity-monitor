//! popup専用のphysical footprint上位プロセス取得。

use std::{
    io::{self, Read},
    os::unix::io::AsRawFd,
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// 1プロセスのphysical footprint情報。
pub struct ProcessMemory {
    /// プロセスID。
    pub pid: u32,
    /// `ps` から得た表示名。
    pub name: String,
    /// physical footprint（バイト単位）。
    pub bytes: u64,
}

/// `footprint` の出力からphysical footprintを取り出す。
pub(crate) fn parse_phys_footprint(output: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        let (label, value) = line.trim().split_once(':')?;
        if label.trim() != "phys_footprint" {
            return None;
        }
        value.trim().replace(',', "").parse().ok()
    })
}

/// `ps -axo pid=,comm=` の1行をPIDと表示名へ変換する。
pub(crate) fn parse_process_line(line: &str) -> Option<(u32, String)> {
    let mut fields = line.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let name = fields.collect::<Vec<_>>().join(" ");
    (!name.is_empty()).then_some((pid, name))
}

/// プロセスをphysical footprintの降順で並べ、最大3件に絞る。
pub(crate) fn top_three(mut processes: Vec<ProcessMemory>) -> Vec<ProcessMemory> {
    processes.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.pid.cmp(&right.pid))
    });
    processes.truncate(3);
    processes
}

/// キャンセル可能なphysical footprint scan。各PIDのsubprocessには上限を設ける。
/// 全PIDを候補にしてphysical footprint取得成功分の上位3件を返す。
/// 個別PIDのpermission failureやprocess churnは全体の失敗にしない。
pub fn scan_top_three(cancel: Option<&AtomicBool>) -> Vec<ProcessMemory> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let output = match Command::new("ps")
        .args(["-axo", "pid=,comm="])
        .stdout(Stdio::piped())
        .spawn()
        .ok()
        .and_then(|child| wait_for_child(child, deadline, cancel))
    {
        Some((status, stdout)) if status.success() => stdout,
        _ => return Vec::new(),
    };

    // `ps`で候補を一括取得し、各PIDの詳細値は`footprint`で読む。
    // `ps`だけではActivity Monitor相当のphysical footprintにならないため、
    // 候補列挙と詳細取得を分けている。
    let mut processes = Vec::new();
    for line in String::from_utf8_lossy(&output).lines() {
        if cancel.is_some_and(|flag| !flag.load(Ordering::Acquire)) {
            break;
        }
        if Instant::now() >= deadline {
            return top_three(processes);
        }
        let Some((pid, name)) = parse_process_line(line) else {
            continue;
        };
        // 1つのPIDがハングして全体のpopup更新を止めないよう、
        // 個別上限とscan全体の上限を併用する。
        let process_deadline = (Instant::now() + Duration::from_millis(250)).min(deadline);
        let Some(bytes) = get_process_footprint(pid, cancel, process_deadline) else {
            continue;
        };
        processes.push(ProcessMemory { pid, name, bytes });
    }
    top_three(processes)
}

fn get_process_footprint(pid: u32, cancel: Option<&AtomicBool>, deadline: Instant) -> Option<u64> {
    let mut command = Command::new("footprint");
    command
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::piped());
    let child = spawn_with_suppressed_stderr(command)?;
    let (status, stdout) = wait_for_child(child, deadline, cancel)?;
    if !status.success() {
        return None;
    }
    parse_phys_footprint(&String::from_utf8_lossy(&stdout))
}

/// 子プロセスの診断を親のpopup端末へ継承せずに起動する。
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
    fn parses_physical_footprint() {
        assert_eq!(
            parse_phys_footprint("pid: 42\nphys_footprint: 1,234,567\n"),
            Some(1_234_567)
        );
    }

    #[test]
    /// physical footprintがない出力は取得不能として扱う。
    fn missing_physical_footprint_is_unavailable() {
        assert_eq!(parse_phys_footprint("resident_size: 123\n"), None);
    }

    #[test]
    /// footprintの降順で並べ、表示対象を最大3件に制限する。
    fn sorts_and_limits_to_three_processes() {
        let processes = (0..4)
            .map(|index| ProcessMemory {
                pid: index,
                name: format!("p{index}"),
                bytes: index as u64 * 10,
            })
            .collect();
        let top = top_three(processes);
        assert_eq!(top.iter().map(|p| p.pid).collect::<Vec<_>>(), vec![3, 2, 1]);
    }

    #[test]
    /// プロセスが入れ替わって空の結果になってもpanicしない。
    fn process_churn_can_produce_empty_result_without_panicking() {
        assert!(top_three(Vec::new()).is_empty());
    }

    #[test]
    /// 子プロセスの診断をpopupの標準エラーへ漏らさない。
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
