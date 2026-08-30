# Window Title Display Design

## Overview

`herdr-mac-activity-monitor` は、macOS の Memory 使用量と Battery 残量を取得し、Herdr popup に表示するプラグインである。

本変更では、既存の popup 表示を維持したまま、同じ Memory / Battery 情報を Herdr の foreground terminal の window title に表示する。

主目的は、ローカル LLM の実行中など、Memory 使用量と Battery 残量を常時確認できるようにすることである。

## Goals

- Memory 使用量を常時確認できるようにする。
- Memory 使用率を常時確認できるようにする。
- Battery 残量を常時確認できるようにする。
- Battery の充電・放電状態を常時確認できるようにする。
- 既存の Herdr popup 表示を維持する。
- 既存の Memory / Battery 取得処理を再利用する。
- 既存の 3 秒更新ループを再利用する。
- Herdr の公式 `client.window_title.set` / `client.window_title.clear` APIを利用する。
- Herdr socket に直接 JSON-RPC を送信する。
- 新しい外部依存を原則追加しない。

## Non-Goals

今回の変更では以下を行わない。

- popup 機能の削除
- popup の表示形式の大幅な変更
- Memory / Battery 取得ロジックの変更
- 更新間隔の変更
- CPU / Disk など新しいシステム情報の追加
- macOS のタイトルバーを直接操作する実装
- Herdr CLI wrapper を経由した window title 更新
- 独立した timer / background thread による window title 更新
- 無関係なリファクタリング

## User Experience

window title は短く、一目で重要な情報を確認できる形式とする。

基本形式：

    Memory 27.5/32GB 86% | Battery 83% ↓

意味：

- `27.5/32GB`: 使用済み Memory / 総 Memory
- `86%`: Memory 使用率
- `83%`: Battery 残量
- `↓`: 放電中など Battery state を表現

Battery の状態表現は既存の `Battery` / `ChargingState` のモデルに従い、実装時に既存コードと一貫した表現を選択する。

## Existing Data Flow

現在のデータフローは以下である。

    memory::get()
        ↓
    MemInfo

    battery::get()
        ↓
    Battery

    3秒ごとの run_loop
        ↓
    format_lines()
        ↓
    Herdr popup

変更後：

    memory::get() ─────┐
                       ├──→ popup rendering
    battery::get() ────┤
                       │
                       └──→ window title formatting
                                  ↓
                           client.window_title.set

同じ tick で取得した Memory / Battery データを popup と window title の両方で利用する。

## Architecture

### Data acquisition

既存の以下をそのまま利用する。

- `src/memory.rs`
- `src/battery.rs`
- `MemInfo`
- `Battery`
- `ChargingState`

Memory / Battery の取得処理は変更しない。

### Window title formatting

popup 用の `format_lines()` と window title 用の文字列生成は分離する。

window title は 1 行の短い文字列とし、popup の複数行表示ロジックとは独立させる。

例：

    format_window_title(&mem, &batt) -> String

### Herdr communication

新しい Herdr 通信モジュールを追加する。

例：

    src/herdr.rs

責務：

- `HERDR_SOCKET_PATH` を取得する
- Unix domain socket に接続する
- newline-delimited JSON を送信する
- `client.window_title.set` を呼び出す
- `client.window_title.clear` を呼び出す

Herdr CLI は使用しない。

### Update lifecycle

既存の `src/tui.rs` の 3 秒更新ループに window title 更新を統合する。

1. Memory を取得
2. Battery を取得
3. popup を描画
4. window title 用文字列を生成
5. `client.window_title.set` を送信
6. 3 秒待機
7. 次の tick

plugin 終了時には `client.window_title.clear` を呼び出す。

## Error Handling

window title API の失敗によって popup plugin 全体を終了させない。

例えば socket 接続に失敗した場合：

    window title update failed
        ↓
    warning / error logging
        ↓
    popup は継続

Memory / Battery の既存エラー処理も変更しない。

## Compatibility

既存の `placement = "popup"` は変更しない。

今回の目的は popup を廃止することではなく、既存 popup に window title 表示という別の表示経路を追加することである。

window title が正常に動作することを確認した後、将来的に popup を必須としない構成への変更を検討できるが、それは今回のスコープ外とする。

## Testing

以下を確認する。

### Unit tests

window title formatter が以下を正しく生成すること。

- Memory 使用量
- Memory 総量
- Memory 使用率
- Battery 残量
- Battery state

### Herdr communication tests

可能な範囲で、socket に送信する JSON-RPC request の内容をテストする。

### Integration / manual verification

実際の Herdr 環境で、

1. plugin を起動する
2. popup が従来通り表示される
3. window title に Memory / Battery が表示される
4. 約3秒周期で値が更新される
5. plugin 終了後に window title が clear される
6. window title 更新に失敗しても popup が継続する

ことを確認する。

## Constraints

- Rust の既存構成に従う。
- 可能な限り標準ライブラリを使用する。
- 外部依存を増やさない。
- 既存の Memory / Battery 取得処理を再利用する。
- 既存の 3 秒更新ループを再利用する。
- 既存 popup 機能を維持する。
- Herdr の `client.window_title.set` / `client.window_title.clear` を利用する。
- `HERDR_SOCKET_PATH` を利用して Unix domain socket に直接接続する。
- Herdr CLI wrapper は使用しない。
