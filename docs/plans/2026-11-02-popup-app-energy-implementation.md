# ポップアップのアプリ別エネルギー表示 実装計画

> **AIエージェントへの指示:** タスクごとにTDDを行い、実装前にテストの失敗を確認する。各ステップは進捗に合わせてチェックする。

**目標:** ポップアップに、権限不要で読めるプロセス別エネルギーカウンターの差分から、アプリ単位で合算した上位3件を表示する。

**アーキテクチャ:** ポップアップ専用の既存プロセスworkerで現在ユーザーのPIDを列挙し、既存メモリ一覧とエネルギーカウンターを同じsnapshotで取得する。プロセス開始時刻と累積nJの前回値をworker内で保持し、比較可能な差分を`.app` bundle単位に合算して平均mWへ変換する。

**技術スタック:** Rust 2021、macOS `libproc` (`proc_pid_rusage` / `RUSAGE_INFO_V6`)、既存Makefile検証ターゲット。

**仕様 (Spec):** [2026-11-02-popup-app-energy-design.md](2026-11-02-popup-app-energy-design.md)

## グローバル制約 (Global Constraints)

- サンプリングはポップアップ表示中に既存の更新間隔で行う。
- 管理者権限は求めず、ユーザー自身のプロセスから読める値を対象とする。
- 初回および新規PIDの読取は基準値として保存し、比較可能な次回以降から計測値に含める。
- PID再利用やカウンターの逆行を差分として扱わない。
- 取得できないプロセスの値は除外し、0として補完しない。
- Activity MonitorのEnergy Impactスコアそのもの、システム全体の消費量、popup終了後の履歴保存は対象外。
- メモリ・バッテリー・CPU・Thermal表示とwindow titleは維持する。

## レビューフォーカス (Review Focus)

- 初回sampleまたは新しいPIDでは、過去の累積分を使用量として誤計上せず基準値だけを保存する。
- PIDが再利用された場合やカウンターが減少した場合、負の差分や古いプロセスとの差分を表示しない。
- 同じアプリのhelperを合算してから順位付けし、アプリ上位3件を返す。
- workerの更新間隔が揺れても、差分エネルギーを実際の経過時間で割って比較する。
- 一部または全プロセスの取得に失敗した場合、取得不能を0 mWとして表示しない。

## タスク 1: プロセスエネルギー差分とアプリ集計

**ファイル:** `src/processes.rs` とそのinline unit tests。

**インターフェース:**

- `ProcessEnergySample { pid: u32, started_at: u64, energy_nj: u64, name: String, application: Option<AppBundle> }` は1回の取得値を表す。
- `EnergySampler::update(sampled_at: Instant, current: Vec<ProcessEnergySample>) -> EnergySampleState` は前回のPID・開始時刻・累積カウンターを保持し、比較可能な値をアプリ単位の`EnergyConsumer { name, average_mw, process_count }`へ集約して返す。
- 最初のsampleと比較できるプロセスがないsampleは`EnergySampleState::Sampling`、比較できる値が得られたsampleは`EnergySampleState::Measured(Vec<EnergyConsumer>)`を返す。
- 集計前のデータをテストするため、`ProcessEnergySample`と`EnergySampleState`は`pub(crate)`とする。`AppBundle`は既存のpath/nameを再利用する。

- [x] **ステップ 1: アプリ集計・時間差分・PID境界を検査する失敗テストを追加**

`energy_sampler_aggregates_before_ranking_and_returns_average_milliwatts`では、同じbundleの4 PIDが各100,000,000 nJ、別アプリが350,000,000 / 300,000,000 / 250,000,000 nJを2秒間に使った入力を与え、結果が合算後の200 / 175 / 150 mWの順で上位3アプリとなることを確認する。先にPID単位で上位3件を切る誤実装と昇順ソートを検出する。

`energy_sampler_uses_elapsed_time_and_ignores_new_reused_or_reset_pids`では、前回sampleからの差分だけが経過秒数で割られること、初回PID・開始時刻の異なる同一PID・減少したカウンターが結果に含まれないことを確認する。

- [x] **ステップ 2: 対象テストを実行して期待した失敗を確認**

実行: `make test`

期待値: 新しい`ProcessEnergySample` / `EnergySampler` APIが未実装のため、対象テストの参照が解決せず失敗する。他の失敗があれば原因を確認し、今回の変更と無関係な既存失敗として記録する。

- [x] **ステップ 3: `src/processes.rs` にエネルギーサンプラーを実装**

前回値との照合キーをPIDと`started_at`にし、`checked_sub`できる累積nJだけを使う。アプリbundle単位でdeltaを合算してから平均mWを算出し、降順・名前順で安定化して3件に切る。bundleのないプロセスはPIDごとに独立した項目にする。

- [x] **ステップ 4: GREENを確認し、代表的な誤実装の検出力を点検**

実行: `make test`

期待値: 対象テストと既存テストがPASSする。確認用の隔離コピーで集計順を「合算後」から「個別プロセス上位3件」へ変え、集計順テストがアサーションでFAILすることを確認する。隔離コピーを破棄し、元のコードで`make test`がPASSすることを確認する。

## タスク 2: 取得worker、ポップアップ表示、要件の反映

**ファイル:** `src/processes.rs`, `src/metrics.rs`, `docs/requirements/activity-monitor-popup.md` とそれぞれのinline unit tests。

**インターフェース:**

- `scan_popup_processes(cancel: Option<&AtomicBool>) -> Option<PopupProcessScan>` は既存のPID列挙を1回だけ行い、memory結果と`Option<Vec<ProcessEnergySample>>`を返す。Energy sampleは現在ユーザーのPIDについて`proc_pid_rusage(pid, RUSAGE_INFO_V6, ...)`から読む。読めないPIDは除外し、全件取得不能はenergy側の`None`にする。
- `PopupSnapshot`に`energy_usage: EnergyUsage`を追加する。`EnergyUsage`は`Unavailable`、`Sampling`、`Measured(Vec<EnergyConsumer>)`を表す。
- 既存`ProcessScanner` workerが`scan_popup_processes`と`EnergySampler`を使い、最新のmemory値とenergy値を独立して保持する。
- 表示見出しは `Top 3 App Energy Use` とする。計測中・取得不能・測定値ありを区別し、測定値はアプリ名・平均mW・複数PID数を表示する。

- [x] **ステップ 1: popup表示の失敗テストを追加**

`popup_displays_top_app_energy_usage_in_milliwatts`で見出しと`  Safari — 2.5 mW (2 processes)`の表示を確認する。`popup_shows_sampling_before_first_energy_delta`と`popup_marks_unavailable_energy_without_zero`で計測中と取得不能を別の文言にすることを確認する。`popup_does_not_round_small_positive_energy_to_zero`で0.04 mWを0.0 mWと表示しないことを確認する。

macOS専用の`rusage_v6_extracts_process_start_and_energy_counter_fields`では、SDKの`rusage_info_v6`にある`ri_proc_start_abstime`と`ri_energy_nj`の位置へ異なるfixture値を置き、実際に使うdecode関数が正しい両値を読むことを確認する。

- [x] **ステップ 2: 対象テストを実行して期待した失敗を確認**

実行: `make test`

期待値: 新しい`RusageInfoV6`、`decode_energy_fields`、`PopupSnapshot.energy_usage`が未実装のため、対象テストがコンパイル時に失敗する。他の失敗があれば原因を確認する。

- [x] **ステップ 3: 同一PID列挙からメモリとenergyを取得し、workerへ接続**

`src/processes.rs`のmacOS FFIをSDKの`rusage_info_v6` layoutと`RUSAGE_INFO_V6` flavorに合わせる。PID列挙結果をmemory/energyの両方へ使い、別個の`ps` scanを追加しない。取得に失敗したエネルギー値は`None`として返す。

- [x] **ステップ 4: popup snapshot・状態表示とPower要件を実装**

`src/metrics.rs`の既存workerにsample trackerを置き、popupが閉じた後に履歴を保持しない。現在のmemory結果保持動作を維持し、energyは`Sampling` / `Unavailable` / `Measured`を明示する。要件文書のPower項目へアプリ別エネルギー上位3件、取得範囲、Energy Impact scoreとは異なる値であることを追記する。

- [x] **ステップ 5: GREENとLevel 1検証を確認**

実行: `make test`、続けて `make fmt-check check lint test`

期待値: popup unit testsと全suiteがPASSし、fmt・check・clippyにエラーや警告がない。失敗があれば原因を分類し、今回の変更に起因する場合は修正して該当検証とLevel 1を再実行する。

## セルフレビュー

- `EnergySampler`の純粋な差分計算とOS依存取得を分離し、FFIをロジックテストへ混ぜない。
- workerは既存のpopup専用threadを再利用し、memoryとenergyのPID列挙を共有する。
- テストはhelper合算前のtruncate、昇順、累積値の誤用、PID再利用、取得不能を検出する。
- 検証コマンドはプロジェクト指定のMake targetに限定し、Level 1を最後に実行する。
