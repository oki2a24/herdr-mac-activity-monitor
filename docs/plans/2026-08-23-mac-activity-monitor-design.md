# macOS 使用メモリ・バッテリー ダッシュボード (Herdr プラグイン)

- 日付: 2026-08-23
- 状態: 承認済み（実装へ）
- 対象リポジトリ: herdr-mac-activity-monitor

## 目的・背景

Activity Monitor で見ていた「使用済みメモリ」「バッテリー残量 %」を、一眼で確認できれば
良い。使用済みメモリで「他にアプリを起動できるか」、バッテリー % で「充電を始めるか」を
即座に判断したい。常時メニューバー表示は Herdr プラグイン v1 では不可能（v1 の UI は
ターミナルペインのみ）ため、Herdr の **popup のペイン** で表示する形とする。

## 表示先

- Herdr の `[[panes]]` 項目。`placement = "popup"`。
- 既定寸法 `width = "80%"`, `height = 60`。
- 起動: `herdr plugin pane open --plugin <id> --entrypoint monitor`。
- 閉じる: Esc / `q` / Ctrl-C。

## 表示内容（最小・確定）

```
Activity Monitor
─────────────────────────────
Memory   29.80 GB / 34.36 GB   87%
Battery   83%   discharging
```

- 進捗バーは付けない（ユーザー指示）。
- 更新周期: **3 秒**。
- 数値は **10 進 GB**（Activity Monitor と一致、`x/1e9`）。
- 圧迫度（memory pressure）は表示しない（ユーザー指示）。

## データ取得（cargo の crate 依存は crossterm のみ）

### メモリ（vm_stat + sysctl）

Activity Monitor の "Memory Used" と同じ計算式:

```
psize   = sysctl hw.pagesize
mem     = sysctl hw.memsize
pages   = vm_stat から parse:
            free, inactive, speculative, purgeable
used    = (mem/psize) - (free + inactive + speculative + purgeable) [ページ]
bytes   = used * psize
used_gb = bytes / 1e9
total_gb= mem / 1e9
percent = used_gb / total_gb * 100
```

実測値（2026-08-23、32GB/10進 34.36GB）:
- `vm_stat`(page 16384B): free=152673, inactive=121622, spec=2118, purgeable=2168
- used = 2097152 - (152673+121622+2118+2168) = 1700616 pages
- 1700616 x 16384 / 1e9 = **29.80 GB** → `29.80 / 34.36  87%`

### バッテリー（pmset -g batt）

- `pmset -g batt` 出力を parse。
- `batteryPercent`: `... (id=xxx) 83%; discharging; 17:55 remaining present: true`
  の `83` を `(\d+)%` で抽出。
- 充電中判定: 行に `discharging` があれば `discharging`、`charging` があれば `charging`、
  どちらもなければ `charged`。
- バッテリーなし（デスクトップ/外付け）: `present: false` 或いは取得失敗 → バッテリー行は
  `n/a` と表示、プロセスは落ちない。

### 各項目の失敗は独立ハンドリング

- メモリ取得失敗 → `Memory   ?`、バッテリー失敗 → `Battery  n/a`。

## アーキテクチャ（crossterm 生TUI、1バイナリ）

### 1. `memory.rs`
- `pub struct MemInfo { used_gb: f64, total_gb: f64, percent: i32 }`
- `pub fn get() -> Result<MemInfo, String>`
- 内部: `Command::new("sysctl")` x 2、`Command::new("vm_stat")` x 1。標準 `std::process` /
  正規表現は自前 parse（regex crate なし、`str::split`/`find`）。

### 2. `battery.rs`
- `pub enum ChargingState { Charging, Discharging, Charged }`
- `pub enum Battery { Present { percent: u8, state: ChargingState }, Absent }`
- `pub fn get() -> Battery`（失敗は `Battery::Absent`。`pmset -g batt` 経由）

### 3. `tui.rs`
- `crossterm` による生モード / 別画面（alternate screen）。
- 毎 3 秒に `memory::get() / battery::get()` を呼び、画面を redraw。
- キー: `q` / Ctrl-C / Esc でループから抜ける。
- 非同期: `std::thread::sleep` 単発ループで十分（3s なので crossterm の `poll` 併用）。

### 4. `main.rs`
- `mod memory; mod battery; mod tui;` を公開。
- `tui::run()` を呼ぶ。

### データフロー

```
loop:
  sleep(3s) → memory::get() → battery::get() → render(screen)
  key? if q/esc/ctrl-c => break
  (resize 時: crossterm::execute resize)
```

## 表示フォーマット（確定）

```
Activity Monitor
─────────────────────────────
Memory   {used_gb:.2f} GB / {total_gb:.2f} GB   {percent}%
Battery   {%}   {state}
```

- 失敗時: `Memory   ?` / `Battery  n/a`。
- 行の幅は動的（端末幅に依存しない、固定 4 行で十分）。

## 依存

- `cargo` の crate: **crossterm のみ**（外部プロセス呼出は標準 `std::process`）。
- Rust 標準ライブラリ + crossterm。他に `regex` 等は使わない（自前 parse）。

## herdr-plugin.toml

```toml
id = "localmac.activity-monitor"
name = "Activity Monitor"
version = "0.1.0"
min_herdr_version = "0.7.3"   # プラグインAPI/イベント対応の最低版本
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

- `plugin link /Users/oki2a24/herdr-mac-activity-monitor` で登録。
- 起動: `herdr plugin pane open --plugin localmac.activity-monitor --entrypoint monitor`。

## テスト

- `cargo build` 成功。
- 手動作動: popup 内で 3 秒毎に更新、`q`/Esc/Ctrl-C で退場。
- 非 macos でのビルドは不要（platforms=["macos"]）。

## 進め方

実装は **herdr の opencodeサブエージェント**（kind opencode, model `ollama/qwen3.8:27b-mlx`）に
委任。本リポジトリを作業ディレクトリに渡す。
