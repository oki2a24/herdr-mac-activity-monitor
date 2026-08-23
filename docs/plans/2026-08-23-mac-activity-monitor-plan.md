# macOS 使用メモリ・バッテリー ダッシュボード 実装計画

> **AIエージェントへの指示:** REQUIRED SUB-SKILL: herdr の opencode サブエージェント
> （kind=opencode, model=`ollama/qwen3.8:27b-mlx`）が新コンテキストで実行する。
> 本計画と仕様書をすべて読み、各タスクを **TDD（レッド・グリーン・リファクター）＋ファズ**
> で順に実行せよ。各実装タスクは「テスト先行 → RED 確認 → 最小実装 → GREEN 確認 → リファクター」
> のサイクルを厳守する。ファブテストは std のみのプロパティベースで `cargo test` で実行する。

**目標:** macOS の使用済みメモリ（GB と使用率 %）・バッテリー残量（% と充電中/充電中ではない）を、
Herdr popup ペインで 3 秒ごとに更新して表示する 1 バイナリの TUI。

**アーキテクチャ:** Rust 単一バイナリ。I/O（`vm_stat`/`sysctl`/`pmset` 呼出）と**純粋なパース/計算**
を分離する。パース/計算は `src/parsers.rs` に集約し、I/O はその上を呼ぶ。TUI は `crossterm`。

**技術スタック:** Rust + **crossterm のみ**（外部 crate はこれだけ）。
データ取得は `std::process::Command`。正規表現・ファズフレームワーク等の他 crate は使わない。
ファズは `std::hash::DefaultHasher` の代わりに**自前 xorshift64 乱数**（テストモジュール内）で実現。

**仕様 (Spec):** `docs/plans/2026-08-23-mac-activity-monitor-design.md`（必ず先に読む）

**グローバル制約:**
- メモリ使用量: `used = (mem/psize) - (free + inactive + speculative + purgeable)` [ページ単位]、
   `used_gb = used*psize/1e9`、`total_gb = mem/1e9`、`percent = round(used/total*100)`。
  数値は **10 進 GB（/1e9）**。
- バッテリー: `pmset -g batt` の行から `\d+%` を抽出。`discharging`/`charging`/`charged` を判定。
- 更新周期 **3 秒**。キー `q` / `Esc` / `Ctrl-C` で終了。
- メモリ取得失敗/無効 → `Memory     ?`、バッテリーなし/失敗 → `Battery  n/a`。プロセスは落ちない。
- 進捗バーなし。圧迫度（memory_pressure）は表示しない。
- 外部 crate は crossterm のみ。プロダクションコードでは `std` のみ。
- 表示フォーマットは仕様書「表示フォーマット（確定）」に厳密に従う。
- **テスト: タスクごとに先決で書き、失敗（RED）を確認してから実装。各パース/計算関数には
   任意入力で panic しないことを検証する std ファズテストを付与する。**
- **コミット: メッセージは日本語＋ Conventional Commits。** 形式は
  `<type>: <日本語の説明>`。type は `feat` / `fix` / `refactor` / `chore` / `test` /
  `docs` / `build` / `ci` / `style` のみ。主語・文尾の句点なし。
- **コード: 公開される関数/型/enum には日本語の doc コメント（`///`）を追記すること。**
  内部ヘルパーも役割が自明でなければ `///` / `//` で短く日本語で注釈する。
  実装者は各関数先頭に `/// 何をするか` の1文を入れる。

---

## ファイル構成

```
herdr-mac-activity-monitor/
├── Cargo.toml          # crossterm 依存のみ
├── .gitignore          # /target
├── herdr-plugin.toml   # Herdr マニフェスト（panes: popup）
└── src/
     ├── main.rs        # mod 宣言 + tui::run()
     ├── memory.rs      # I/O: sysctl + vm_stat → parsers で計算 → Result<MemInfo,String>
     ├── battery.rs     # I/O: pmset          → parsers でパース → Battery
     ├── tui.rs         # crossterm 生モード、3s redraw、キー処理
     └── parsers.rs     # 純粋関数群（テスト・ファズの主対象）
       ├── VmStat { free,inactive,speculative,purgeable: u64 }
       ├── parse_pages(&str) -> Option<u64>
       ├── parse_vm_stat(&str) -> Result<VmStat,String>
       ├── ChargingState { Charging, Discharging, Charged }
       ├── parse_battery_line(&str, present: bool) -> Option<Battery>
       ├── MemInfo { used_gb:f64, total_gb:f64, percent:i32 }
       ├── compute_memory(total_pages, psize, vmstat) -> Result<MemInfo,String>
       └── format_lines(&Option<MemInfo>, &Battery) -> Vec<String>
```

---

## タスク 1: Cargo プロジェクト初期化

**ファイル:** 作成 `Cargo.toml`, `.gitignore`

- [ ] **ステップ 1: プロジェクト生成**

実行（`herdr-mac-activity-monitor/` で、空のディレクトリ）:
```bash
cargo new --bin --name herdr-activity-monitor .
# 既存ファイルがある場合は `cargo init --name herdr-activity-monitor`
```

- [ ] **ステップ 2: crossterm 依存追加**

`Cargo.toml`:
```toml
[dependencies]
crossterm = "0.27"
```

- [ ] **ステップ 3: `.gitignore`**

```
/target
```

- [ ] **ステップ 4: ビルド確認**

実行: `cargo build` 期待値: 成功（既存 main.rs が未使用なら削除可）。

- [ ] **ステップ 5: (任意) git init**

```bash
git add -A && git commit -m "chore: crossterm依存のcargoプロジェクト初期化"
```

---

## タスク 2: parsers.rs — pages / vm_stat（TDD + ファズ）

**ファイル:** 作成 `src/parsers.rs`、`src/main.rs` に `mod parsers;` を宣言。

**インターフェース:**
- 生産:
  - `pub struct VmStat { pub free:u64, pub inactive:u64, pub speculative:u64, pub purgeable:u64 }`
  - `pub fn parse_pages(line: &str) -> Option<u64>`
  - `pub fn parse_vm_stat(s: &str) -> Result<VmStat, String>`

- [ ] **ステップ 1: RED — `parse_pages` の失敗テスト**

`src/parsers.rs`:
```rust
pub struct VmStat { pub free:u64, pub inactive:u64, pub speculative:u64, pub purgeable:u64 }

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
}

fn parse_pages(_line: &str) -> Option<u64> { todo!() }
fn parse_vm_stat(_s: &str) -> Result<VmStat, String> { todo!() }
```

- [ ] **ステップ 2: Verify RED**

実行: `cargo test parse_pages` 期待値: `todo!()` で **panic**（失敗）すること。

- [ ] **ステップ 3: GREEN — `parse_pages` 最小実装**

```rust
fn parse_pages(line: &str) -> Option<u64> {
    let after = line.split(':').nth(1)?;
    after.split_whitespace().next()?.parse::<u64>().ok()
}
```

- [ ] **ステップ 4: Verify GREEN**

実行: `cargo test parse_pages` 期待値: **全 PASS**。

- [ ] **ステップ 5: RED — `parse_vm_stat` の失敗テスト**

`src/parsers.rs` の tests モジュールに追加:
```rust
#[test]
fn parses_full_vm_stat() {
    let s = "Page size of 16384 bytes\n\
        Pages free:     152673\n\
        Pages inactive: 121622\n\
        Pages speculative:  2118\n\
        Pages purgeable:    2168\n\
        Pages wired down: 1456763\n";
    let v = parse_vm_stat(s).unwrap();
    assert_eq!((v.free, v.inactive, v.speculative, v.purgeable),
         (152673, 121622, 2118, 2168));
}
#[test]
fn parse_vm_stat_missing_is_err() {
    assert!(parse_vm_stat("garbage\n").is_err());
}
```

- [ ] **ステップ 6: Verify RED**

実行: `cargo test parse_vm_stat` 期待値: `todo!()` による panic（失敗）。

- [ ] **ステップ 7: GREEN — `parse_vm_stat` 最小実装**

```rust
fn parse_vm_stat(s: &str) -> Result<VmStat, String> {
    let mut free = None;
    let mut inactive = None;
    let mut speculative = None;
    let mut purgeable = None;
    for line in s.lines() {
        let t = line.trim_start();
        if t.starts_with("Pages free:") { free = Some(parse_pages(t)); }
        else if t.starts_with("Pages inactive:") { inactive = Some(parse_pages(t)); }
        else if t.starts_with("Pages speculative:") { speculative = Some(parse_pages(t)); }
        else if t.starts_with("Pages purgeable:") { purgeable = Some(parse_pages(t)); }
    }
    match (free, inactive, speculative, purgeable) {
        (Some(f), Some(i), Some(sp), Some(p)) =>
            Ok(VmStat { free:f, inactive:i, speculative:sp, purgeable:p }),
        _ => Err("vm_stat parse failed".to_string()),
    }
}
```

- [ ] **ステップ 8: Verify GREEN**

実行: `cargo test parse_vm_stat` 期待値: **全 PASS**。

- [ ] **ステップ 9: REFACTOR — ファズテスト追加（std、パース群の全不変条件）**

まず `src/parsers.rs` の**モジュールトップ**（最上部の `use`/struct を並べる部分、
   `#[cfg(test)] mod tests` の外側）に、全テストモジュールで再利用する乱数ジェネレータ
   を定義する:
```rust
// parsers.rs 最上位（モジュール本体、任意の #[cfg(test)] mod の外側）
#[cfg(test)]
pub(crate) struct Lcg(u64);
#[cfg(test)]
impl Lcg {
    pub(super) fn new(seed: u64) -> Self { Lcg(seed | 1) }
    pub(super) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13; x ^= x >> 7; x ^= x << 17;
        self.0 = x; x
     }
    pub(super) fn str_n(&mut self, n: usize) -> String {
         // 半角英数字・空白・コロンで任意文字列（UTF-8 保証）を生成
        const AL: [u8; 7] = *b": 0123456789";
        let bytes: Vec<u8> = (0..n).map(|_| AL[(self.next() % AL.len()) as usize]).collect();
        String::from_utf8(bytes).unwrap()
     }
}
```
次に `tests` モジュール内に `parse_pages`/`parse_vm_stat` のファズを追加
   （`use super::Lcg;` でモジュールトップの `Lcg` を利用）:
```rust
#[test]
fn fuzz_parse_pages_never_panic() {
    let mut r = Lcg::new(0x51ed);
    for _ in 0..5000 {
        let s = r.str_n(r.next() % 32);
        let _ = parse_pages(&s); // 不変条件: 任意文字列で panic しない
     }
}
#[test]
fn fuzz_parse_vm_stat_never_panic() {
    let mut r = Lcg::new(0xF00DBA);
    for _ in 0..5000 {
        let s = r.str_n(r.next() % 64);
        let _ = parse_vm_stat(&s); // 不変条件: panic せず Result を返す
     }
}
```

- [ ] **ステップ 10: Verify GREEN（ファズ）**

実行: `cargo test` 期待値: **全 PASS**（panic なし）。

- [ ] **ステップ 11: コミット**

```bash
git add -A && git commit -m "feat: ページ・vm_stat 解析をTDD+ファズで追加"
```

---

## タスク 3: parsers.rs — battery / memory 計算 / format（TDD + ファズ）

**ファイル:** 変更 `src/parsers.rs`

**インターフェース:**
- 生産:
  - `pub enum ChargingState { Charging, Discharging, Charged }`
  - `pub enum Battery { Present { percent:u8, state:ChargingState }, Absent }`
  - `pub fn parse_battery_line(line: &str, present: bool) -> Option<Battery>`
  - `pub fn split_present(line: &str) -> bool`（`present: false` なら false）
  - `pub struct MemInfo { pub used_gb:f64, pub total_gb:f64, pub percent:i32 }`
  - `pub fn compute_memory(total_pages:u64, psize:u64, vm:VmStat) -> Result<MemInfo,String>`
  - `pub fn format_lines(m: &Option<MemInfo>, b: &Battery) -> Vec<String>`

- [ ] **ステップ 1: RED — battery パース tests**

```rust
#[cfg(test)]
mod bt {
    use super::*;
    const L: &str = " -InternalBattery-0 (id=22478947) 83%; discharging; 17:55 remaining present: true";
    #[test] fn discharging() {
        match parse_battery_line(L, true) {
            Some(Battery::Present { percent, state }) => {
                assert_eq!(percent, 83);
                assert!(matches!(state, ChargingState::Discharging));
            }
            _ => panic!("expected present discharging"),
        }
    }
    #[test] fn absent_when_present_false() {
        let l = " -InternalBattery-0 (id=1) 100%; charged; present: false";
        assert_eq!(parse_battery_line(l, false), None);
    }
    #[test] fn charging_keyword() {
        let l = " -InternalBattery-0 40%; charging; present: true";
        let b = parse_battery_line(l, true).unwrap();
        assert!(matches!(b, Battery::Present { state: ChargingState::Charging, .. }));
    }
    #[test] fn charged_fallback() {
        let l = " -InternalBattery-0 100%; 5:00 remaining present: true";
        let b = parse_battery_line(l, true).unwrap();
        assert!(matches!(b, Battery::Present { state: ChargingState::Charged, .. }));
    }
}
```

- [ ] **ステップ 2: Verify RED**

実行: `cargo test bt` 期待値: 未定義による compile/panic（失敗）。

- [ ] **ステップ 3: GREEN — battery パース実装**

```rust
pub enum ChargingState { Charging, Discharging, Charged }
pub enum Battery {
    Present { percent: u8, state: ChargingState },
    Absent,
}
pub fn split_present(line: &str) -> bool {
    // "present: false" でない限り true
    !line.contains("present: false")
}
pub fn parse_battery_line(line: &str, present: bool) -> Option<Battery> {
    if !present { return None; }
    let percent = line.split('%').next()
        .and_then(|s| s.trim().rsplit(' ').next())
        .and_then(|s| s.parse::<u8>().ok())?;
    let state = if line.contains("discharging") {
        ChargingState::Discharging
    } else if line.contains("charging") {
        ChargingState::Charging
    } else {
        ChargingState::Charged
    };
    Some(Battery::Present { percent, state })
}
```

- [ ] **ステップ 4: Verify GREEN**

実行: `cargo test bt` 期待値: **全 PASS**。

- [ ] **ステップ 5: RED — memory 計算 tests**

```rust
#[cfg(test)]
mod mem {
    use super::*;
    #[test] fn computes_used_gb_and_percent() {
        // 総ページ=2097152 (32/16384*1e9), psize=16384,
        // free=152673 inactive=121622 spec=2118 purgeable=2168
        let vm = VmStat { free:152673, inactive:121622, speculative:2118, purgeable:2168 };
        let m = compute_memory(2097152, 16384, vm).unwrap();
        assert!((m.total_gb - 34.359738368).abs() < 0.01); // 34.36GB
        assert!((m.used_gb - 29.80).abs() < 0.05);
        assert_eq!(m.percent, 87);
    }
    #[test] fn total_less_than_freed_is_err() {
        let vm = VmStat { free:999999999, inactive:0, speculative:0, purgeable:0 };
        assert!(compute_memory(100, 16384, vm).is_err()); // used が負になり得ない
    }
}
```

- [ ] **ステップ 6: Verify RED**

実行: `cargo test mem` 期待値: compile 失敗（失敗）。

- [ ] **ステップ 7: GREEN — memory 計算実装**

```rust
pub struct MemInfo { pub used_gb: f64, pub total_gb: f64, pub percent: i32 }
pub fn compute_memory(total_pages: u64, psize: u64, vm: VmStat) -> Result<MemInfo, String> {
    let freed = vm.free + vm.inactive + vm.speculative + vm.purgeable;
    let used_pages = total_pages
        .checked_sub(freed)
        .ok_or("used_pages overflow")?;
    let used_bytes = used_pages as u128 * psize as u128;
    let total_bytes = total_pages as u128 * psize as u128;
    let used_gb = used_bytes as f64 / 1e9;
    let total_gb = total_bytes as f64 / 1e9;
    let percent = if total_pages == 0 { 0 }
        else { ((used_pages as f64 / total_pages as f64) * 100.0).round() as i32 };
    Ok(MemInfo { used_gb, total_gb, percent })
}
```

- [ ] **ステップ 8: Verify GREEN**

実行: `cargo test mem` 期待値: **全 PASS**。

- [ ] **ステップ 9: RED — format tests**

```rust
#[cfg(test)]
mod fmt {
    use super::*;
    #[test] fn renders_present_battery() {
        let m = MemInfo { used_gb:29.80, total_gb:34.36, percent:87 };
        let b = Battery::Present { percent:83, state:ChargingState::Discharging };
        let lines = format_lines(&Some(m), &b);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[2], "Memory   29.80 GB / 34.36 GB 87%");
        assert_eq!(lines[3], "Battery   83% discharging");
    }
    #[test] fn renders_memory_missing_as_question() {
        let b = Battery::Present { percent:83, state:ChargingState::Charged };
        let lines = format_lines(&None, &b);
        assert_eq!(lines[2], "Memory   ?");
    }
    #[test] fn renders_battery_absent_as_na() {
        let m = MemInfo { used_gb:1.0, total_gb:2.0, percent:50 };
        let lines = format_lines(&Some(m), &Battery::Absent);
        assert_eq!(lines[3], "Battery   n/a");
    }
}
```
（注: 各行の空白数・列揃えは仕様書「表示フォーマット（確定）」に固定。
`Memory   ` / `Battery   ` の先頭空白は 3 文字、値と単位の間・各列間は 1 文字。
   実装者はアサーション文言に合わせて `format!` の空白を1文字単位で固定する。
  仕様書と不一致が疑われる場合は、ここで仕様書側を合わせてから GREEN へ。**）

- [ ] **ステップ 10: Verify RED**

実行: `cargo test fmt` 期待値: compile 失敗（失敗）。

- [ ] **ステップ 11: GREEN — format 実装**

```rust
impl ChargingState {
    fn as_str(&self) -> &'static str {
        match self {
            ChargingState::Charging => "charging",
            ChargingState::Discharging => "discharging",
            ChargingState::Charged => "charged",
        }
    }
}
pub fn format_lines(m: &Option<MemInfo>, b: &Battery) -> Vec<String> {
    let mem_line = match m {
        Some(x) => format!("Memory   {:.2} GB / {:.2} GB {}%", x.used_gb, x.total_gb, x.percent),
        None => "Memory   ?".to_string(),
    };
    let batt_line = match b {
        Battery::Present { percent, state } => format!("Battery   {}% {}", percent, state.as_str()),
        Battery::Absent => "Battery   n/a".to_string(),
    };
    vec![
        "Activity Monitor".to_string(),
        "─────────────────────────────".to_string(),
        mem_line,
        batt_line,
    ]
}
```

- [ ] **ステップ 12: Verify GREEN**

実行: `cargo test fmt` 期待値: **全 PASS**。

- [ ] **ステップ 13: REFACTOR — ファズ（バッテリー・計算・format、全不変条件）**

`tests` モジュール内に追加（`super::Lcg` を利用、同一モジュール内で統合）:
```rust
#[test]
fn fuzz_parse_battery_line_never_panic() {
    let mut r = Lcg::new(0xB41);
    for _ in 0..5000 {
        let s = r.str_n(r.next() % 48);
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
        let vm = VmStat { free:r.next(), inactive:r.next(), speculative:r.next(), purgeable:r.next() };
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
        let m = if r.next() % 2 == 0 {
            Some(MemInfo { used_gb: r.next() as f64/1e6, total_gb: r.next() as f64/1e6, percent: (r.next()%101) as i32 - 50 })
         } else { None };
        let b = if r.next() % 2 == 0 {
            Battery::Present { percent: r.next() as u8, state: ChargingState::Charging }
         } else { Battery::Absent };
        let lines = format_lines(&m, &b);
        assert_eq!(lines.len(), 4);
      }
}
```
（注: `super::Lcg` を使い `tests` 内で1モジュールに統合。
`parse_pages`/`parse_vm_stat`/`parse_battery_line`/`compute_memory`/`format_lines`
   のすべてが任意入力でも panic しない・不変条件を保つことを検証。**

- [ ] **ステップ 14: Verify GREEN（ファズ）**

実行: `cargo test` 期待値: **全 PASS**。

- [ ] **ステップ 15: コミット**

```bash
git add -A && git commit -m "feat: バッテリー・メモリ計算・出力整形の解析をTDD+ファズで追加"
```

---

## タスク 4: memory.rs（I/O、vm_stat+sysctl → parsers）

**ファイル:** 作成 `src/memory.rs`、`main.rs` に `mod memory;`。

**インターフェース:**
- 消費: `crate::parsers::{compute_memory, parse_vm_stat, VmStat}`
- 生産: `pub fn get() -> Result<crate::parsers::MemInfo, String>`

- [ ] **ステップ 1: RED — `sysctl_uint` に対するテストは I/O 依存のため不要。**
   このタスクは I/O 層。TDD の対象は純粋部分（タスク2・3で済む）。
   ここでは**手動作動のみ**で検証する（パニックしないこと・値が出ることを確認）。

- [ ] **ステップ 2: 実装**

`src/memory.rs`:
```rust
use std::process::Command;
use crate::parsers::{compute_memory, parse_vm_stat};

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{cmd} failed: {e}"))?
        .stdout;
    Ok(String::from_utf8_lossy(&out).into_owned())
}
fn sysctl_uint(name: &str) -> Result<u64, String> {
    let out = run("sysctl", &["-n", name])?;
    out.trim().parse::<u64>().map_err(|e| format!("parse {name}: {e}"))
}
pub fn get() -> Result<crate::parsers::MemInfo, String> {
    let psize = sysctl_uint("hw.pagesize")?;
    let total_bytes = sysctl_uint("hw.memsize")?;
    let total_pages = total_bytes / psize;
    let raw = run("vm_stat", &[]).map_err(|e| format!("vm_stat: {e}"))?;
    let vm = parse_vm_stat(&raw)?;
    compute_memory(total_pages, psize, vm)
}
```

- [ ] **ステップ 3: 手動作動確認**

一時 `main.rs` の代わりに:
```bash
cargo run --example mem_check 2>/dev/null || \
  cat > /tmp/memcheck.rs <<'EOF'
// 手動確認用（examples/ に置く）
EOF
```
代替として、一時テストバイナリ:
```rust
// examples/mem_check.rs
fn main() {
    let m = memory::get().expect("mem get");
    println!("{:.2} {:.2} {}", m.used_gb, m.total_gb, m.percent);
}
```
実行: `cargo run --example mem_check` 期待値: `29.xx 34.36 8x` 程度。

- [ ] **ステップ 4: コミット**

```bash
git add -A && git commit -m "feat: メモリI/O層（sysctl+vm_stat）を追加"
```

---

## タスク 5: battery.rs（I/O、pmset → parsers）

**ファイル:** 作成 `src/battery.rs`、`main.rs` に `mod battery;`。

**インターフェース:**
- 消費: `crate::parsers::{Battery, parse_battery_line, ChargingState}`
- 生産: `pub fn get() -> crate::parsers::Battery`（失敗は `Battery::Absent`）

- [ ] **ステップ 1: RED 対象は純粋部分（タスク3で済む）。I/O 層は手動作動のみ。**

- [ ] **ステップ 2: 実装**

`src/battery.rs`:
```rust
use std::process::Command;
use crate::parsers::{Battery, split_present};

pub fn get() -> Battery {
    let raw = match Command::new("pmset").arg("-g").arg("batt").output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout),
        Err(_) => return Battery::Absent,
    };
    let line = raw.lines()
        .find(|l| l.contains("%") && l.contains("present"))
        .unwrap_or("");
    let present = split_present(line);
    parse_battery_line(line, present).unwrap_or(Battery::Absent)
}
```
（注: `split_present(line)` で `present: false` を検出し、`parse_battery_line`
   に渡す。`%` と `present` を両方含む行を探すのは実出力の `InternalBattery-0 ...` 行のため。**

- [ ] **ステップ 3: 手動作動確認**

`examples/batt_check.rs`:
```rust
fn main() {
    match battery::get() {
        crate::parsers::Battery::Present { percent, state } =>
            println!("{percent}% {state:?}"),
        crate::parsers::Battery::Absent => println!("n/a"),
    }
}
```
実行: `cargo run --example batt_check` 期待値: `8x% Discharging` 程度、デスクトップなら `n/a`。

- [ ] **ステップ 4: コミット**

```bash
git add -A && git commit -m "feat: バッテリーI/O層（pmset）を追加"
```

---

## タスク 6: tui.rs（crossterm 生モード、3s redraw、キー処理）

**ファイル:** 作成 `src/tui.rs`。

**インターフェース:**
- 消費: `crate::memory::get`, `crate::battery::get`, `crate::parsers::format_lines`
- 生産: `pub fn run() -> std::io::Result<()>`

- [ ] **ステップ 1: RED 対象は純粋部分（format はタスク3で済む）。TUI は I/O＋生モード、手動作動のみ。**

- [ ] **ステップ 2: 実装**

`src/tui.rs`:
```rust
use std::io::Write;
use std::time::Duration;
use crossterm::{
    execute,
    terminal::{enable_raw_mode, disable_raw_mode, enter_alternate_screen, leave_alternate_screen},
    event::{self, poll, Event, KeyCode},
    cursor::{Hide, Show},
    style::Print,
};
use crate::{battery, memory, parsers::MemInfo};

pub fn run() -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, enter_alternate_screen(), Hide)?;
    let result = run_loop(&mut stdout);
    disable_raw_mode()?;
    execute!(stdout, leave_alternate_screen(), Show)?;
    result
}

fn run_loop<F: std::io::Write>(out: &mut F) -> std::io::Result<()> {
    let tick = Duration::from_secs(3);
    loop {
        let mem: Option<MemInfo> = memory::get().ok();
        let batt = battery::get();
        for l in parsers::format_lines(&mem, &batt) {
            execute!(out, Print(l), Print("\n"))?;
        }
        out.flush().ok();
        // 3s 後 or キー入力で戻り、キーがあれば q/Esc で抜ける
        let _ = poll(tick).map_err(|_| ());
        if let Event::Key(k) = event::read()? {
            match k.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                _ => {}
            }
        }
    }
    Ok(())
}
```

- [ ] **ステップ 3: 手動作動確認**

実行: `cargo run` 期待値:
- 4 行表示（`Activity Monitor` / 罫線 / `Memory ...` / `Battery ...`）
- 3 秒ごと更新
- `q` or `Esc` で元画面へ戻る（別画面から抜ける）

- [ ] **ステップ 4: コミット**

```bash
git add -A && git commit -m "feat: crosstermによる3秒更新TUIを追加"
```

---

## タスク 7: main.rs 統合 + release ビルド

**ファイル:** 変更 `src/main.rs`。

- [ ] **ステップ 1: 実装**

`src/main.rs`:
```rust
mod battery;
mod memory;
mod parsers;
mod tui;

fn main() -> std::io::Result<()> {
    tui::run()
}
```
（注: `mod parsers;` を最上位 `main.rs` で宣言。`memory.rs`/`battery.rs`/`tui.rs`
   は `crate::parsers` を使うため必要**）

- [ ] **ステップ 2: release ビルド**

実行: `cargo build --release` 期待値: 成功、`target/release/herdr-activity-monitor` 生成。

- [ ] **ステップ 3: コミット**

```bash
git add -A && git commit -m "feat: mainをTUIに接続しreleaseビルド"
```

---

## タスク 8: herdr-plugin.toml + 起動確認

**ファイル:** 作成 `herdr-plugin.toml`（リポジトリルート）。

- [ ] **ステップ 1: マニフェスト**

```toml
id = "localmac.activity-monitor"
name = "Activity Monitor"
version = "0.1.0"
min_herdr_version = "0.7.3"
description = "macOS 使用メモリ・バッテリーをHerdr popupに表示"
platforms = ["macos"]

[[panes]]
id = "monitor"
title = "Activity Monitor"
placement = "popup"
width = "80%"
height = 60
command = ["herdr-activity-monitor"]
```

- [ ] **ステップ 2: バイナリを PATH へ**

```bash
ln -sf "$PWD/target/release/herdr-activity-monitor" "$HOME/.local/bin/herdr-activity-monitor"
```

- [ ] **ステップ 3: 登録・起動**

```bash
herdr plugin link "$PWD"
herdr plugin pane open --plugin localmac.activity-monitor --entrypoint monitor
```

- [ ] **ステップ 4: 起動確認**

popup 開く → 3 秒ごと更新 → `q`/Esc/Ctrl-C で閉じる、を確認。

- [ ] **ステップ 5: コミット**

```bash
git add -A && git commit -m "chore: herdrプラグインマニフェストを追加し起動確認"
```

---

## サブエージェント向け実行メモ

- **各パース/計算関数は必ず RED 確認後に実装。** 「todo!() で panic すること」を `cargo test` で確認。
- ファズは std の xorshift64（タスク2 の `Lcg`）で、`parse_pages`/`parse_vm_stat`/
   `parse_battery_line`/`compute_memory`/`format_lines` の全不変条件を検証する。
  全関数が「任意入力でも panic しない」ことが保証されること。
- I/O 層（memory.rs/battery.rs/tui.rs）は `cargo run --example ...` / `cargo run` で手動作動確認。
- 外部 crate は crossterm のみ。正規表現・ファズ crate 等は禁止。
- `mod parsers;` は `src/main.rs` 最上位で宣言すること（`crate::parsers` 依存のため）。
- **公開シンボル（`pub` の関数・型・enum・メソッド）には、日本語の `///` doc コメントを1文ずつ付けること。**
  内部ヘルパーも役割が自明でなければ `//` の1文で注釈する。例:
```rust
/// `hw.pagesize` と `hw.memsize` を読んでページ単位で「使用済みメモリ」を算出する。
/// 失敗時はエラー。Activity Monitor の "Memory Used" と同じ値を 10進 GB で返す。
pub fn get() -> Result<MemInfo, String> { /* ... */ }

/// 各メモリカウンタ（ページ数）を保持する構造体。
pub struct VmStat { /* ... */ }
```
全タスク完了後に `cargo test`（全 PASS・ファズ含む）と `cargo build --release` を再実行し、
   結果を報告し、コミット履歴を列挙する。
