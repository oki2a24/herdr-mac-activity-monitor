//! ウィンドウタイトルを常時更新する監視プロセス。
//! タイトル操作に集中し、`memory` / `battery` / `parsers` / `herdr`を再利用する。
use std::fs;

/// 接続断（サーバ停止）または連続失敗閾値超過で終了判定する。
/// `socket_exists == false` は即終了。`socket_exists == true` なら
/// `failed_count > max_fail` のとき終了、そうでなければ継続。
pub fn should_exit(failed_count: u32, max_fail: u32, socket_exists: bool) -> bool {
    if !socket_exists {
        return true;
    }
    failed_count > max_fail
}

fn record_title_update_result(failed_count: &mut u32, succeeded: bool) {
    if succeeded {
        *failed_count = 0;
    } else {
        *failed_count += 1;
    }
}

/// 与えられた pid が生きていれば `true`。`pid == 0` は `false`。
/// Unix では `kill -0 <pid>` の結果で判定する（シグナルは実際に送らない）。
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let status = std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
        .map(|o| o.status.success());
    status.unwrap_or(false)
}

/// 監視プロセスの排他ロックを取得する。
/// `HERDR_PLUGIN_STATE_DIR` が未設定なら、`true` を返してガードなしで動作する。
/// 設定されていれば `<state_dir>/watch.pid` を操作して排他制御を行う。
pub fn acquire_watch_lock() -> bool {
    let dir = match std::env::var_os("HERDR_PLUGIN_STATE_DIR") {
        Some(d) => d,
        None => return true,
    };
    let pid_file = std::path::Path::new(&dir).join("watch.pid");
    let raw = fs::read_to_string(&pid_file);
    match raw {
        Ok(_) => {
            let existing: u32 = raw.ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            if is_pid_alive(existing) {
                return false;
            }
            fs::write(&pid_file, std::process::id().to_string()).is_ok()
        }
        Err(_) => fs::write(&pid_file, std::process::id().to_string()).is_ok(),
    }
}

/// 監視プロセス本体。PIDロックを取得できなければすぐに終了する。
/// 以降は 3 秒周期で タイトルを更新し、接続断または連続失敗で終了する。
pub fn run() -> std::io::Result<()> {
    use std::thread;
    if !acquire_watch_lock() {
        return Ok(());
    }
    let mut failed: u32 = 0;
    loop {
        let mem = crate::memory::get().ok();
        let batt = crate::battery::get();
        let title = crate::parsers::format_window_title(&mem, &batt);
        match crate::herdr::set_window_title(&title) {
            Ok(()) => record_title_update_result(&mut failed, true),
            Err(e) => {
                record_title_update_result(&mut failed, false);
                eprintln!("warn: failed to set window title: {e}");
                let socket =
                    std::env::var_os("HERDR_SOCKET_PATH").and_then(|p| fs::metadata(p).ok());
                if should_exit(failed, 3, socket.is_some()) {
                    break;
                }
            }
        }
        thread::sleep(std::time::Duration::from_secs(3));
    }
    // 接続断・連続失敗でループを終了したら、セットしていた window title をクリアする。
    crate::herdr::clear_window_title().ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // HERDR_PLUGIN_STATE_DIR はプロセス全体に共有されるため、
    // 3つのテストを直列化してデータを競合させない。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    /// ソケットが消えた場合は失敗回数に関係なく終了する。
    // why: 接続先がなくなった後に監視を続けないため。
    fn exits_when_socket_gone() {
        assert!(should_exit(0, 5, false));
        assert!(should_exit(3, 5, false));
    }

    #[test]
    /// ソケットがあり、失敗回数が閾値以下なら継続する。
    // why: 失敗回数が許容範囲なら監視を早期終了しないため。
    fn continues_when_socket_present_and_under_threshold() {
        assert!(!should_exit(0, 5, true));
        assert!(!should_exit(5, 5, true));
    }

    #[test]
    /// ソケットがあり、失敗回数が閾値を超えた場合は終了する。
    // why: 連続失敗が上限を超えたら監視を終了するため。
    fn exits_when_socket_present_and_over_threshold() {
        assert!(should_exit(6, 5, true));
        assert!(should_exit(100, 5, true));
    }

    #[test]
    /// window title更新の失敗ごとに連続失敗回数を1ずつ増やす。
    // why: 閾値判定に渡す回数を失敗数と一致させるため。
    fn failed_title_update_increments_failure_count_once() {
        let mut failed = 0;

        record_title_update_result(&mut failed, false);
        assert_eq!(failed, 1);
        record_title_update_result(&mut failed, false);
        assert_eq!(failed, 2);
    }

    #[test]
    /// window title更新に成功したら連続失敗回数をゼロへ戻す。
    // why: 過去の失敗を現在の連続失敗として数え続けないため。
    fn successful_title_update_resets_failure_count() {
        let mut failed = 2;

        record_title_update_result(&mut failed, true);

        assert_eq!(failed, 0);
    }

    #[test]
    /// ロック取得後にタイトル更新が失敗し、ソケット不在なら監視を終了する。
    // why: ロック成功時の本体実行と接続断時の終了・後始末を確認するため。
    fn run_exits_after_title_update_fails_without_socket() {
        const CHILD_ENV: &str = "HERDR_WATCH_RUN_CHILD";
        if env::var_os(CHILD_ENV).is_some() {
            run().expect("watch should exit cleanly when the socket is absent");
            return;
        }

        use std::env::temp_dir;
        let dir = temp_dir().join(format!("am-watch-run-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("state directory must be created");
        let socket = dir.join("missing.sock");
        let mut child = std::process::Command::new(
            std::env::current_exe().expect("test binary path must be available"),
        )
        .args([
            "--exact",
            "watch::tests::run_exits_after_title_update_fails_without_socket",
            "--nocapture",
        ])
        .env(CHILD_ENV, "1")
        .env("HERDR_PLUGIN_STATE_DIR", &dir)
        .env("HERDR_SOCKET_PATH", &socket)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("child test process must start");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut finished_before_deadline = false;
        while std::time::Instant::now() < deadline {
            if child
                .try_wait()
                .expect("child test status must be readable")
                .is_some()
            {
                finished_before_deadline = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !finished_before_deadline {
            let _ = child.kill();
        }
        let output = child
            .wait_with_output()
            .expect("child test output must be available");
        let _ = fs::remove_dir_all(&dir);

        assert!(
            finished_before_deadline,
            "watch must stop instead of looping on a missing socket: {output:?}"
        );
        assert!(output.status.success(), "child test failed: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.matches("warn: failed to set window title").count(),
            1,
            "run should attempt one title update before exiting: {stderr}"
        );
    }

    #[test]
    /// 現在のプロセスIDを生存中として検出する。
    // why: 現在のプロセスを有効なPIDとして認識するため。
    fn current_process_alive() {
        let pid = std::process::id();
        assert!(is_pid_alive(pid));
    }

    #[test]
    /// PID 0を実プロセスの生存確認対象にせずfalseとする。
    // why: PID 0を通常プロセスの生存確認に使わないため。
    fn dead_pid_not_alive() {
        // 非常に高確率で未使用の pid。確実に死んだ pid は取得できないため
        // `kill -0` の挙動（no such process）を信じて false を期待する。
        assert!(!is_pid_alive(0));
    }

    #[test]
    /// 状態ディレクトリ未設定時はロックなしで起動を許可する。
    // why: 状態ディレクトリが未設定でも起動を妨げないため。
    fn unsets_env_returns_true() {
        let _g = ENV_LOCK.lock().unwrap();
        env::remove_var("HERDR_PLUGIN_STATE_DIR");
        assert!(acquire_watch_lock());
    }

    #[test]
    /// PIDファイルがない場合は現在のPIDでロックを作成する。
    // why: ロックがない場合に現在のプロセスが取得できるため。
    fn acquires_when_no_existing_pid() {
        use std::env::temp_dir;
        let _g = ENV_LOCK.lock().unwrap();
        let dir = temp_dir().join(format!("am-watch-t3-{}", std::process::id()));
        env::set_var("HERDR_PLUGIN_STATE_DIR", &dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(acquire_watch_lock());
        let pid_file = dir.join("watch.pid");
        assert!(pid_file.exists());
        env::remove_var("HERDR_PLUGIN_STATE_DIR");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    /// 生存中のPIDが記録されたロックは二重起動を拒否する。
    // why: 既存の監視プロセスとの二重起動を防ぐため。
    fn blocks_when_live_pid_present() {
        use std::env::temp_dir;
        let _g = ENV_LOCK.lock().unwrap();
        let dir = temp_dir().join(format!("am-watch-t3b-{}", std::process::id()));
        env::set_var("HERDR_PLUGIN_STATE_DIR", &dir);
        fs::create_dir_all(&dir).unwrap();
        // 生 pid を仮置き（自 pid）。
        fs::write(dir.join("watch.pid"), std::process::id().to_string()).unwrap();
        assert!(!acquire_watch_lock());
        // 死者 pid（pid 0）なら上書きできる。
        fs::write(dir.join("watch.pid"), "0").unwrap();
        assert!(acquire_watch_lock());
        env::remove_var("HERDR_PLUGIN_STATE_DIR");
        let _ = fs::remove_dir_all(&dir);
    }
}
