/// バッテリーの I/O 層（IOPowerSources）。
mod battery;
mod herdr;
/// メモリ・バッテリーの I/O 層（`sysctl` / `vm_stat`）。
mod memory;
/// popup専用の詳細metric collectorと表示契約。
mod metrics;
/// 純粋なパース・計算・出力整形（テスト・ファズの主対象）。
mod parsers;
/// popup専用のphysical footprint上位プロセス取得。
mod processes;
/// 生 TUI（crossterm、3 秒更新）。
mod tui;
mod watch;

/// プログラム入口。--watch で watch デーモン、それ以外で TUI を起動する。
fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--watch") {
        watch::run()
    } else {
        tui::run()
    }
}
