# 🔄 Session Handoff: 常時 window title 更新（watch デーモン）の Herdr ペイン実装

## 🎯 最終目標 (Ultimate Goal)
- `herdr-mac-activity-monitor` で、popup（Ctrl+m）を開いていなくても、Herdr の
  **window title** に `Memory 27.5/32GB 86% | Battery 83% discharging` 形式を
  **約 3 秒周期で常時** 表示する。
- 手段: バイナリに `--watch` モードを追加し、herdr の `[[startup]]` フックから
  背景で常時起動する。popup 側はタイトル操作から手を引く。
- 実装の進め方（ユーザー確定）: **opencode のサブエージェントは使わず、
  Herdr ペインに生成AIを起動して各タスクを実装委譲**（過去設計
  `docs/plans/2026-08-25-mac-activity-monitor-keybind-design.md` の運用方針を継承）。
- ユーザー承認済み: 実行方式 =「startup フックで常駐デーモン」、popup 扱い =
  「popup は維持、タイトルはデーモンが占有」。

## ✅ 完了した事項と「意思決定の背景」 (Done & Why)

- [済] 現状把握・調査
  - **Why:** タイトル更新が `src/tui.rs` の `run_loop` 内（popup 起動中のみ）に
    組まれており、popup 閉じで `clear_window_title()` が呼ばれて消えていた。
    「常時」にするには popup と独立した常時プロセスが必要と判断。
  - **Crucial:** Herdr 0.8.2 に定期タイマー（cron 等）は無い。`[[events]]` は
    `worktree.created` 等トリガーのみ。`[[startup]]` はドキュメント上
    「one-shot・自前で exit すべき」と記載だが、Herdr は異步起動・失敗でもサーバを
    止めないため、**事実上長時間ループのデーモン起動トリグとして使える**。
    公式に非推奨の使い方残りうる点は、実運用上無害と判断し設計で明記済み。

- [済] 設計ドキュメント作成・コミット
  - ファイル: `docs/plans/2026-08-30-persistent-window-title-design.md`
  - コミット: `e83205d`「docs: window title 常時更新（watch デーモン）の設計を追加」
  - **Crucial:** 終了条件は「`HERDR_SOCKET_PATH` ファイル消失（サーバ停止）で
    即 break / 連続 3 回失敗で break」。重複起動防止は
    `HERDR_PLUGIN_STATE_DIR/watch.pid` に自身 pid を書き、他プロセス生存中なら即
    exit。live handoff での二重起動を防止。

- [済] 実装計画ドキュメント作成・コミット（9 タスク、TDD、プレースホルダ無し）
  - ファイル: `docs/plans/2026-08-30-persistent-window-title.md`
  - コミット: `8850472`「docs: 常時 window title の実装計画を追加」
  - タスク構成:
    1. 純粋関数 `should_exit(failed_count, max_fail, socket_exists) -> bool`
    2. `is_pid_alive(pid: u32) -> bool`（`kill -0` で判定、pid==0 は false）
    3. `acquire_watch_lock() -> bool`（state_dir 未設定は無ガードで true）
    4. `watch::run() -> std::io::Result<()>`（3秒ループ＋接続断/連敗で終了）
    5. `main.rs` に `--watch` 引数分岐（含めば `watch::run()`、以外は `tui::run()`）
    6. `tui.rs` から `set_window_title` / `clear_window_title()` 呼び出しを削除
    7. `herdr-plugin.toml` に `[[startup]]` 追加、version `0.1.2`→`0.1.3`、
       `cargo build --release` して `bin/herdr-activity-monitor` にコピー
    8. `README.md` に常時更新の説明追記
    9. 最終確認（全テスト・build・e2e・`git tag v0.1.3`・push）

## 🚧 現在の物理的状態 (Physical Anchor)

- 作業ディレクトリ: `/Users/oki2a24/herdr-mac-activity-monitor`（git branch `main`）
- `git status`: **working tree clean**、`origin/main` に対し **2 コミット先行**
  （`e83205d` `8850472`、**push 未実施**）。
- `git tag`: `v0.1.1` `v0.1.2`。`v0.1.3` は **まだ未生成**。
- 依存関係: `crossterm = "0.27"` のみ（`Cargo.toml` L7）。**新依存禁止**。
- `rustc 1.97.1` / `cargo 1.97.1`。macOS (darwin)。
- `src/main.rs` L2,3,5,7,9 に `mod battery; mod herdr; mod memory; mod parsers; mod tui;`
  がある。**`mod watch;` は無い**（タスク 1 で追加する前提）。
  `main` は L11-14 で `fn main() -> std::io::Result<()> { tui::run() }`。
- `src/tui.rs`:
  - L21: `crate::herdr::clear_window_title().ok();`（タスク 6 で削除対象）
  - L38-39: `let window_title = crate::parsers::format_window_title(&mem, &batt);`
    と続く `if let Err(e) = crate::herdr::set_window_title(&window_title) { ... }`
    ブロック（タスク 6 で削除対象）。
- `src/herdr.rs`: `set_window_title` / `clear_window_title` / `set_request` /
  `clear_request` とその `#[cfg(test)]`（L52-98）。**削除しない**（後方互換）。
- `src/parsers.rs` L153: `format_window_title(&m,&b) -> String`、既存テスト L499-528。
  **変更しない・維持**。
- 直近の成功コマンド: `cargo test` → `test result: ok. 28 passed; 0 failed`
  （タスク 1-3 で最終的に約 31-34 件になるはず）。
- 直近の失敗/未検証: なし。`watch.rs` は未生成（計画ドキュメント内に
  コード全文を記載済み）。

## 📝 次の具体的なアクション (Next Steps)

1. **Herdr 環境確認**: `test "${HERDR_ENV:-}" = 1`。パスしなければ
   「Herdr 内ではない」と報告して停止（herdr skill 鉄則）。
2. **新規ペイン生成（作業ディレクトリ保持・呼出者 focus を維持）**:
   ```bash
   herdr pane split --current --direction right --cwd "/Users/oki2a24/herdr-mac-activity-monitor" --no-focus
   ```
   返却 JSON の `.result.pane.pane_id` を取得。
3. **生成AIを起動**（ユーザー指定の種。`herdr agent` で有効 kind を確認。
   例: `--kind opencode` または `--kind codex`）:
   ```bash
   herdr agent start monitor-impl --kind <kind> --pane <pane_id>
   ```
4. **計画に従い 9 タスクを順に `agent prompt` で委譲**（タスクごとに
   `--wait` で settled 状態を待つ。タスク 1,2,3 は `src/watch.rs`＋`#[cfg(test)]`、
   各タスク末でコミット）:
   ```bash
   herdr agent prompt monitor-impl "計画 docs/plans/2026-08-30-persistent-window-title.md のタスク1（should_exit）を実装。TDD: 失敗テスト→失敗確認→実装→パス→コミット 'feat: watch デーモンの終了判定 should_exit を追加'。コードは計画ドキュメントのコードブロックをそのまま用いる。" --wait --timeout 300000
   ```
   …タスク 2〜9 も同様に各 prompt で委譲。
5. **タスク 7 で再起動検証**: `herdr plugin unlink localmac.activity-monitor 2>/dev/null || true`
   → `herdr plugin link /Users/oki2a24/herdr-mac-activity-monitor` →
   `herdr server reload-config` → 標題が約 3 秒周期で更新されること、
   `herdr plugin log list --plugin localmac.activity-monitor` に
   `action_id: "startup"` が出ることを確認。
6. **タスク 9 e2e**: `herdr server stop` → `sleep 12` →
   `ps aux | grep herdr-activity-monitor | grep -v grep` は空（ゾンビ無し）。
7. **最終**: `cargo test`（全件 PASS）・`cargo build --release`・
   `cp target/release/herdr-activity-monitor bin/herdr-activity-monitor`・
   `git tag v0.1.3`。push は **ユーザー確認後**のみ。

## 注意事項・落とし穴

- **herdr skill 鉄則**: `herdr server stop` はユーザーの明示意思が無ければ叩かない。
  タスク 9 のゾンビ検証で `herdr server stop` を使う必要があるが、この段階では
  ユーザーに確認を得るか、`herdr --no-session` / 名前付きテストセッションで隔離。
- `[[startup]]` は one-shot と公式記載。長時間ループは非推奨だが実運用無害
  （設計文に明記済み）。
- `HERDR_PLUGIN_STATE_DIR` 未設定の非 herdr 実行時は pid ロックなしで動作。
- バイナリ名・プラグイン id（`localmac.activity-monitor`）・マニフェスト構造を
  変更しない（グローバル制約）。

## 💬 再開用プロンプト (Resumption Prompt)

> 次のセッションの最初に貼るプロンプト:
>
> 「`herdr-mac-activity-monitor` の常時 window title 更新を実装する。
>  設計は `docs/plans/2026-08-30-persistent-window-title-design.md`、
>  実装計画は `docs/plans/2026-08-30-persistent-window-title.md`
>  （9 タスク、TDD、各タスク末でコミット）の通りに進めてください。
>
>  運用方針: **opencode のサブエージェントを使わず、Herdr ペインに生成AIを起動して
>  各タスクを実装委譲する**（`herdr` skill を用いる）。
>
>  手順:
>  1) `test "${HERDR_ENV:-}" = 1` で Herdr 内を確認。
>  2) `herdr pane split --current --direction right --cwd /Users/oki2a24/herdr-mac-activity-monitor --no-focus`
>     で作業ペインを生成し `.result.pane.pane_id` を取得。
>  3) `herdr agent start monitor-impl --kind <適宜> --pane <pane_id>` で生成AIを起動。
>  4) 計画のタスク1→9 を `herdr agent prompt monitor-impl "..." --wait`
>     で順に委譲。コードは計画ドキュメントのコードブロックを正確に用いる。
>  5) タスク7は `herdr plugin link` + `herdr server reload-config` で起動確認、
>     タスク9は `herdr server stop` 後のゾンビ検証（このときのみ server を止め、
>     ユーザー確認を得ること）。
>  6) `cargo test`（全件 PASS）・`cargo build --release`・bin コピー・
>     `git tag v0.1.3`。push はユーザー確認後。
>
>  現在の物理状態: working tree clean、origin/main に対し 2 コミット先行
>  （push 済みではない、`v0.1.3` タグ未生成）、`cargo test` は 28 件 PASS、
>  `src/main.rs` に `mod watch;` は未登録、`src/tui.rs` L21/L38-39 がタイトル操作
>  の削除対象。設計・計画は既にコミット済み。」
