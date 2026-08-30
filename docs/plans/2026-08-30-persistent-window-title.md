# Persistent Window Title 実装計画

> **AIエージェントへの指示:** REQUIRED SUB-SKILL: この計画をタスクごとに実装するには、移植された `subagent-driven-development` スキル（推奨）または `executing-plans` スキルを起動して使用してください。ステップには追跡用のチェックボックス (`- [ ]`) を使用します。

**目標:** popup 起動の有無に関わらず、Herdr の window title に Memory / Battery 情報を約 3 秒周期で常時表示する。

**アーキテクチャ:** バイナリに `--watch` モードを追加し、`[[startup]]` フックから背景で常時起動する。watch デーモンは `memory::get()`/`battery::get()`/`format_window_title()`/`herdr::set_window_title()` を再利用し、サーバ停止（ソケット消失）または連続失敗で自然に終了する。`watch.pid` ガードで live handoff 時の二重起動を防止する。popup 側（`tui.rs`）からタイトル操作を撤去し、タイトルは watch デーモンが独占する。

**技術スタック:** Rust（std のみ）。`crossterm` は popup 用に維持、watch モードは不使用。新依存なし。

**仕様 (Spec):** `docs/plans/2026-08-30-persistent-window-title-design.md`

**グローバル制約 (Global Constraints):**
- 新しい外部依存は追加しない（`crossterm` のみ維持）。
- macOS のみ（`platforms = ["macos"]` を継承）。
- 既存の `memory` / `battery` / `parsers` の取得・計算ロジックを変更しない。
- 更新間隔は 3 秒のまま維持する。
- `src/herdr.rs` の `clear_request` / `clear_window_title` とそのテストは削除しない（後方互換）。
- 既存の `parsers` / `herdr` の単体＋ファズテストは全て維持・通す。
- TDD を徹底し、各タスクで頻繁にコミットする。
- バイナリ名・プラグイン id・マニフェスト構造の既存約束を変更しない。

---

## ファイル構成

- 新規 `src/watch.rs` — watch デーモン。責務: 終了判定（純粋）`should_exit`、pid 生存判定 `is_pid_alive`、pid ロック `acquire_watch_lock`、ループ `run`。
- 変更 `src/main.rs` — 引数分岐（`--watch` で `watch::run()`、それ以外は `tui::run()`）。
- 変更 `src/tui.rs` — ループ内の `set_window_title` と終了時の `clear_window_title()` を削除。
- 変更 `herdr-plugin.toml` — `[[startup]]` 追加、version `0.1.2` → `0.1.3`。
- 変更 `README.md` — 常時更新の説明と watch モード挙動を追記。

---

### タスク 1: 終了判定の純粋関数 `should_exit`

**ファイル:**
- 作成: `src/watch.rs`
- 変更: `src/main.rs`（`mod watch;` 登録）
- テスト: `src/watch.rs`（`#[cfg(test)]` 内）

**インターフェース (Interfaces):**
- 消費: なし
- 生産: `pub fn should_exit(failed_count: u32, max_fail: u32, socket_exists: bool) -> bool`

  仕様:
  - `socket_exists == false`（サーバ停止と判定）なら即 `true`。
  - `socket_exists == true` なら `failed_count > max_fail` のとき `true`、そうでなければ `false`。

- [ ] **ステップ 1: 失敗するテストを作成**

`src/watch.rs` の先頭に以下を追加する（`mod tests` は後続タスクで拡張していく）。

```rust
//! window title 常時更新の watch デーモン。
//! タイトル操作に集中し、`memory` / `battery` / `parsers` / `herdr` を再利用する。
use std::thread;
use std::time::Duration;

/// 接続断（サーバ停止）または連続失敗閾値超過で終了判定する。
/// `socket_exists == false` は即終了。`socket_exists == true` なら
/// `failed_count > max_fail` のとき終了、そうでなければ継続。
pub fn should_exit(failed_count: u32, max_fail: u32, socket_exists: bool) -> bool {
    if !socket_exists {
        return true;
    }
    failed_count > max_fail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exits_when_socket_gone() {
        assert!(should_exit(0, 5, false));
        assert!(should_exit(3, 5, false));
    }

    #[test]
    fn continues_when_socket_present_and_under_threshold() {
        assert!(!should_exit(0, 5, true));
        assert!(!should_exit(5, 5, true));
    }

    #[test]
    fn exits_when_socket_present_and_over_threshold() {
        assert!(should_exit(6, 5, true));
        assert!(should_exit(100, 5, true));
    }
}
```

- [ ] **ステップ 2: 失敗を確認して実行**

実行: `cargo test watch::tests 2>&1 | head -30`
期待値: 実装が既にあり PASS になるが、ここでは `mod watch;` が無いためコンパイルエラーになる。`src/main.rs` に `mod watch;` を追加するまで `error[E0433]` / 未登録モジュール。

- [ ] **ステップ 3: `main.rs` にモジュール登録**

`src/main.rs` に `mod watch;` を追加する（他 mod の並びに追加）。

- [ ] **ステップ 4: パスを確認して実行**

実行: `cargo test watch::tests`
期待値: `should_exit` 3件すべて PASS。

- [ ] **ステップ 5: コミット**

```bash
git add src/watch.rs src/main.rs
git commit -m "feat: watch デーモンの終了判定 should_exit を追加"
```

---

### タスク 2: pid 生存判定 `is_pid_alive`

**ファイル:**
- 変更: `src/watch.rs`
- テスト: `src/watch.rs`

**インターフェース (Interfaces):**
- 消費: なし
- 生産: `pub fn is_pid_alive(pid: u32) -> bool`

  仕様: 与えられた pid が生きているプロセスに対応すれば `true`。
  他プロセスの生存は Unix で `kill(pid, 0)` シグナルなし探査で判定する
  （Rust std には公開 API がないため、
`std::os::raw` / `libc` 無しで `Command` の `kill` を呼ぶのが最も依存が少ない。
  ただし `libc` 無しで生判定を正確に做すのは難因此、実装は `std::process::Command` で
  `kill -0` を起動し、返り値 0 で生存、非 0（no such process）で不生存を返す。
  `pid == 0` は無効とみなし `false` を返す。

- [ ] **ステップ 1: 失敗するテストを作成**

`src/watch.rs` の `tests` モジュールに以下を追加する。

```rust
 #[test]
    fn current_process_alive() {
        let pid = std::process::id();
        assert!(is_pid_alive(pid));
     }

     #[test]
    fn dead_pid_not_alive() {
        // 非常に高確率で未使用の pid。確実に死んだ pid は取得できないため
        // `kill -0` の挙動（no such process）を信じて false を期待する。
        assert!(!is_pid_alive(0));
     }
```

- [ ] **ステップ 2: 失敗を確認して実行**

実行: `cargo test watch::tests::current_process_alive 2>&1 | tail -20`
期待値: `is_pid_alive` 未定義でコンパイルエラー。

- [ ] **ステップ 3: 最小実装を作成**

`src/watch.rs` に以下を追加する。

```rust
/// 与えられた pid が生きていれば `true`。`pid == 0` は `false`。
/// Unix では `kill -0 <pid>` の結果で判定する（シグナルは実際に送らない）。
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
     }
     let status = std::process::Command::new("kill")
         .arg("-0")
         .arg(pid.to_string())
         .output()
         .map(|o| o.status.success());
    status.unwrap_or(false)
}
```

- [ ] **ステップ 4: パスを確認して実行**

実行: `cargo test watch::tests`
期待値: `current_process_alive` / `dead_pid_not_alive` とも PASS。

- [ ] **ステップ 5: コミット**

```bash
git add src/watch.rs
git commit -m "feat: watch デーモンの pid 生存判定 is_pid_alive を追加"
```

---

### タスク 3: pid ロック `acquire_watch_lock`

**ファイル:**
- 変更: `src/watch.rs`
- テスト: `src/watch.rs`

**インターフェース (Interfaces):**
- 消費: `is_pid_alive`（タスク 2）。
- 生産: `pub fn acquire_watch_lock() -> bool`

  仕様:
   - 環境変数 `HERDR_PLUGIN_STATE_DIR` が未設定なら、pid ガードなしで `true` を返す
     （非 herdr 実行時はロックなし）。
   - 設定されていれば `<state_dir>/watch.pid` を読む。
      - ファイルが無ければ自 pid を書き `true`（獲得した）。
      - ファイルがあれば中に書かれた pid を読み、`is_pid_alive` が `true` なら競合で `false`。
         `false`（死者 pid）なら上書きして `true`。

- [ ] **ステップ 1: 失敗するテストを作成**

`src/watch.rs` の `tests` モジュールに以下を追加する。`HERDR_PLUGIN_STATE_DIR` を
一時ディレクトリに差し替えて検証する。

```rust
 use std::fs;

     #[test]
   #[allow(unsafe_code, deprecated)]
    fn unsets_env_returns_true() {
        unsafe {
            std::env::remove_var("HERDR_PLUGIN_STATE_DIR");
         }
        assert!(acquire_watch_lock());
     }

     #[test]
   #[allow(unsafe_code, deprecated)]
    fn acquires_when_no_existing_pid() {
        use std::env::temp_dir;
        let dir = temp_dir().join(format!("am-watch-t3-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("HERDR_PLUGIN_STATE_DIR", &dir); }
        assert!(acquire_watch_lock());
        let pid_file = dir.join("watch.pid");
        assert!(pid_file.exists());
        fs::remove_dir_all(&dir).ok();
     }

     #[test]
   #[allow(unsafe_code, deprecated)]
    fn blocks_when_live_pid_present() {
        use std::env::temp_dir;
        let dir = temp_dir().join(format!("am-watch-t3b-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("HERDR_PLUGIN_STATE_DIR", &dir); }
        // 生 pid を仮置き（自 pid）。
        fs::write(dir.join("watch.pid"), std::process::id().to_string()).unwrap();
        assert!(!acquire_watch_lock());
        // 死者 pid（pid 0）なら上書きできる。
        fs::write(dir.join("watch.pid"), "0").unwrap();
        assert!(acquire_watch_lock());
        fs::remove_dir_all(&dir).ok();
     }
```

- [ ] **ステップ 2: 失敗を確認して実行**

実行: `cargo test watch::tests::acquires_when_no_existing_pid 2>&1 | tail -20`
期待値: `acquire_watch_lock` 未定義でコンパイルエラー。

- [ ] **ステップ 3: 最小実装を作成**

`src/watch.rs` に以下を追加する。

```rust
pub fn acquire_watch_lock() -> bool {
    use std::fs;
    use std::path::Path;
    let dir = match std::env::var_os("HERDR_PLUGIN_STATE_DIR") {
        Some(d) => d,
         None => return true,
     };
    let pid_file = Path::new(&dir).join("watch.pid");
    let raw = fs::read_to_string(&pid_file);
    match raw {
        Ok(_) => {
            let existing: u32 = raw
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            if is_pid_alive(existing) {
                return false;
            }
            fs::write(&pid_file, std::process::id().to_string()).is_ok()
         }
         Err(_) => {
            fs::write(&pid_file, std::process::id().to_string()).is_ok()
     }
    }
}
```

- [ ] **ステップ 4: パスを確認して実行**

実行: `cargo test watch::tests`
期待値: pid ロック 3件すべて PASS。既存の `should_exit` / `is_pid_alive` も維持。

- [ ] **ステップ 5: コミット**

```bash
git add src/watch.rs
git commit -m "feat: watch デーモンの pid ロック acquire_watch_lock を追加"
```

---

### タスク 4: watch ループ `run`

**ファイル:**
- 変更: `src/watch.rs`
- 消費: `should_exit`（タスク 1）、`acquire_watch_lock`（タスク 3）、
   `crate::memory` / `crate::battery` / `crate::parsers::format_window_title` /
   `crate::herdr::set_window_title`

**インターフェース (Interfaces):**
- 消費: `acquire_watch_lock() -> bool`、`should_exit(u32,u32,bool) -> bool`、
   `memory::get()`、`battery::get()`、`format_window_title(&m,&b)->String`、
   `herdr::set_window_title(&str) -> Result<(),String>`
- 生産: `pub fn run() -> std::io::Result<()>`

  仕様:
   - 冒頭で `acquire_watch_lock()` が `false` のなら `return Ok(())`（他インスタンスが稼働中）。
   - ループ: `memory::get().ok()`（失敗は `None`）、`battery::get()`、
     `format_window_title(&mem,&batt)`、`herdr::set_window_title(&title)`。
   - `set_window_title` が `Err` なら
      - `failed += 1`、
      - `socket_exists = HERDR_SOCKET_PATH` ファイルが存在するか、
      - `if should_exit(failed, MAX_FAIL, socket_exists) { break; }`。
   - `Ok` なら `failed = 0`。その後 `sleep(3s)`、入力の待ちは不要。
   - `MAX_FAIL` は定数 `3`。

- [ ] **ステップ 1: 失敗するテスト（コンパイル）を作成**

`watch::run` は I/O 主体のため単体テストはコンパイルのみ確認する。
`src/main.rs` で `--watch` を呼べることを確認する（タスク 5）。
ここでは `run` 関数のシグネチャが確定することを確認する。

- [ ] **ステップ 2: 失敗を確認して実行**

実行: `cargo test watch 2>&1 | tail -10`
期待値: `run` 未定義なら未使用モジュール。ここでは `run` が存在しないため
`main` 側で呼ぶとエラー、`mod tests` の既存テストは PASS。

- [ ] **ステップ 3: 実装を作成**

`src/watch.rs` に以下を追加する（既存 `mod tests` の上、関数群の末尾に）。

```rust
const MAX_FAIL: u32 = 3;
const TICK: Duration = Duration::from_secs(3);

/// watch デーモン本体。pid ロックを取得できなければ即 return。
/// 以降は 3 秒周期でタイトルを更新し、接続断または連続失敗で終了する。
pub fn run() -> std::io::Result<()> {
    use std::fs;
    use std::thread;
    if !acquire_watch_lock() {
        return Ok(());
     }
    let mut failed: u32 = 0;
    loop {
        let mem = crate::memory::get().ok();
        let batt = crate::battery::get();
        let title = crate::parsers::format_window_title(&mem, &batt);
        match crate::herdr::set_window_title(&title) {
            Ok(()) => failed = 0,
            Err(e) => {
                failed += 1;
                eprintln!("warn: failed to set window title: {e}");
                let socket = std::env::var_os("HERDR_SOCKET_PATH")
                    .and_then(|p| fs::metadata(p).ok());
                if should_exit(failed, MAX_FAIL, socket.is_some()) {
                    break;
                 }
             }
        }
        thread::sleep(TICK);
     }
    Ok(())
}
```

- [ ] **ステップ 4: パスを確認して実行**

実行: `cargo build 2>&1 | tail -15 && cargo test watch 2>&1 | tail -10`
期待値: コンパイル成功、`watch::tests` の 7 件（3+2+2 等）PASS。

- [ ] **ステップ 5: コミット**

```bash
git add src/watch.rs
git commit -m "feat: watch デーモン本体 run（3秒ループ＋接続断終了）を追加"
```

---

### タスク 5: `main.rs` に `--watch` 引数分岐

**ファイル:**
- 変更: `src/main.rs`

**インターフェース (Interfaces):**
- 消費: `watch::run() -> std::io::Result<()>`、`tui::run() -> std::io::Result<()>`
- 生産: なし

  仕様: 引数に `--watch` が含まれれば `watch::run()`、それ以外は `tui::run()`。

- [ ] **ステップ 1: 失敗を確認して実行**

実行: `cargo run --release -- --help 2>&1 | tail` もしくは `cargo build`
期待値: 分岐未実装のため現状の `main`（`tui::run()` のみ）がコンパイル OK でも、
`--watch` を渡しても popup が出る（期待通りではなく）。ここは挙動検証のため
手動で確認する。

- [ ] **ステップ 2: 実装を作成**

`src/main.rs` の `main` を以下に書き換える。

```rust
fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--watch") {
        watch::run()
     } else {
        tui::run()
 }
}
```

- [ ] **ステップ 3: 検証して実行**

`src/watch.rs` で `crate::memory` / `crate::battery` /
`crate::parsers` / `crate::herdr` が `pub` 経由で参照できるか確認する。
`main.rs` は各 `mod` を既に登録済み（トップレベル `mod`）なので
`watch.rs` から `crate::xxx` で参照可能。

実行: `cargo build --release 2>&1 | tail -10`
期待値: コンパイル成功。`bin/` へのコピーはタスク 7。

- [ ] **ステップ 4: コンパイルのみ確認（手動）**

実行: `cargo build --release 2>&1 | tail -5`
期待値: `Finished`。`target/release/herdr-activity-monitor --watch` を
短期間で起動し、`HERDR_SOCKET_PATH` 未設定なら `set_window_title` が `Err` となり
3 回失敗→`break`→自然終了することを確認（`ps` で残留無しを確認）。

- [ ] **ステップ 5: コミット**

```bash
git add src/main.rs
git commit -m "feat: --watch 引数で watch デーモンを起動する分岐を追加"
```

---

### タスク 6: `tui.rs` からタイトル操作を撤去

**ファイル:**
- 変更: `src/tui.rs`

**インターフェース (Interfaces):**
- 消費: なし
- 生産: なし。タイトル操作は `watch` に移管済み。

  仕様:
   - `run_loop` 内の `crate::herdr::set_window_title(...)` 呼び出しブロックを削除。
   - `run` 内の `crate::herdr::clear_window_title().ok();` 呼び出しを削除。
   - この結果、`tui.rs` は `crate::herdr` を一切使わなくなる。
   - `crate::parsers::format_window_title` の参照も `tui.rs` から消えるが、
     `watch.rs` で使うので削除はしない。

- [ ] **ステップ 1: 失敗を確認（現状コードの再確認）**

実行: `rg -n "window_title|herdr::" src/tui.rs`
期待値: 4 箇所（`set_window_title`, `clear_window_title` の呼び出し各 1 と
  該当 `eprintln`）が見られる。

- [ ] **ステップ 2: 削除を実行**

`src/tui.rs` の `run` 内の `crate::herdr::clear_window_title().ok();` 行と、
  `run_loop` 内の以下のブロックを削除する。

```rust
let window_title = crate::parsers::format_window_title(&mem, &batt);
if let Err(e) = crate::herdr::set_window_title(&window_title) {
    eprintln!("warn: failed to set window title: {e}");
}
```

- [ ] **ステップ 3: 検証して実行**

実行: `cargo build 2>&1 | tail -10`
期待値: `unused import` 等を排除するため、`tui.rs` が `crate::herdr` を参照
  しないことを確認。`mod tests` は `tui.rs` に無いため問題なし。
  `format_window_title` の `use` が `watch.rs` 側で使えれば OK
  （`parsers` は `pub mod`）。

- [ ] **ステップ 4: 既存テスト維持を確認**

実行: `cargo test 2>&1 | tail -5`
期待値: 28 件（タスク 1-3 追加分で 31〜34 件）がすべて PASS。

- [ ] **ステップ 5: コミット**

```bash
git add src/tui.rs
git commit -m "refactor: popup 側からタイトル操作を撤去し watch が独占"
```

---

### タスク 7: `herdr-plugin.toml` に `[[startup]]` 追加

**ファイル:**
- 変更: `herdr-plugin.toml`
- 変更: `bin/herdr-activity-monitor`（再ビルドのコピー）

**インターフェース (Interfaces):**
- 消費: `--watch` 引数（タスク 5）
- 生産: なし

  仕様:
   - `[[startup]]` を 1 個追加。`command = ["./bin/herdr-activity-monitor", "--watch"]`。
   - `version = "0.1.3"` に更新。

- [ ] **ステップ 1: 実装**

`herdr-plugin.toml` の `[[panes]]` と `[[actions]]` の間に以下を追加する
（起動フックはビルド/パン/アクションの前に書くのが読みやすい）。

```toml
[[startup]]
command = ["./bin/herdr-activity-monitor", "--watch"]
```

  トップの `version = "0.1.2"` を `version = "0.1.3"` に更新。

- [ ] **ステップ 2: 再ビルドして `bin/` にコピー**

```bash
cargo build --release
cp target/release/herdr-activity-monitor bin/herdr-activity-monitor
```

- [ ] **ステップ 3: 検証**

実行:
```bash
herdr plugin unlink localmac.activity-monitor 2>/dev/null || true
herdr plugin link /Users/oki2a24/herdr-mac-activity-monitor
herdr plugin list
```

期待値:
  - `plugin list` に `localmac.activity-monitor` と `0.1.3` が見える。
  - リンク直後の `herdr server` 側で startup が走る。
    `herdr plugin log list --plugin localmac.activity-monitor` に
    `action_id: "startup"` のログが出る。

- [ ] **ステップ 4: 手動確認（実機）**

実行（別ターミナル）:

```bash
# 起動時に watch が起動し、タイトルが更新されることを確認
herdr server reload-config
# タイトルが約 3 秒周期で更新されること
tail -f ~/.config/herdr/herdr-client.log | grep -iE "window_title|startup"
# server stop 後は watch がゾンビにならないこと
herdr server stop
ps aux | grep herdr-activity-monitor | grep -v grep
```

期待値:
  - server stop 直後、`ps` に `herdr-activity-monitor` が残っていない
    （約 3×MAX_FAIL=9 秒以内で自然終了）。

- [ ] **ステップ 5: コミット**

```bash
git add herdr-plugin.toml bin/herdr-activity-monitor
git commit -m "feat: startup フックで watch デーモンを常時起動 (v0.1.3)"
```

---

### タスク 8: README 追記

**ファイル:**
- 変更: `README.md`

**インターフェース (Interfaces):**
- 消費: なし
- 生産: なし

  仕様: 「概要」「使い方」「技術仕様」に watch モードの説明を追加。

- [ ] **ステップ 1: 実装**

`README.md` に以下を追記（各セクションに合わせて挿入）。

```markdown
## 常時 window title 更新

v0.1.3 以降、プラグイン起動と同時に `--watch` デーモンが背景で起動し、
popup を開いていなくても約 3 秒周期で window title に
`Memory 27.5/32GB 86% | Battery 83% ↓` 形式で反映します。
popup は従来どおり `Ctrl+m` で開けます。

デーモンは以下で自然終了します。

- `herdr server stop` 後の約 9 秒内（連続 3 回失敗で切り捨て）
- live handoff 時
- 他インスタンスが稼働中（`HERDR_PLUGIN_STATE_DIR/watch.pid` で検知）
```

  「技術仕様」セクションに追記:

```markdown
- 常時更新: `--watch` モード（`[[startup]]` フック）で 3 秒ループ
- 重複起動防止: `HERDR_PLUGIN_STATE_DIR/watch.pid`
- 接続断検知: `HERDR_SOCKET_PATH` ファイル消失、または 3 回連続失敗
```

- [ ] **ステップ 2: コミット**

```bash
git add README.md
git commit -m "docs: README に常時 window title 更新（watch モード）を追記"
```

---

### タスク 9: 最終確認・ビルド・テスト・配布

**ファイル:**
- 変更: なし

**インターフェース (Interfaces):**
- 消費: 全タスク
- 生産: なし

- [ ] **ステップ 1: 全テスト・ビルド**

実行:
```bash
cargo test
cargo build --release
cp target/release/herdr-activity-monitor bin/herdr-activity-monitor
```

期待値:
  - 全テスト（既存 28 件+新規 8 件）PASS。
  - release ビルド成功。

- [ ] **ステップ 2: 手動 end-to-end**

実行:
```bash
herdr plugin unlink localmac.activity-monitor 2>/dev/null || true
herdr plugin link /Users/oki2a24/herdr-mac-activity-monitor
# 起動時、タイトルが更新されることを確認
herdr server reload-config
# タイトルが約 3 秒周期で更新
# popup を開いても閉じてもタイトルは消えない
herdr plugin pane open --plugin localmac.activity-monitor --entrypoint monitor
# server stop 後、watch が自然終了する
herdr server stop
sleep 12
ps aux | grep herdr-activity-monitor | grep -v grep  # 何も出ないこと
```

期待値: 上記すべて通過。

- [ ] **ステップ 3: バージョンタグ**

```bash
git tag v0.1.3
git push  # （ユーザー確認後）
```

- [ ] **ステップ 4: 設計ドキュメントの記録**

`docs/plans/2026-08-30-persistent-window-title-design.md` の状態を
「実装済み」と記述し、コミット。

```bash
git add docs/plans/2026-08-30-persistent-window-title-design.md
git commit -m "docs: 実装済みとして更新"
```