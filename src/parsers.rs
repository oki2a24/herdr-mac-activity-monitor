/// 各テストモジュールで再利用する、std のみ xorshift64 乱数ジェネレータ。
#[cfg(test)]
pub(crate) struct Lcg(u64);
#[cfg(test)]
impl Lcg {
    /// 奇数シードで固定化し、xorshift64 を初期化する。
    pub(super) fn new(seed: u64) -> Self {
        Lcg(seed | 1)
    }
    /// 次の擬似乱数を生成して返す。
    pub(super) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// 半角英数字・空白・コロンの任意文字列（UTF-8 保証）を生成する。
    pub(super) fn str_n(&mut self, n: usize) -> String {
        const AL: [u8; 12] = *b": 0123456789";
        let len = AL.len() as u64;
        let bytes: Vec<u8> = (0..n).map(|_| AL[(self.next() % len) as usize]).collect();
        String::from_utf8(bytes).unwrap()
    }
}

/// 各メモリカウンタ（ページ数）を保持する構造体。
#[derive(Debug, PartialEq, Eq)]
pub struct VmStat {
    pub free: u64,
    pub inactive: u64,
    pub speculative: u64,
    pub purgeable: u64,
}

/// 充電状態。
#[derive(Debug, PartialEq, Eq)]
pub enum ChargingState {
    Charging,
    Discharging,
    Charged,
}

/// バッテリー状態。`Percent` と `state` を保持するか `Absent`。
#[derive(Debug, PartialEq, Eq)]
pub enum Battery {
    Present { percent: u8, state: ChargingState },
    Absent,
}

/// リンクから `present: false` を判定する。`false` を含む行は `false`、
/// それ以外は `true`（= 接続中・存在する）を返す。
pub fn split_present(line: &str) -> bool {
    !line.contains("present: false")
}

/// `pmset -g batt` の 1 行から `Battery` を抽出する。
/// `present == false` の行は `None`。`%` の直数字列を `u8` として取り、
/// `discharging` / `charging` / 該当しない場合 `Charged` を返す。
pub fn parse_battery_line(line: &str, present: bool) -> Option<Battery> {
    if !present {
        return None;
    }
    // `%` の直前の数字列を取り出す（タブ区切りや id 数字を除外）。
    let before = line.split('%').next()?;
    let percent: u8 = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .parse()
        .ok()?;
    let state = if line.contains("discharging") {
        ChargingState::Discharging
    } else if line.contains("charging") {
        ChargingState::Charging
    } else {
        ChargingState::Charged
    };
    Some(Battery::Present { percent, state })
}

/// 使用済みメモリの情報。10進 GB 単位。
#[derive(Debug, PartialEq)]
pub struct MemInfo {
    pub used_gb: f64,
    pub total_gb: f64,
    pub percent: i32,
}

/// `total_pages` と `psize` と `vm_stat` から使用済みメモリを計算する。
/// `used = (total_pages) - (free+inactive+speculative+purgeable)` (ページ単位)。
/// 中間の合計が u64 をオーバーフローすると、または `used_pages` が負になるときは
/// エラーを返す（panic しない）。
pub fn compute_memory(total_pages: u64, psize: u64, vm: VmStat) -> Result<MemInfo, String> {
    let freed = vm
        .free
        .checked_add(vm.inactive)
        .and_then(|a| a.checked_add(vm.speculative))
        .and_then(|a| a.checked_add(vm.purgeable))
        .ok_or_else(|| "freed overflow".to_string())?;
    let used_pages = total_pages
        .checked_sub(freed)
        .ok_or_else(|| "used_pages underflow".to_string())?;
    let used_bytes = used_pages as u128 * psize as u128;
    let total_bytes = total_pages as u128 * psize as u128;
    let used_gb = used_bytes as f64 / 1e9_f64;
    let total_gb = total_bytes as f64 / 1e9_f64;
    let percent = if total_pages == 0 {
        0
    } else {
        ((used_pages as f64 / total_pages as f64) * 100.0).round() as i32
    };
    Ok(MemInfo {
        used_gb,
        total_gb,
        percent,
    })
}

/// 状態から表示列の `Vec<String>` を生成する。
/// 4行固定: タイトル / 罫線 / Memory / Battery。
/// ラベル(`Memory`/`Battery`)は4スペースで揃え、Battery なしは3スペースの `n/a`。
pub fn format_lines(m: &Option<MemInfo>, b: &Battery) -> Vec<String> {
    let mem_line = match m {
        Some(x) => format!(
            "Memory    {:.2} GB / {:.2} GB {}%",
            x.used_gb, x.total_gb, x.percent
        ),
        None => "Memory   ?".to_string(),
    };
    let batt_line = match b {
        Battery::Present { percent, state } => {
            format!("Battery    {}% {}", percent, state.as_str())
        }
        Battery::Absent => "Battery   n/a".to_string(),
    };
    vec![
        "Activity Monitor".to_string(),
        "─────────────────────────────".to_string(),
        mem_line,
        batt_line,
    ]
}

/// ウィンドウタイトル用の1行文字列を生成する。
/// 形式: "Memory USED/TOTALGB PERCENT% | Battery PERCENT% STATE"。
/// Memory 欠損は "Memory n/a"、Battery 欠損は "... | Battery n/a"。
pub fn format_window_title(m: &Option<crate::parsers::MemInfo>, b: &Battery) -> String {
    let mem_part = match m {
        Some(x) => format!("{:.2}/{:.2}GB {}%", x.used_gb, x.total_gb, x.percent),
        None => "n/a".to_string(),
    };
    let batt_part = match b {
        Battery::Present { percent, state } => {
            format!("{}% {}", percent, state.as_str())
        }
        Battery::Absent => "n/a".to_string(),
    };
    format!("Memory {} | Battery {}", mem_part, batt_part)
}

impl ChargingState {
    /// 充電状態を表す小文字英語の固定文字列を返す。
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            ChargingState::Charging => "charging",
            ChargingState::Discharging => "discharging",
            ChargingState::Charged => "charged",
        }
    }
}

/// 指定された行からコロン直後のページ数を解析する。
/// 実機の `vm_stat` は値末尾に「.」を付帯するため、最初の「.」以前を数値として取り、
/// 数値でない・コロンなしの入力は `None` を返す（panic しない）。
fn parse_pages(line: &str) -> Option<u64> {
    let after = line.split(':').nth(1)?;
    let token = after.split_whitespace().next()?;
    token.split('.').next()?.trim().parse::<u64>().ok()
}

/// `vm_stat` の出力を解析して `VmStat` を返す。
/// 必要な4種（free/inactive/speculative/purgeable）が揃わない場合はエラー。
pub fn parse_vm_stat(s: &str) -> Result<VmStat, String> {
    let mut free = None;
    let mut inactive = None;
    let mut speculative = None;
    let mut purgeable = None;
    for line in s.lines() {
        let t = line.trim_start();
        if t.starts_with("Pages free:") {
            free = parse_pages(t);
        } else if t.starts_with("Pages inactive:") {
            inactive = parse_pages(t);
        } else if t.starts_with("Pages speculative:") {
            speculative = parse_pages(t);
        } else if t.starts_with("Pages purgeable:") {
            purgeable = parse_pages(t);
        }
    }
    match (free, inactive, speculative, purgeable) {
        (Some(f), Some(i), Some(sp), Some(p)) => Ok(VmStat {
            free: f,
            inactive: i,
            speculative: sp,
            purgeable: p,
        }),
        _ => Err("vm_stat parse failed".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pages_free() {
        assert_eq!(parse_pages("Pages free:     152673"), Some(152673));
    }

    #[test]
    fn parse_pages_missing_is_none() {
        assert_eq!(parse_pages("Pages wired down: 1"), Some(1));
        assert_eq!(parse_pages("Pages free: notanumber"), None);
        assert_eq!(parse_pages("NoColonHere"), None);
    }

    #[test]
    fn parses_full_vm_stat() {
        let s = "Page size of 16384 bytes\n\
                 Pages free:     152673\n\
                 Pages inactive: 121622\n\
                 Pages speculative:  2118\n\
                 Pages purgeable:    2168\n\
                 Pages wired down: 1456763\n";
        let v = parse_vm_stat(s).unwrap();
        assert_eq!(
            (v.free, v.inactive, v.speculative, v.purgeable),
            (152673, 121622, 2118, 2168)
        );
    }

    #[test]
    fn parse_vm_stat_missing_is_err() {
        assert!(parse_vm_stat("garbage\n").is_err());
    }

    #[test]
    fn parse_vm_stat_trailing_dot_ok() {
        // 実機の vm_stat は値末尾に「.」を含める
        let raw = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
                       Pages free:                                     49914.\n\
                       Pages inactive:                                127020.\n\
                       Pages speculative:                              24820.\n\
                       Pages purgeable:                                 3656.\n\
                       Pages wired down:                              1667782.\n";
        let v = parse_vm_stat(raw).expect("トレイリングドットを許容");
        assert_eq!(
            (v.free, v.inactive, v.speculative, v.purgeable),
            (49914, 127020, 24820, 3656)
        );
    }

    #[test]
    fn fuzz_parse_pages_never_panic() {
        let mut r = Lcg::new(0x51ed);
        for _ in 0..5000 {
            let n = (r.next() % 32) as usize;
            let s = r.str_n(n);
            let _ = parse_pages(&s); // 不変条件: 任意文字列で panic しない
        }
    }

    #[test]
    fn fuzz_parse_vm_stat_never_panic() {
        let mut r = Lcg::new(0xF00DBA);
        for _ in 0..5000 {
            let n = (r.next() % 64) as usize;
            let s = r.str_n(n);
            let _ = parse_vm_stat(&s); // 不変条件: panic せず Result を返す
        }
    }

    #[test]
    fn fuzz_parse_battery_line_never_panic() {
        let mut r = Lcg::new(0xB41);
        for _ in 0..5000 {
            let n = (r.next() % 48) as usize;
            let s = r.str_n(n);
            let _ = parse_battery_line(&s, true);
            let _ = parse_battery_line(&s, false);
        }
    }

    #[test]
    fn fuzz_compute_memory_bounds() {
        let mut r = Lcg::new(0xD2);
        for _ in 0..5000 {
            let tp = r.next();
            let ps = r.next() | 1;
            let free = r.next();
            let inactive = r.next();
            let speculative = r.next();
            let purgeable = r.next();
            let vm = VmStat {
                free,
                inactive,
                speculative,
                purgeable,
            };
            if let Ok(m) = compute_memory(tp, ps, vm) {
                // 不変条件: 0<=percent<=100, 0<=used_gb<=total_gb
                assert!((0..=100).contains(&m.percent));
                assert!(m.used_gb <= m.total_gb + 1e-6);
                assert!(m.used_gb >= 0.0);
            }
        }
    }

    #[test]
    fn fuzz_format_lines_four_lines_never_panic() {
        let mut r = Lcg::new(0xF4);
        for _ in 0..5000 {
            let m = if r.next().is_multiple_of(2) {
                Some(MemInfo {
                    used_gb: r.next() as f64 / 1e6,
                    total_gb: r.next() as f64 / 1e6,
                    percent: (r.next() % 101) as i32 - 50,
                })
            } else {
                None
            };
            let b = if r.next().is_multiple_of(2) {
                Battery::Present {
                    percent: r.next() as u8,
                    state: ChargingState::Charging,
                }
            } else {
                Battery::Absent
            };
            let lines = format_lines(&m, &b);
            assert_eq!(lines.len(), 4);
        }
    }
}

#[cfg(test)]
mod bt {
    use super::*;
    const L: &str =
        " -InternalBattery-0 (id=22478947) 83%; discharging; 17:55 remaining present: true";

    #[test]
    fn discharging() {
        match parse_battery_line(L, true) {
            Some(Battery::Present { percent, state }) => {
                assert_eq!(percent, 83);
                assert!(matches!(state, ChargingState::Discharging));
            }
            _ => panic!("expected present discharging"),
        }
    }

    #[test]
    fn absent_when_present_false() {
        let l = " -InternalBattery-0 (id=1) 100%; charged; present: false";
        assert_eq!(parse_battery_line(l, false), None);
    }

    #[test]
    fn battery_with_tab_and_id_number() {
        // 実機の pmset はタブ区切りで id 数字を % より前に含む
        let l = " -InternalBattery-0 (id=22478947)\t81%; discharging; 1:42 remaining present: true";
        match parse_battery_line(l, true) {
            Some(Battery::Present { percent, state }) => {
                assert_eq!(percent, 81);
                assert!(matches!(state, ChargingState::Discharging));
            }
            _ => panic!("expected present discharging"),
        }
    }

    #[test]
    fn charging_keyword() {
        let l = " -InternalBattery-0 40%; charging; present: true";
        let b = parse_battery_line(l, true).unwrap();
        assert!(matches!(
            b,
            Battery::Present {
                state: ChargingState::Charging,
                ..
            }
        ));
    }

    #[test]
    fn charged_fallback() {
        let l = " -InternalBattery-0 100%; 5:00 remaining present: true";
        let b = parse_battery_line(l, true).unwrap();
        assert!(matches!(
            b,
            Battery::Present {
                state: ChargingState::Charged,
                ..
            }
        ));
    }
}

#[cfg(test)]
mod mem {
    use super::*;

    #[test]
    fn computes_used_gb_and_percent() {
        // 総ページ=2097152, psize=16384
        let vm = VmStat {
            free: 152673,
            inactive: 121622,
            speculative: 2118,
            purgeable: 2168,
        };
        let m = compute_memory(2097152, 16384, vm).unwrap();
        assert!((m.total_gb - 34.359738368).abs() < 0.01);
        assert!((m.used_gb - 29.80).abs() < 0.05);
        assert_eq!(m.percent, 87);
    }

    #[test]
    fn total_less_than_freed_is_err() {
        let vm = VmStat {
            free: 999999999,
            inactive: 0,
            speculative: 0,
            purgeable: 0,
        };
        assert!(compute_memory(100, 16384, vm).is_err());
    }
}

#[cfg(test)]
mod fmt {
    use super::*;

    #[test]
    fn renders_present_battery() {
        let m = MemInfo {
            used_gb: 29.80,
            total_gb: 34.36,
            percent: 87,
        };
        let b = Battery::Present {
            percent: 83,
            state: ChargingState::Discharging,
        };
        let lines = format_lines(&Some(m), &b);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[2], "Memory    29.80 GB / 34.36 GB 87%");
        assert_eq!(lines[3], "Battery    83% discharging");
    }

    #[test]
    fn renders_memory_missing_as_question() {
        let b = Battery::Present {
            percent: 83,
            state: ChargingState::Charged,
        };
        let lines = format_lines(&None, &b);
        assert_eq!(lines[2], "Memory   ?");
    }

    #[test]
    fn renders_battery_absent_as_na() {
        let m = MemInfo {
            used_gb: 1.0,
            total_gb: 2.0,
            percent: 50,
        };
        let lines = format_lines(&Some(m), &Battery::Absent);
        assert_eq!(lines[3], "Battery   n/a");
    }

    #[test]
    fn present() {
        let m = MemInfo {
            used_gb: 29.80,
            total_gb: 34.36,
            percent: 87,
        };
        let b = Battery::Present {
            percent: 83,
            state: ChargingState::Discharging,
        };
        assert_eq!(
            format_window_title(&Some(m), &b),
            "Memory 29.80/34.36GB 87% | Battery 83% discharging"
        );
    }

    #[test]
    fn memory_missing() {
        let b = Battery::Present {
            percent: 83,
            state: ChargingState::Discharging,
        };
        assert_eq!(
            format_window_title(&None, &b),
            "Memory n/a | Battery 83% discharging"
        );
    }

    #[test]
    fn battery_absent() {
        let m = MemInfo {
            used_gb: 29.80,
            total_gb: 34.36,
            percent: 87,
        };
        assert_eq!(
            format_window_title(&Some(m), &Battery::Absent),
            "Memory 29.80/34.36GB 87% | Battery n/a"
        );
    }
}
