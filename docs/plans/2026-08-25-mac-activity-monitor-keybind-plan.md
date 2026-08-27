# Activity Monitor Plugin Keybind 追加 実装計画

> **AIエージェントへの指示:** REQUIRED SUB-SKILL: この計画をタスクごとに実装するには、移植された `subagent-driven-development` スキル（推奨）または `executing-plans` スキルを起動して使用してください。ステップには追跡用のチェックボックス (`- [ ]`) を使用します。

**目標:** Herdr プラグイン `localmac.activity-monitor` に `prefix+m` キーバインドで Activity Monitor popup を開くアクションを追加し、GitHub リポジトリ経由で配布できるようにする。

**アーキテクチャ:** マニフェスト `herdr-plugin.toml` に `[[actions]]` と `[[keys.command]]` を追加。変更はリポジトリにコミットし、タグ付きで push して `herdr plugin install` での更新を想定。Herdr 側で popup の重複開放を抑止し実質的なトグルとして機能。既存 TUI 終了キー維持。

**技術スタック:** Herdr プラグイン manifest TOML、bash、git

**仕様 (Spec):** `docs/plans/2026-08-25-mac-activity-monitor-keybind-design.md`

**グローバル制約:**
- キーは `prefix+m`
- Action id は `toggle-monitor`
- `[[keys.command]]` の `type` は `plugin_action`
- 既存の popup 終了キー `q`/`Esc`/`Ctrl-C` は維持
- サブエージェント相当は Herdr ペインで生成AIを起動して作業を進める
- 配布は GitHub から `herdr plugin install` で行う
- `herdr-plugin.toml` の `version` は更新してタグ付けする

---
### タスク 1: `herdr-plugin.toml` に action と keys.command を追加し、バージョンを上げて GitHub に配布

**ファイル:**
- 変更: `herdr-plugin.toml`
- テスト: 手動確認

**インターフェース:**
- 消費: 既存の `[[panes]]` id `monitor`、既存の `version`
- 生産: `[[actions]]` id `toggle-monitor`、`[[keys.command]]` key `prefix+m`、`version` bump

- [ ] **ステップ 1: action と keybind の TOML ブロックを追加**

```toml
[[actions]]
id = "toggle-monitor"
title = "Toggle Activity Monitor"
contexts = ["workspace"]
command = ["bash", "-c", "HERDR_BIN_PATH=\"${HERDR_BIN_PATH}\"; \"${HERDR_BIN_PATH}\" plugin pane open --plugin localmac.activity-monitor --entrypoint monitor"]

[[keys.command]]
key = "prefix+m"
type = "plugin_action"
command = "localmac.activity-monitor.toggle-monitor"
description = "toggle activity monitor popup"
```

- [ ] **ステップ 2: version を 0.1.1 に更新**

```toml
version = "0.1.1"
```

- [ ] **ステップ 3: 変更をコミット**

```bash
git add herdr-plugin.toml
git commit -m "feat: prefix+m キーバインドで Activity Monitor popup を開くアクションを追加"
git commit -m "chore: bump version to 0.1.1"
```

- [ ] **ステップ 4: タグを打って push**

```bash
git tag -a v0.1.1 -m "prefix+m keybind"
git push origin <current-branch> --tags
```

- [ ] **ステップ 5: GitHub インストールで動作確認**

```bash
herdr plugin install oki2a24/herdr-mac-activity-monitor
herdr plugin action list --plugin localmac.activity-monitor
```

期待値: `toggle-monitor` が一覧に表示される

- [ ] **ステップ 6: キーバインド動作確認**

Herdr セッションで `prefix+m` を押下し、Activity Monitor popup が開くことを確認。既存の `q`/`Esc`/`Ctrl-C` 終了も維持。

---
計画が完成し、`docs/plans/2026-08-25-mac-activity-monitor-keybind-plan.md` に保存されました。

---

## 訂正 (2026-08-27, Herdr 0.8.2 で確認)

上記の **マニフェスト `herdr-plugin.toml` への `[[keys.command]]` 記述は Herdr 0.8.2 で無効** だと判明した。

- マニフェストパーサ (`RawPluginManifest` struct) に `keys` フィールドが無く、マニフェスト内の `[[keys.command]]` は無視される。
- 有効な手立ては **`~/.config/herdr/config.toml` のトップレベル** に `[[keys.command]]` を書くこと。`key = "prefix+m"`, `type = "plugin_action"`, `command = "localmac.activity-monitor.toggle-monitor"`。
- `herdr server reload-config` で実行中サーバーへ適用（起動時にも読み込まれる）。マニフェストからの `[[keys.command]]` ブロックは削除済み。
- plugins.mdx の "Keybindings" セクションは 0.8.2 の挙動と不一致。
