use std::{
    fs,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn wait_for_exit(mut child: Child) -> (bool, Output) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut finished_before_deadline = false;
    while Instant::now() < deadline {
        if child
            .try_wait()
            .expect("child status should be readable")
            .is_some()
        {
            finished_before_deadline = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    if !finished_before_deadline {
        let _ = child.kill();
    }
    let output = child
        .wait_with_output()
        .expect("child output should be available");
    (finished_before_deadline, output)
}

#[test]
/// `--watch`を指定すると実バイナリがwatch処理を起動する。
// why: mainの引数取得やモード委譲が抜けて通常起動へ変わる回帰を防ぐため。
fn watch_argument_runs_the_watch_entrypoint() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let state_dir = std::env::temp_dir().join(format!(
        "herdr-main-entrypoint-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&state_dir).expect("temporary state directory should be created");
    let pid_file = state_dir.join("watch.pid");
    fs::write(&pid_file, "0").expect("stale PID should be written");
    let socket_path = state_dir.join("missing.sock");

    let child = Command::new(env!("CARGO_BIN_EXE_herdr-activity-monitor"))
        .arg("--watch")
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .env("HERDR_SOCKET_PATH", &socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("application binary should start");
    let child_pid = child.id().to_string();
    let (finished_before_deadline, output) = wait_for_exit(child);
    let recorded_pid = fs::read_to_string(&pid_file).ok();
    let _ = fs::remove_dir_all(&state_dir);

    assert!(
        finished_before_deadline,
        "watch mode should exit after the missing socket is detected: {output:?}"
    );
    assert!(output.status.success(), "child process failed: {output:?}");
    assert_eq!(recorded_pid.as_deref(), Some(child_pid.as_str()));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("warn: failed to set window title").count(),
        1
    );
}

#[test]
/// 端末が使えない環境では通常起動を成功扱いにしない。
// why: TUIの端末初期化が抜けて起動したように見える回帰を防ぐため。
fn default_entrypoint_reports_missing_terminal() {
    let child = Command::new(env!("CARGO_BIN_EXE_herdr-activity-monitor"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("application binary should start");
    let (finished_before_deadline, output) = wait_for_exit(child);

    assert!(
        finished_before_deadline,
        "TUI startup should reject a missing terminal instead of waiting for input"
    );
    assert!(
        !output.status.success(),
        "default launch should report that the terminal is unavailable"
    );
}
