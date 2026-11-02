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
    if args.iter().any(|a| a == "--watch") {
        watch::run()
    } else {
        tui::run()
    }
}
