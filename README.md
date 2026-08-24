# macOS Activity Monitor for Herdr

macOS の使用メモリ・バッテリー残量を Herdr popup に表示するプラグイン。

## 概要

Activity Monitor で確認していた「使用済みメモリ」「バッテリー残量 %」を Herdr の popup ペインで一覧表示します。
更新周期は 3 秒。Rust で実装し、crossterm のみを使用。

## インストール

```bash
herdr plugin install oki2a24/herdr-mac-activity-monitor
```

インストール完了後、プラグインが有効になります。

## アップデート

```bash
herdr plugin update localmac.activity-monitor
```

または手動で最新版を取得：

```bash
herdr plugin install oki2a24/herdr-mac-activity-monitor
```

## アンインストール

```bash
herdr plugin uninstall localmac.activity-monitor
```

## 使い方

Herdr で popup を開く：

```bash
herdr plugin pane open --plugin localmac.activity-monitor --entrypoint monitor
```

表示内容：

```
Activity Monitor
─────────────────────────────
Memory    29.80 GB / 34.36 GB   87%
Battery    83%   discharging
```

- `q` / `Esc` / `Ctrl-C` で終了
- 3 秒ごとに自動更新

## 技術仕様

- 言語: Rust
- 依存: crossterm のみ
- メモリ取得: `sysctl hw.pagesize` / `hw.memsize` + `vm_stat`
- バッテリー取得: `pmset -g batt`
- プラットフォーム: macOS のみ

## 開発

```bash
cargo build --release
cp target/release/herdr-activity-monitor bin/
```

`herdr-plugin.toml` の version を更新し、コミット・タグを付けて push してください。
