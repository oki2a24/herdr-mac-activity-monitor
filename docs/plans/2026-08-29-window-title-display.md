# Window Title Display 実装計画

> **AIエージェントへの指示:** この計画をタスクごとに実装するには、`subagent-driven-development` または `executing-plans` を使用してください。各ステップには追跡用チェックボックスを使用してください。

**目標:** 既存の Memory / Battery 情報を維持しながら、Herdr の foreground terminal window title に同じ情報を表示する。

**アーキテクチャ:** 既存の 3 秒更新ループで取得した `MemInfo` と `Battery` を popup と window title の両方で利用する。window title 通信は `HERDR_SOCKET_PATH` の Unix domain socket に newline-delimited JSON-RPC を直接送信し、`client.window_title.set` / `clear` を利用する。

**技術スタック:** Rust、crossterm、Rust standard library (`std::os::unix::net::UnixStream`)、Herdr Socket API

**仕様 (Spec):** `docs/plans/2026-08-29-window-title-display-design.md`

**グローバル制約 (Global Constraints):**
- 既存の Memory / Battery 取得処理を変更しない。
- 既存の 3 秒更新ループを再利用する。
- 既存の popup 機能を維持する。
- `placement = "popup"` を変更しない。
- Herdr CLI wrapper は使用しない。
- `HERDR_SOCKET_PATH` の Unix domain socket に直接接続する。
- `client.window_title.set` / `client.window_title.clear` を使用する。
- 新しい外部依存は原則追加しない。
- window title 更新失敗によって popup を終了させない。

---

## 実装前の確認

### タスク 1: 現在のコードと仕様の最終確認

**ファイル:**
- 参照: `src/memory.rs`
- 参照: `src/battery.rs`
- 参照: `src/parsers.rs`
- 参照: `src/tui.rs`
- 参照: `src/main.rs`
- 参照: `herdr-plugin.toml`
- 参照: `docs/plans/2026-08-29-window-title-display-design.md`

**インターフェース:**
- 消費: 既存の `MemInfo`、`Battery`、`run_loop`
- 生産: 実装対象ファイルと既存構造の最終確認

- [ ] **ステップ 1: 既存コードを確認**
  - `memory::get()` が返す `MemInfo` のフィールドを確認する。
  - `battery::get()` が返す `Battery` と `ChargingState` を確認する。
  - `tui::run_loop()` の既存更新順序を確認する。
  - `herdr-plugin.toml` の `placement = "popup"` を確認する。

- [ ] **ステップ 2: テスト実行**
  実行:

      cargo test

  期待値: 既存テストが PASS。

- [ ] **ステップ 3: コミット**
  このタスクではコード変更がないためコミット不要。

---

## タスク 2: window title formatter を追加

**ファイル:**
- 変更: `src/parsers.rs`
- テスト: `src/parsers.rs` の既存テスト構成、または既存の適切なテストファイル

**インターフェース:**

Consumes:

```rust
&MemInfo
&Battery
```

Produces:
```
String
```
関数：
```
fn format_window_title(mem: &MemInfo, batt: &Battery) -> String
```
- [ ] ステップ 1: 失敗するテストを作成
Memory と Battery の代表的な値から、
Memory 27.5/32GB 86% | Battery 83% ↓
の形式になることを確認するテストを追加する。
- [ ] ステップ 2: テストを実行
実行:
```
cargo test format_window_title
```
期待値: 新しいテストが formatter 未実装のため FAIL。
- ステップ 3: 最小限の実装
MemInfo と Battery の既存構造を利用して format_window_title() を実装する。
popup 用の format_lines() は変更しない。
- ステップ 4: テストを実行
実行:
```
cargo test format_window_title
```
期待値: PASS。
- [ ] ステップ 5: コミット
```
git add src/parsers.rs
git commit -m "feat: add window title formatter"
```

## タスク 3: Herdr socket 通信層を追加
ファイル:
- 作成: src/herdr.rs
- 変更: src/main.rs
インターフェース:
Produces:
```
pub fn set_window_title(title: &str) -> Result<(), String>
pub fn clear_window_title() -> Result<(), String>
```
通信仕様：
```
HERDR_SOCKET_PATH
    ↓
UnixStream::connect()
    ↓
newline-delimited JSON
```
set request:
```
{
  "id": "window_title_set",
  "method": "client.window_title.set",
  "params": {
    "title": "<title>"
  }
}
```
clear request:
```
{
  "id": "window_title_clear",
  "method": "client.window_title.clear",
  "params": {}
}
```
- [ ] ステップ 1: socket request生成のテストを作成
socketへ送信する JSON が、
- method
- params
- title
を正しく含むことを検証する。
- [ ] ステップ 2: テストを実行
実行:
cargo test herdr
期待値: 新しいテストが FAIL。
- [ ] ステップ 3: Unix socket 通信を実装
std::os::unix::net::UnixStream を使用する。
HERDR_SOCKET_PATH が存在しない場合は Err を返す。
socket接続または送信に失敗した場合も Err を返す。
JSONは1 requestにつき1行として送信する。
- [ ] ステップ 4: テストを実行
実行:
```
cargo test herdr
```
期待値: PASS。
- [ ] ステップ 5: 全テストを実行
実行:
cargo test
期待値: PASS。
- [ ] ステップ 6: コミット
```
git add src/herdr.rs src/main.rs
git commit -m "feat: add herdr window title client"
```
## タスク 4: 3秒更新ループへ window title 更新を統合
ファイル:
- 変更: src/tui.rs
インターフェース:
Consumes:
```
MemInfo
Battery
format_window_title()
herdr::set_window_title()
herdr::clear_window_title()
```
Produces:
- 既存 popup 更新
- window title 更新
- [ ] ステップ 1: 更新処理のテスト可能な単位を確認
既存 run_loop() の構造を確認し、Memory/Battery を取得した直後に同じ値から window title を生成できる位置を確定する。
- [ ] ステップ 2: 最小限の実装
既存の1 tickの中で、
```
memory::get()
battery::get()
    ↓
format_lines()
format_window_title()
    ↓
popup rendering
window title set
```
となるようにする。
window title更新に失敗しても run_loop() は終了させない。
例えば warning/error を出して次の tickへ進む。
- [ ] ステップ 3: 終了処理を追加
ループ終了時に、
```
herdr::clear_window_title()
```
を1回呼び出す。
clear失敗によって終了処理そのものを失敗させない。
- [ ] ステップ 4: 全テストを実行
実行:
```
cargo test
```
期待値: PASS。
- [ ] ステップ 5: build
実行:
```
cargo build
```
期待値: PASS。
- [ ] ステップ 6: コミット
```
git add src/tui.rs
git commit -m "feat: display activity status in window title"
```
## タスク 5: 実環境で動作確認
ファイル:
- 変更なし
- [ ] ステップ 1: pluginをbuild
```
cargo build
期待値: PASS。
```
- [ ] ステップ 2: Herdrからpluginを起動
既存のplugin起動方法を使用する。
- [ ] ステップ 3: popupを確認
既存のMemory / Battery popupが従来通り表示される。
- [ ] ステップ 4: window titleを確認
ターミナルのwindow titleに、
```
Memory xx.x/yyGB zz% | Battery nn%
```
のような情報が表示される。
- [ ] ステップ 5: 更新を確認
MemoryまたはBatteryの状態が変化した場合、最大で既存の3秒更新周期程度でwindow titleにも反映される。
- [ ] ステップ 6: 終了を確認
plugin終了後、
```
client.window_title.clear
```
によりwindow titleが通常のHerdr titleへ戻る。
- [ ] ステップ 7: socket障害時の挙動を確認
window title APIが利用できない場合でも、popupが継続して動作することを確認する。
完了条件
以下をすべて満たしたら実装完了とする。
- [ ] cargo test が PASS
- [ ] cargo build が PASS
- [ ] 既存popupが維持されている
- [ ] Memoryがwindow titleに表示される
- [ ] Memory使用率がwindow titleに表示される
- [ ] Battery残量がwindow titleに表示される
- [ ] Battery stateがwindow titleに表示される
- [ ] 約3秒周期でwindow titleが更新される
- [ ] plugin終了時にwindow titleがclearされる
- [ ] window title更新失敗でpopupが終了しない

