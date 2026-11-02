# Popup app memory implementation plan

**Goal:** ポップアップにアプリ単位で合算したメモリ使用量の上位3件を表示する。

**Architecture:** 現在のユーザーに属するPIDを列挙し、`footprint`へ複数PIDを一括指定して各プロセスのphysical footprintを読む。実行パスに含まれる最上位`.app` bundleで合算し、既存のpopup専用バックグラウンドworkerで取得する。

**Tech stack:** Rust、macOS `ps` / `footprint` / `libproc` / Foundation、既存のMakefile検証ターゲット。

**Spec:** [popup-app-memory-design.md](2026-11-01-popup-app-memory-design.md)

## Global constraints

- 表示件数はアプリ集約後の上位3件とする。
- physical footprintを使い、取得不能な値は推測しない。
- 高コストのsnapshotはpopup表示中だけ取得する。
- window titleと他のmetricの表示契約を維持する。

## Review focus

- 複数helperのmemoryが同一`.app` bundleへ合算されること。
- nested helper `.app`も最上位アプリbundleへまとめられること。
- アプリ合計を計算した後に降順で上位3件へ絞ること。
- アプリ外のプロセスをアプリ名と誤認せず、判別可能な名前で残すこと。
- footprintの欠損やPID消滅で全体scanがpanicしないこと。
- refresh中は最後に取得できた一覧を表示し続け、scan完了後に差し替えること。

## Task 1: Process snapshot parsing and app attribution

**Files:** `src/processes.rs` とそのunit test。

- [x] user UIDでprocess IDを絞り込むparser testと、footprint複数プロセス出力のparser testを追加する。
- [x] nested helper bundleを含むpath testを追加し、最上位`.app` bundleが選ばれることを確認する。
- [x] parser、libproc path lookup、Foundation display name lookupを実装する。

## Task 2: App aggregation and popup output

**Files:** `src/processes.rs`, `src/metrics.rs` とunit test。

- [x] 同一bundle内の2 processのfootprintを合算するtestを追加し、process単位の誤集計で失敗することを確認する。
- [x] aggregate後の上位3件制限とapp名表示のtestを追加する。
- [x] popup workerをcurrent-user processの一括snapshotに切り替え、cancelとtimeoutを維持する。
- [x] popupを「Top 3 Memory Consumers (apps grouped)」へ変更し、app名・合計memory・複数process数を表示する。
- [x] refresh中も最新の完了snapshotを表示する。
- [x] `make fmt-check check lint test`で検証する。
