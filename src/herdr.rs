use std::io::Write;
use std::os::unix::net::UnixStream;

/// JSON 文字列リテラルとして安全にエスケープする（`"..."` の中身、クォートは付けない）。
/// バックスラッシュ・ダブルクォート・改行（`\n`, `\r`, `\t`）を `\u00XX` / `\\` / `\"` に変換し、
/// 結果は newline を含まない（1行）を保証する。
fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\u0022"),
            '\\' => out.push_str("\\u005c"),
            c if c.is_ascii_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            other => out.push(other),
        }
    }
    out
}

/// `client.window_title.set` の JSON-RPC リクエスト（1行）を生成する。
pub fn set_request(title: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":"window_title_set","method":"client.window_title.set","params":{{"title":"{}"}}}}"#,
        json_escape(title)
     )
 }

/// `client.window_title.clear` の JSON-RPC リクエスト（1行・空 params）を生成する。
pub fn clear_request() -> String {
    r#"{"jsonrpc":"2.0","id":"window_title_clear","method":"client.window_title.clear","params":{}}"#.to_string()
}

/// `HERDR_SOCKET_PATH` の Unix domain socket に `body` を1行送信する。
/// 環境変数未設定・接続失敗・送信失敗で `Err`、成功で `Ok(())`。
fn send(body: &str) -> Result<(), String> {
    let path = std::env::var_os("HERDR_SOCKET_PATH")
        .ok_or_else(|| "HERDR_SOCKET_PATH not set".to_string())?;
    let mut stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
    writeln!(stream, "{body}").map_err(|e| e.to_string())
}

/// ウィンドウタイトルをセットする。
pub fn set_window_title(title: &str) -> Result<(), String> {
    send(&set_request(title))
}

/// ウィンドウタイトルをクリアする。
pub fn clear_window_title() -> Result<(), String> {
    send(&clear_request())
}

#[cfg(test)]
mod tests {
    use super::*;
     #[test]
    fn set_request_shape() {
        let s = set_request("abc");
        assert!(s.contains("client.window_title.set"));
        assert!(s.contains(r#""id":"window_title_set""#));
        assert!(s.contains("params"));
        assert!(s.contains("abc"));
      }

     #[test]
    fn clear_request_shape() {
        let s = clear_request();
        assert!(s.contains("client.window_title.clear"));
        assert!(s.contains(r#""id":"window_title_clear""#));
        assert!(s.contains("params"));
      }

    #[test]
    fn set_request_escapes_special_chars_single_line() {
        let s = set_request("a\"b\\c\nd");
        assert!(!s.contains('\n'));
        assert!(s.contains("\\u0022"));
        assert!(s.contains("\\u005c"));
        assert!(s.contains("\\u000a"));
    }

    #[test]
    #[allow(unsafe_code, deprecated)]
    fn set_window_title_unset_env_is_err() {
        unsafe {
            std::env::remove_var("HERDR_SOCKET_PATH");
        }
        assert!(set_window_title("x").is_err());
    }

    #[test]
    #[allow(unsafe_code, deprecated)]
    fn clear_window_title_unset_env_is_err() {
        unsafe {
            std::env::remove_var("HERDR_SOCKET_PATH");
        }
        assert!(clear_window_title().is_err());
    }
}
