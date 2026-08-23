/// メモリ使用量の I/O 層。`sysctl` / `vm_stat` を呼出、
/// 純粋な解析・計算 (`crate::parsers`) で `MemInfo` を返す。
use std::process::Command;

use crate::parsers::{compute_memory, parse_vm_stat};

/// 外部コマンドを実行し、標準出力を文字列で返す。エラー時はメッセージ。
fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{cmd} failed: {e}"))?
        .stdout;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// 単一の `sysctl -n <name>` 結果を `u64` にパースする。
fn sysctl_uint(name: &str) -> Result<u64, String> {
    let out = run("sysctl", &["-n", name])?;
    out.trim()
        .parse::<u64>()
        .map_err(|e| format!("parse {name}: {e}"))
}

/// `hw.pagesize` と `hw.memsize` と `vm_stat` から使用済みメモリを算出する。
/// 失敗時はエラー。10進 GB 単位で Activity Monitor と同じ値を返す。
pub fn get() -> Result<crate::parsers::MemInfo, String> {
    let psize = sysctl_uint("hw.pagesize")?;
    let total_bytes = sysctl_uint("hw.memsize")?;
    let total_pages = total_bytes / psize;
    let raw = run("vm_stat", &[]).map_err(|e| format!("vm_stat: {e}"))?;
    let vm = parse_vm_stat(&raw)?;
    compute_memory(total_pages, psize, vm)
}
