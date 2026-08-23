/// バッテリーの I/O 層（`pmset`）。
mod battery;
/// メモリ・バッテリーの I/O 層（`sysctl` / `vm_stat`）。
mod memory;
/// 純粋なパース・計算・出力整形（テスト・ファズの主対象）。
mod parsers;
/// 生 TUI（crossterm、3 秒更新）。
mod tui;

/// プログラム入口。TUI を起動する。
fn main() -> std::io::Result<()> {
    tui::run()
}
