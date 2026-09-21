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
    /// 空きページ数。
    pub free: u64,
    /// inactiveページ数。
    pub inactive: u64,
    /// speculativeページ数。
    pub speculative: u64,
    /// purgeableページ数。
    pub purgeable: u64,
}

/// 充電状態。
#[derive(Debug, PartialEq, Eq)]
pub enum ChargingState {
    /// 充電中。
    Charging,
    /// バッテリー駆動で放電中。
    Discharging,
    /// 満充電、または外部電源接続中。
    Charged,
}

/// バッテリー状態。`Percent` と `state` を保持するか `Absent`。
#[derive(Debug, PartialEq, Eq)]
pub enum Battery {
    /// バッテリーが存在し、残量と充電状態を保持する。
    Present { percent: u8, state: ChargingState },
    /// バッテリー情報を取得できない、またはバッテリーが存在しない。
    Absent,
}

/// 使用済みメモリの情報。10進 GB 単位。
#[derive(Debug, PartialEq)]
pub struct MemInfo {
    /// 使用済みメモリ（10進GB）。
    pub used_gb: f64,
    /// 物理メモリ総量（10進GB）。
    pub total_gb: f64,
    /// 使用率（0から100の整数パーセント）。
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
/// 3行固定: タイトル / Memory / Battery。
/// ラベル(`Memory`/`Battery`)は値の開始列を9固定で揃える（Memory 3スペース、Battery 2スペース）。
/// 欠損時も9列に揃い、`?`/`n/a` を表示。
pub fn format_lines(m: &Option<MemInfo>, b: &Battery) -> Vec<String> {
    let mem_line = match m {
        Some(x) => format!(
            "Memory   {:.2} GB / {:.2} GB {}%",
            x.used_gb, x.total_gb, x.percent
        ),
        None => "Memory   ?".to_string(),
    };
    let batt_line = match b {
        Battery::Present { percent, state } => {
            format!("Battery  {}% {}", percent, state.as_str())
        }
        Battery::Absent => "Battery  n/a".to_string(),
    };
    vec!["Activity Monitor".to_string(), mem_line, batt_line]
}

/// ウィンドウタイトル用の1行文字列を生成する。
/// 形式: "🧠 USED/TOTALGB PERCENT% | 🔋 PERCENT% STATE"。
/// Memory 欠損は "🧠 n/a"、Battery 欠損は "... | 🔋 n/a"。
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
    format!("🧠 {} | 🔋 {}", mem_part, batt_part)
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
    /// `Pages free:` 行からページ数を読み取る。
    fn parses_pages_free() {
        assert_eq!(parse_pages("Pages free:     152673"), Some(152673));
    }

    #[test]
    /// 対象外行、非数値、コロンなしを安全に扱う。
    fn parse_pages_missing_is_none() {
        assert_eq!(parse_pages("Pages wired down: 1"), Some(1));
        assert_eq!(parse_pages("Pages free: notanumber"), None);
        assert_eq!(parse_pages("NoColonHere"), None);
    }

    #[test]
    /// vm_statの4種類のページカウンタをまとめて解析する。
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
    /// 必須カウンタが不足したvm_stat入力をエラーにする。
    fn parse_vm_stat_missing_is_err() {
        assert!(parse_vm_stat("garbage\n").is_err());
    }

    #[test]
    /// 実機出力にある数値末尾のピリオドを許容する。
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
    /// 任意の生成文字列に対してページ解析がpanicしない。
    fn fuzz_parse_pages_never_panic() {
        let mut r = Lcg::new(0x51ed);
        for _ in 0..5000 {
            let n = (r.next() % 32) as usize;
            let s = r.str_n(n);
            let _ = parse_pages(&s); // 不変条件: 任意文字列で panic しない
        }
    }

    #[test]
    /// 任意の生成文字列に対してvm_stat解析がpanicしない。
    fn fuzz_parse_vm_stat_never_panic() {
        let mut r = Lcg::new(0xF00DBA);
        for _ in 0..5000 {
            let n = (r.next() % 64) as usize;
            let s = r.str_n(n);
            let _ = parse_vm_stat(&s); // 不変条件: panic せず Result を返す
        }
    }

    #[test]
    /// 任意のカウンタ値でも、成功時のメモリ計算結果の範囲不変条件を守る。
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
    /// 欠損を含む任意の表示入力でも3行固定の出力を返す。
    fn fuzz_format_lines_three_lines_never_panic() {
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
            assert_eq!(lines.len(), 3);
        }
    }
}

#[cfg(test)]
mod mem {
    use super::*;

    #[test]
    /// 通常のページカウンタからGB値と使用率を計算する。
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
    /// 解放済みページが総ページを超える入力をエラーにする。
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
    /// 実際のメモリ値とバッテリー値を既定の3行表示へ整形する。
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
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Activity Monitor");
        assert_eq!(lines[1], "Memory   29.80 GB / 34.36 GB 87%");
        assert_eq!(lines[2], "Battery  83% discharging");
    }

    #[test]
    /// メモリ欠損を`?`として表示し、行数とラベル位置を維持する。
    fn renders_memory_missing_as_question() {
        let b = Battery::Present {
            percent: 83,
            state: ChargingState::Charged,
        };
        let lines = format_lines(&None, &b);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Activity Monitor");
        assert_eq!(lines[1], "Memory   ?");
    }

    #[test]
    /// バッテリー不在を`n/a`として表示する。
    fn renders_battery_absent_as_na() {
        let m = MemInfo {
            used_gb: 1.0,
            total_gb: 2.0,
            percent: 50,
        };
        let lines = format_lines(&Some(m), &Battery::Absent);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Activity Monitor");
        assert_eq!(lines[1], "Memory   1.00 GB / 2.00 GB 50%");
        assert_eq!(lines[2], "Battery  n/a");
    }

    #[test]
    /// メモリ・バッテリーがともに存在する場合のwindow titleを整形する。
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
            "🧠 29.80/34.36GB 87% | 🔋 83% discharging"
        );
    }

    #[test]
    /// メモリ欠損時もバッテリー部分を保持したwindow titleを整形する。
    fn memory_missing() {
        let b = Battery::Present {
            percent: 83,
            state: ChargingState::Discharging,
        };
        assert_eq!(
            format_window_title(&None, &b),
            "🧠 n/a | 🔋 83% discharging"
        );
    }

    #[test]
    /// バッテリー不在時もメモリ部分を保持したwindow titleを整形する。
    fn battery_absent() {
        let m = MemInfo {
            used_gb: 29.80,
            total_gb: 34.36,
            percent: 87,
        };
        assert_eq!(
            format_window_title(&Some(m), &Battery::Absent),
            "🧠 29.80/34.36GB 87% | 🔋 n/a"
        );
    }
}
