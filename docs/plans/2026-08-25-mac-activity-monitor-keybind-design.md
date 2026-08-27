# macOS Activity Monitor プラグイン Keybind 追加設計

- 日付: 2026-08-25
- 状態: 承認待ち
- 対象リポジトリ: herdr-mac-activity-monitor

## 目的

Herdr のキーバインドで Activity Monitor popup を表示/非表示を切り替えられるようにする。
既存の popup 用コマンド実行は維持し、`q` / `Esc` / `Ctrl-C` で終了も継続。

## 要件

- キー: `prefix+m`
- Herdr プラグイン manifest に action と keys.command を追加
- Action は `herdr plugin pane open --plugin localmac.activity-monitor --entrypoint monitor` を実行
- 「トグル」は Herdr 側で重複開放が抑止されるため、実質的なトグルとして機能する
- 実装は Bounded: `herdr-plugin.toml` 変更のみ

## 設計

### manifest 変更

```toml
[[actions]]
id = "toggle-monitor"
title = "Toggle Activity Monitor"
contexts = ["workspace"]
command = ["bash", "-c", "HERDR_BIN_PATH=\"${HERDR_BIN_PATH}\"; \"${HERDR_BIN_PATH}\" plugin pane open --plugin localmac.activity-monitor --entrypoint monitor"]
```

```toml
[[keys.command]]
key = "prefix+m"
type = "plugin_action"
command = "localmac.activity-monitor.toggle-monitor"
description = "toggle activity monitor popup"
```

### 実装メモ

- `HERDR_BIN_PATH` を使って CLI を呼び出すことで、Unix ソケット/Windows 名前付きパイプの差異を吸収
- `placement = "popup"` は既存の pane 定義で維持
- popup の `Escape` 包含は Herdr 仕様により、TUI の `q`/`Esc` 終了は継続

## 実装指示

- デザイン通り `herdr-plugin.toml` を更新
- `prefix+m` で popup が開くことを確認
- 既存の `q`/`Esc`/`Ctrl-C` 終了が機能することを確認

## サブエージェント運用

- サブエージェントは opencode のサブエージェントスキルを使用しない
- Herdr ペインを生成し、そこで生成AIを起動して作業を進める
- 例: `herdr plugin pane open --plugin localmac.activity-monitor --entrypoint monitor` などのペインを生成し、そのペイン内で作業指示を行う
- 実装計画ドキュメントには「サブエージェント相当の作業は Herdr ペインで生成AIを起動して実行する」と明記する

## テスト

- `herdr plugin list` でプラグインが有効であることを確認
- `prefix+m` で popup が開くことを確認
- 既存の popup 終了キーが機能することを確認

## 訂正 (2026-08-27, Herdr 0.8.2 で確認)

上記の `[[keys.command]]` を `herdr-plugin.toml`（マニフェスト）に書く手法は **Herdr 0.8.2 で無効** だと判明した。

- Herdr 0.8.2 のマニフェストパーサ (`RawPluginManifest` struct) には `keys` フィールドが存在しないため、マニフェスト内の `[[keys.command]]` は無視される（バイナリ解析と `herdr --default-config` から確認）。
- 有効な手立ては **`~/.config/herdr/config.toml` のトップレベルに `[[keys.command]]` を記述** すること。バイナリの `CommandKeybindConfig` は `shell` / `pane` / `popup` / `plugin_action` の type を持ち、`--default-config` L132 付近の例が証左。
- plugins.mdx の "Keybindings" セクションは 0.8.2 の挙動と不一致（ドキュメントの誤り）。

よって `prefix+m` はマニフェストではなく `config.toml` に登録し、`herdr server reload-config` で実行中サーバーへ適用する。マニフェストからの `[[keys.command]]` ブロックは削除済み。
