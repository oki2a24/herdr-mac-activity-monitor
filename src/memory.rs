//! メモリ使用量の I/O 層。`sysctl` / `vm_stat` を呼出、
//! 純粋な解析・計算 (`crate::parsers`) で `MemInfo` を返す。
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
fn sysctl_uint<F>(name: &str, run_command: &mut F) -> Result<u64, String>
where
    F: FnMut(&str, &[&str]) -> Result<String, String>,
{
    let out = run_command("sysctl", &["-n", name])?;
    out.trim()
        .parse::<u64>()
        .map_err(|e| format!("parse {name}: {e}"))
}

/// `hw.pagesize` と `hw.memsize` と `vm_stat` から使用済みメモリを算出する。
/// 失敗時はエラー。10進 GB 単位で Activity Monitor と同じ値を返す。
///
/// 外部コマンドを起動できない場合や、出力を解析できない場合はエラーを返す。
pub fn get() -> Result<crate::parsers::MemInfo, String> {
    get_with(run)
}

fn get_with<F>(mut run_command: F) -> Result<crate::parsers::MemInfo, String>
where
    F: FnMut(&str, &[&str]) -> Result<String, String>,
{
    let psize = sysctl_uint("hw.pagesize", &mut run_command)?;
    let total_bytes = sysctl_uint("hw.memsize", &mut run_command)?;
    let total_pages = total_bytes / psize;
    let raw = run_command("vm_stat", &[]).map_err(|e| format!("vm_stat: {e}"))?;
    let vm = parse_vm_stat(&raw)?;
    compute_memory(total_pages, psize, vm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// sysctlとvm_statの値からメモリ情報を算出し、必要なコマンドを順に呼ぶ。
    // why: 取得元の値とページ換算が正しいメモリ表示につながるため。
    fn get_computes_memory_from_system_command_outputs() {
        let mut commands = Vec::new();
        let memory = get_with(|command, args| {
            commands.push((command.to_string(), args.join(" ")));
            match (command, args) {
                ("sysctl", ["-n", "hw.pagesize"]) => Ok("4096\n".to_string()),
                ("sysctl", ["-n", "hw.memsize"]) => Ok("4096000\n".to_string()),
                ("vm_stat", []) => Ok("Pages free: 10.\n\
                     Pages inactive: 20.\n\
                     Pages speculative: 30.\n\
                     Pages purgeable: 40.\n"
                    .to_string()),
                _ => panic!("unexpected command: {command} {args:?}"),
            }
        })
        .unwrap();

        assert_eq!(memory.percent, 90);
        assert!((memory.used_gb - 0.0036864).abs() < f64::EPSILON);
        assert!((memory.total_gb - 0.004096).abs() < f64::EPSILON);
        assert_eq!(
            commands,
            vec![
                ("sysctl".to_string(), "-n hw.pagesize".to_string()),
                ("sysctl".to_string(), "-n hw.memsize".to_string()),
                ("vm_stat".to_string(), "".to_string()),
            ]
        );
    }

    #[test]
    /// sysctlの不正値とvm_stat実行失敗をエラーとして返す。
    // why: 取得失敗をゼロ値などの正常なメモリ情報と誤認しないため。
    fn get_propagates_parse_and_command_errors() {
        let parse_error = get_with(|_, _| Ok("not-a-number".to_string())).unwrap_err();
        assert!(parse_error.starts_with("parse hw.pagesize:"));

        let command_error = get_with(|command, args| match (command, args) {
            ("sysctl", ["-n", "hw.pagesize"]) => Ok("4096".to_string()),
            ("sysctl", ["-n", "hw.memsize"]) => Ok("4096000".to_string()),
            ("vm_stat", []) => Err("offline".to_string()),
            _ => panic!("unexpected command: {command} {args:?}"),
        })
        .unwrap_err();
        assert_eq!(command_error, "vm_stat: offline");
    }

    #[test]
    /// 実行できない外部コマンドをエラーにする。
    // why: コマンド起動失敗を空の出力として扱い解析へ進めないため。
    fn run_reports_a_missing_executable() {
        let command = std::env::temp_dir().join(format!(
            "am-missing-command-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let command = command.to_string_lossy().into_owned();

        let error = run(&command, &[]).unwrap_err();

        assert!(error.starts_with(&format!("{command} failed:")));
    }
}
