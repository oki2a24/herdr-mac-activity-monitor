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

Herdr v1 にはプラグイン更新コマンドはないため、最新版を再インストールしてください。

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

表示内容:
```
─────────────────────────────
Memory    29.80 GB / 34.36 GB   87%
Battery    83%   discharging
```

- `q` / `Esc` で終了
- 3 秒ごとに自動更新

## 常時 window title 更新

v0.1.3 以降、プラグイン起動と同時に `--watch` デーモンが背景で起動し、
popup を開いていなくても約 3 秒周期で window title に
`Memory 27.5/32GB 86% | Battery 83% discharging` 形式で反映します。
popup は従来どおり `Ctrl+m` で開けます。

デーモンは以下で自然終了します。

- `herdr server stop` 後の約 9 秒内（連続 3 回失敗で切り捨て）
- live handoff 時
- 他インスタンスが稼働中（`HERDR_PLUGIN_STATE_DIR/watch.pid` で検知）

技術仕様:

- 常時更新: `--watch` モード（`[[startup]]` フック）で 3 秒ループ
- 重複起動防止: `HERDR_PLUGIN_STATE_DIR/watch.pid`
- 接続断検知: `HERDR_SOCKET_PATH` ファイル消失、または 3 回連続失敗

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
