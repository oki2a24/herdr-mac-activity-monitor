//! window title 常時更新の watch デーモン。
 //! タイトル操作に集中し、`memory` / `battery` / `parsers` / `herdr` を再利用する。
use std::fs;
use std::env;

/// 接続断（サーバ停止）または連続失敗閾値超過で終了判定する。
/// `socket_exists == false` は即終了。`socket_exists == true` なら
/// `failed_count > max_fail` のとき終了、そうでなければ継続。
pub fn should_exit(failed_count: u32, max_fail: u32, socket_exists: bool) -> bool {
    if !socket_exists {
        return true;
    }
    failed_count > max_fail
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

/// watch デーモンの排他ロックを取得する。
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
            let existing: u32 = raw
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            if is_pid_alive(existing) {
                return false;
            }
            fs::write(&pid_file, std::process::id().to_string()).is_ok()
         }
         Err(_) => {
            fs::write(&pid_file, std::process::id().to_string()).is_ok()
      }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::Path;

    #[test]
    fn exits_when_socket_gone() {
        assert!(should_exit(0, 5, false));
        assert!(should_exit(3, 5, false));
    }

    #[test]
    fn continues_when_socket_present_and_under_threshold() {
        assert!(!should_exit(0, 5, true));
        assert!(!should_exit(5, 5, true));
    }

    #[test]
    fn exits_when_socket_present_and_over_threshold() {
        assert!(should_exit(6, 5, true));
        assert!(should_exit(100, 5, true));
    }

    #[test]
    fn current_process_alive() {
        let pid = std::process::id();
        assert!(is_pid_alive(pid));
    }

    #[test]
    fn dead_pid_not_alive() {
        // 非常に高確率で未使用の pid。確実に死んだ pid は取得できないため
        // `kill -0` の挙動（no such process）を信じて false を期待する。
        assert!(!is_pid_alive(0));
    }

    #[test]
    fn unsets_env_returns_true() {
        env::remove_var("HERDR_PLUGIN_STATE_DIR");
        assert!(acquire_watch_lock());
    }

    #[test]
    fn acquires_when_no_existing_pid() {
        use std::env::temp_dir;
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
    fn blocks_when_live_pid_present() {
        use std::env::temp_dir;
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