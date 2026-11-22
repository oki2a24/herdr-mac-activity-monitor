/// バッテリー情報の入出力層（IOPowerSources）。
mod battery;
mod herdr;
/// メモリ・バッテリーの I/O 層（`sysctl` / `vm_stat`）。
mod memory;
/// ポップアップ専用の詳細情報収集器と表示形式。
mod metrics;
/// 純粋なパース・計算・出力整形（テスト・ファズの主対象）。
mod parsers;
/// ポップアップに表示する、物理メモリ使用量の大きいプロセスの取得。
mod processes;
/// 端末上の対話表示（crossterm、3秒ごとに更新）。
mod tui;
mod watch;

/// プログラムの入口。`--watch`指定時は監視プロセス、それ以外は端末表示を起動する。
fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    run_with_args(&args, watch::run, tui::run)
}

fn run_with_args<W, T>(args: &[String], run_watch: W, run_tui: T) -> std::io::Result<()>
where
    W: FnOnce() -> std::io::Result<()>,
    T: FnOnce() -> std::io::Result<()>,
{
    if args.iter().any(|a| a == "--watch") {
        run_watch()
    } else {
        run_tui()
    }
}

#[cfg(test)]
mod tests {
    use super::run_with_args;
    use std::cell::Cell;

    #[test]
    /// `--watch` が引数にあればwatch処理だけを起動する。
    // why: 起動モードの指定を誤って通常画面へ流さないため。
    fn routes_watch_flag_to_watch_handler() {
        let args = vec![
            "herdr".to_string(),
            "--verbose".to_string(),
            "--watch".to_string(),
        ];
        let watch_called = Cell::new(false);
        let tui_called = Cell::new(false);

        let result = run_with_args(
            &args,
            || {
                watch_called.set(true);
                Ok(())
            },
            || {
                tui_called.set(true);
                Ok(())
            },
        );

        assert!(result.is_ok());
        assert!(watch_called.get());
        assert!(!tui_called.get());
    }

    #[test]
    /// `--watch` がなければ端末画面を起動する。
    // why: 通常起動時に監視専用モードへ誤って入らないため。
    fn routes_without_watch_flag_to_tui_handler() {
        let args = vec!["herdr".to_string()];
        let watch_called = Cell::new(false);
        let tui_called = Cell::new(false);

        let result = run_with_args(
            &args,
            || {
                watch_called.set(true);
                Ok(())
            },
            || {
                tui_called.set(true);
                Ok(())
            },
        );

        assert!(result.is_ok());
        assert!(!watch_called.get());
        assert!(tui_called.get());
    }
}
