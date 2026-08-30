# Persistent Window Title Design

- 日付: 2026-08-30
- 状態: 実装済み
- 対象リポジトリ: herdr-mac-activity-monitor
- 分類: Bounded（既存コードへの限定追加。データ取得・解析・整形は再利用）

## 目的

現状、Memory / Battery を window title へ反映する処理は `src/tui.rs` の 3 秒更新
ループ（popup 起動中のみ）に組み込まれており、popup を閉じると
`clear_window_title()` が呼ばれてタイトルが消える。結果としてタイトルは
「Ctrl+m で popup を開いている間だけ」しか見えない。

本変更では、popup と独立して常時動く watch デーモンを追加し、それが
`client.window_title.set` を 3 秒周期で呼ぶことで、popup を開いていなくても
window title に Memory / Battery が常時表示されるようにする。

## 要件

- window title を popup 起動の有無に関わらず常時更新する。
- 既存の popup（Ctrl+m / `toggle-monitor` action）を維持する。
- タイトルへの set/clear は新設した watch デーモンが独占する。
- 既存のメモリ・バッテリー取得処理、`format_window_title`、`herdr` 通信層を再利用する。
- 新しい外部依存は追加しない（`crossterm` のみ維持）。
- macOS のみ（既存プラグインの `platforms = ["macos"]` を継承）。

## Non-Goals

- popup 機能の削除・表示形式の変更。
- メモリ・バッテリー取得ロジックの変更。
- 更新間隔（3 秒）の変更。
- CPU / Disk など情報の追加。
- Herdr 公式に無い定期タイマー機構の自作（cron 等）。
- macOS タイトルバーの直接操作。
- 無関係なリファクタリング。

## 背景（Herdr 0.8.2 の仕様）

- `[[startup]]` フックは、セッション復帰と API socket 起動後、有効プラグインごとに
  1 回走る。live handoff で新しいサーバが起動した際にも再走する（クライアント
  接続・config reload・link/enable 時は走らない）。
- 公式ドキュメントでは startup は「one-shot 初期化・自前で exit すべき」とあるが、
  事実上 Herdr が異步起動し、失敗でもサーバを止めないため、長時間ループの
  デーモン起動トリグとして機能する。
- 公式の定期タイマーは存在しない。`[[events]]` は `worktree.created` 等の
  トリガーイベントだけ。
- タイトルは実行中のプロセスが socket 経由で `client.window_title.set` を呼ぶ。

## 設計

### バイナリに `--watch` モードを追加

`src/main.rs` の引数を分岐する。引数なしは現状の popup TUI（`tui::run()`）。
`--watch` は TUI なしで watch デーモンループを動かす。

新規 `src/watch.rs` を追加する。責務は以下の 1 つだけ:

- 3 秒ループで `memory::get()` / `battery::get()` を呼び、
  `parsers::format_window_title(&mem, &batt)` → `herdr::set_window_title(...)` を送る。

watch デーモンは `EnterAlternateScreen` / `enable_raw_mode` 等は行わない
（タイトル only）。`crossterm` は使わない。

### 起動/終了のライフサイクル

- **終了条件（ゾンビ防止）**: `set_window_title` が `Err` のとき、
  `HERDR_SOCKET_PATH` がファイルとして存在しなければ `break`（サーバ停止でソケット
  が削除され、自然に終了）。ソケットが存在する間は N 連敗までリトライし、
  瞬時的な失敗を許容する。
- **重複防止**: `HERDR_PLUGIN_STATE_DIR/watch.pid` に自身 pid を書く。起動時に
  この pid が他プロセスで生存中（`kill(pid, 0)` などで生判定）なら即 exit。
  live handoff で二重起動しうる件を防止する。`HERDR_PLUGIN_STATE_DIR` 未設定時は
  pid 監視なしで動作（非 herdr 実行時）。

### popup 側からタイトル操作を撤去

`src/tui.rs`：

- ループ内の `herdr::set_window_title(...)` 呼び出しを削除。
- 終了時の `herdr::clear_window_title()` 呼び出しを削除。

タイトルは watch デーモン独占とすることで、popup 開閉時のちらつき・競合をなくす。

`src/herdr.rs` の `clear_request` / `clear_window_title` とそのテストは、watch
終了時に最終タイトルを外側ターミナルへ残存させる方針上では使わなくなるが、
削除は行わない（後方互換・再利用可能性）。`set_request` / `set_window_title`
は watch デーモンで引き続き使う。

### `herdr-plugin.toml` に起動フック

```toml
[[startup]]
command = ["./bin/herdr-activity-monitor", "--watch"]
```

サーバ起動時に 1 回だけ（handoff でも再走）背景で起動。
`HERDR_SOCKET_PATH` / `HERDR_PLUGIN_STATE_DIR` 等は注入済み。

### バージョンと README

- `herdr-plugin.toml` 版 `0.1.2` → `0.1.3`。
- README に「常に window title 更新」の説明と、watch モードの起動/終了挙動を追記。

## 純粋な終了判定の分離

終了判定（連敗数＋ソケット存在）と pid 生存判定は、純粋な判定関数に切り出し
単体テスト対象とする。入出力 I/O とは分ける。

- `should_exit(failed_count: u32, max_fail: u32, socket_exists: bool) -> bool`
  - `socket_exists == false` で即 `true`（サーバ停止）。
  - `socket_exists == true` なら `failed_count > max_fail` のとき `true`。
- pid 生存判定は `src/` の別関数（実プロセス API を使うため I/O 側に置く）とし、
  生存判定に使うシステムコール部分は最小限にする。

## 再利用する既存資産

- `src/memory.rs` / `src/battery.rs` / `src/parsers.rs`（`MemInfo` / `Battery` /
  `ChargingState` / `format_window_title`）をそのまま使う。
- `src/herdr.rs` の `set_window_title` を使う。

## 変更サマリ

- 追加: `src/watch.rs`、`main.rs` の引数分岐、`herdr-plugin.toml` の `[[startup]]`。
- 修正: `src/tui.rs`（タイトル set/clear 削除）、`herdr-plugin.toml`（version）、
  `README.md`。
- 削除なし。

## テスト

- 既存の `parsers` / `herdr` の単体＋ファズテストは全て維持・通す。
- 新規 `watch` 終了判定関数の単体テスト。
  - `socket_exists=false` → exit。
  - `socket_exists=true` かつ連敗 > max → exit。
  - 連敗 < max かつソケット存在 → 継続。
- 実機での手動検証:
  1. `herdr server` を再起動し、ポップアップを開いていない状態でもタイトルが
     約 3 秒周期で更新される。
  2. `toggle-monitor` で popup を開くと従来通り詳細表示が出る。
  3. popup を閉じてもタイトルは消えない。
  4. `herdr server stop` 後、watch デーモンは自然に終了する（ゾンビ残らない）。
  5. live handoff（`herdr update` 等）で二重稼働しない。

## コストとリスク

- startup を長時間ループのデーモン起動トリグとして使う点は、公式に「one-shot」
  とされているため非推奨の使い方は残る。ただし Herdr は異步起動・失敗時もサーバ
  を止めないため、実運用上は問題ない。
- live handoff 直後に二重稼働の隙があるが、`watch.pid` ガードで回避。
- herdr 完全停止後は外側ターミナルに最終タイトルが残存する（自然な挙動）。
