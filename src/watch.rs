//! window title 常時更新の watch デーモン。
 //! タイトル操作に集中し、`memory` / `battery` / `parsers` / `herdr` を再利用する。
use std::thread;
use std::time::Duration;

/// 接続断（サーバ停止）または連続失敗閾値超過で終了判定する。
/// `socket_exists == false` は即終了。`socket_exists == true` なら
/// `failed_count > max_fail` のとき終了、そうでなければ継続。
pub fn should_exit(failed_count: u32, max_fail: u32, socket_exists: bool) -> bool {
    if !socket_exists {
        return true;
    }
    failed_count > max_fail
}

#[cfg(test)]
mod tests {
    use super::*;

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
}