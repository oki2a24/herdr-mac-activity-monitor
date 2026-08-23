/// バッテリー状態の I/O 層。`pmset -g batt` を呼出し、
/// 純粋な解析 (`crate::parsers::parse_battery_line`) で `Battery` を返す。
/// 取得失敗時は `Battery::Absent`（プロセスは落ちない）。
use std::process::Command;

use crate::parsers::{parse_battery_line, split_present, Battery};

/// `pmset -g batt` を実行し、`%` と `present` を含む行から `Battery` を取得する。
/// 失敗・バッテリーなし時は `Battery::Absent`。
pub fn get() -> Battery {
    let raw = match Command::new("pmset").arg("-g").arg("batt").output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => return Battery::Absent,
    };
    let line = raw
        .lines()
        .find(|l| l.contains('%') && l.contains("present"))
        .unwrap_or("");
    let present = split_present(line);
    parse_battery_line(line, present).unwrap_or(Battery::Absent)
}
