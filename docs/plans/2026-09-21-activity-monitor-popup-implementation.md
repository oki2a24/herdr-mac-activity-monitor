# Activity Monitor Popup 実装計画

> **AIエージェントへの指示:** REQUIRED SUB-SKILL: この計画をタスクごとに実装するには、`subagent-driven-development` または `executing-plans` スキルを起動して使用してください。

**目標:** 既存のwindow title表示を完全に維持したまま、確定済みのMemory、Compute / Thermal、Battery診断情報を`prefix + m` popupへ追加する。

**アーキテクチャ:** 既存のwatch / popupのprocess境界を維持し、window title用のMemory / Battery取得経路をpopup詳細metricへ混ぜない。軽量metricは各processで独立取得し、高コストmetricはpopup表示中だけpopup側で取得する。方式比較で採用したmetricだけを実装し、取得不能なmetricは他のmetricやwindow titleを停止させない。

**技術スタック:** Rust、既存のHerdr JSON-RPC / crossterm経路、macOS public API、必要に応じたmacOS FFI、既存の標準command subprocess。

**仕様 (Spec):**
- `docs/requirements/activity-monitor-popup.md`
- `docs/research/activity-monitor-popup-research-requirements.md`
- `docs/plans/2026-09-21-activity-monitor-popup-design.md`
- `docs/research/activity-monitor-popup-research.md`

**グローバル制約 (Global Constraints):**
- window titleの表示内容と更新動作を完全に維持する。
- production codeの変更前に、metric取得方式、権限、sampling cost、failure handlingの比較結果をDesignへ反映する。
- 通常権限で動作する方式を優先し、runtime中のsudo / 再承認を必要とする方式を原則採用しない。
- metric取得不能時にzeroや推測値で代替しない。
- 高コストmetricはwatchで取得せず、popup表示中だけ取得する。
- private APIは前提にしない。macmon / IOReport方式は初期版で採用しない。
- production code、Requirements文書、Technical Research Requirements文書を変更しない。
- コミットはユーザーのレビュー後に行う。

---

## 1. 確定済みの方式選択

方式比較はDesign段階で完了した。production codeでは、以下の確定結果だけを実装する。比較結果の根拠はDesign文書とResearch文書を参照する。

### 比較対象と初期優先順位

| Metric | 採用方式 | 除外・代替方式 | 実装方針 |
| --- | --- | --- | --- |
| Memory used | 既存`vm_stat`経路 | Mach VM API / macmon RAM | titleとpopupで同じ計算式を使い、独立取得による時刻差を許容 |
| Memory Pressure | `DispatchSource` memory-pressure event | snapshot command / `vm_stat`推定 | event受信前はunavailable、独自推定しない |
| Swap usage | `sysctl vm.swapusage` | macmon `libc_swap`, `vm_stat` counters | current snapshotのused / total、0も有効値、履歴なし |
| Top memory processes | 全PID候補 + `footprint` `phys_footprint` | RSS表示 | 取得不能PIDを除外し、取得成功分のTop 3をpopup-only表示 |
| CPU utilization | Mach tick差分 | powermetrics / macmon residency | popup process内に1 sample履歴、初回・差分不正・sleep/wake後はunavailable |
| Thermal State | `ProcessInfo.thermalState` snapshot | notification | public API、sudo不要、既存3秒loopへsnapshot統合 |
| Battery % / state | `IOPowerSources` | `pmset` fallback | fallbackなし。既存`Battery`型・formatter・window titleを維持 |
| Estimated runtime | `IOPSGetTimeRemainingEstimate` | 独自計算 | 数値、unknown、unlimitedを区別し、独自計算しない |

### 初期版の除外

CPU / GPU powerとGPU utilizationは初期版の対象外とする。powermetricsのsudo / sampling cost、macmon / IOReportのprivate API依存を初期版へ導入しない。Memory available、温度、Battery drain、平均drainも対象外とする。

---

## 2. タスク一覧

### タスク1: 現行経路の実装前ベースラインを固定する

**ファイル:**
- 参照: `src/main.rs`, `src/watch.rs`, `src/memory.rs`, `src/battery.rs`, `src/parsers.rs`, `src/tui.rs`, `src/herdr.rs`
- テスト: 既存test群

- [ ] 既存のwindow title文字列、3秒更新、Herdr request形状をソースとテストから記録する。
- [ ] `make verify`を実行し、実装前の38 testsの結果を保存する。
- [ ] `git diff --check`と`git status --short`で対象外の変更がないことを確認する。

### タスク2: 確定済み方式と実装境界を確認する

**ファイル:**
- 作成/変更: `research/experiments/`
- 参照: `docs/research/activity-monitor-popup/`

**Produces:** 実装者が参照する確定済みmetric一覧と、window titleへ影響させない境界。

- [ ] Design文書の確定metric一覧と除外一覧を実装タスクの入力として確認する。
- [ ] `watch`には既存Memory / Battery以外のmetricを追加しないことを確認する。
- [ ] `popup`の新規metricをsnapshot、event state、two-point historyのいずれかに分類する。

### タスク3: metric contractと取得状態を定義する

**ファイル:**
- 作成候補: `src/metrics.rs`または既存module内のmetric型
- テスト: metric状態・formatのunit test

**Produces:** metric単位の値と状態を表現する型。少なくとも`Present`、`Unavailable`、`Unknown`、`Unlimited`を区別する。

- [ ] 実装するmetricの最終一覧と表示ラベルをDesign / comparison recordから転記する。
- [ ] `window title`用の既存Memory / Battery型とformatterを変更せず、popup用型を定義する。
- [ ] unknown / unlimited / unavailableをzeroへ変換しないunit testを先に作成する。
- [ ] `make verify`で型とunit testを確認する。

### タスク4: 軽量metricのcollectorを実装する

**ファイル:**
- 作成/変更: `src/metrics/`またはタスク3で確定したcollector module
- テスト: parser / conversion unit test、macOS依存部を除くfailure test

- [ ] Memory Pressure、swap、CPU utilization、Thermal State、Battery候補を比較結果どおりに実装する。CPU utilizationはMach tick差分をpopup process内で保持し、初回・差分不正・sleep/wake後を`unavailable`とする。Thermal Stateは`ProcessInfo.thermalState` snapshotを使用し、notificationを必須にしない。
- [ ] Memory usedは既存のwindow titleとpopupで同じ計算式を使うが、process間の取得時刻差による数値差を許容する。Memory Pressureは別stateとしてpopup process内に保持し、event受信前は`unavailable`とする。
- [ ] Swapは`sysctl vm.swapusage`のcurrent snapshotだけをpopupで取得し、`vm_stat`累積counterや履歴差分を使用しない。
- [ ] BatteryはIOPowerSources raw snapshot取得と`Battery` / `ChargingState`への変換を分離し、既存formatterとwindow title経路を変更しない。API failureは`Battery::Absent`へ変換し、`pmset` fallbackは実装しない。
- [ ] `pmset`専用parser / fixture testを削除または変換テストへ置換し、Battery formatterとwindow titleの既存文字列テストは維持する。
- [ ] public API / FFIのerrorをmetric単位の状態へ変換する。
- [ ] CPU utilization等の履歴metricは、sample timestampと前回値がprocess-localであることを明記する。
- [ ] 既存watch collectorへ詳細metricを追加せず、popup側から明示的に呼び出す。
- [ ] unit testとmacOS実機確認を実行する。

### タスク5: Top memory processesを実装する

**ファイル:**
- 作成/変更: `src/processes.rs`またはcollector module
- テスト: command parser、sorting、process消滅、空結果、permission failure

- [ ] 全PIDを候補にし、`footprint`の`phys_footprint`だけを取得する。permission / process churnで取得できないPIDは除外し、RSSは表示値に使わない。
- [ ] PID、表示名、memory value、取得失敗を型として保持する。
- [ ] process一覧取得、`footprint`実行、`phys_footprint` parse、sort、上位件数制限を分離する。
- [ ] permission / process churnでpopup全体が停止しないunit testを追加する。

### タスク6: popup lifecycleとmetric failure isolationを実装する

**ファイル:**
- 作成/変更: popup起動・更新module、必要なら`src/collectors/`
- テスト: popup lifecycle、停止、timeout、failure isolation

- [ ] popup表示開始時にpopup-only collectorを初期化し、watchから詳細metricを呼ばない。
- [ ] event state、snapshot、two-point historyの各collectorをpopup lifecycleへ接続する。
- [ ] popup終了時にevent source、worker、child processが残らないことを確認する。
- [ ] permission failure、API failure、process churn、初回sampleをmetric単位の状態へ変換し、既存Memory / Battery表示を継続する。

### タスク7: popup formatterと表示を追加する

**ファイル:**
- 変更: popup表示module、既存`src/tui.rs`またはpopup formatter
- テスト: 行構成、状態表示、長いprocess名、欠損metric

- [ ] metric contractからpopup行を生成する。
- [ ] 表示順をMemory（used / Pressure / Swap）、Top memory processes（上位3件）、Compute / Thermal（CPU / Thermal）、Power（Battery / Runtime）に固定し、領域間に空行を入れる。
- [ ] existing Memory / Batteryの表示内容を維持する。
- [ ] `unavailable`、`unknown`、`unlimited`を明示的な文字列で表示する。
- [ ] process名のtruncateと固定幅を既存terminal rendering規則に合わせる。
- [ ] window title formatterとHerdr transportを変更しないことをテストで確認する。

### タスク8: 統合検証と実機確認を行う

**ファイル:**
- 参照: `docs/development/verification.md`, `Makefile`

- [ ] `make verify`を実行する。
- [ ] popupを閉じた状態で高コストsamplerが起動しないことを確認する。
- [ ] popup表示中にmetricが更新され、popup終了後にsamplerが残らないことを確認する。
- [ ] window titleの文字列、更新周期、Herdr連携が変更されていないことを確認する。
- [ ] metric取得失敗時もwatchとpopupの他行が継続することを確認する。
- [ ] `git diff --check`と対象ファイル一覧を確認し、production code以外の不要な変更がないことを確認する。

## 3. 実装前のレビューゲート

production codeへ進む前に以下を確認済みとする。

- 各metricの採用方式が1つに決まっている。
- macmonを初期版で採用しないことが明示されている。
- sudo / runtime再承認を必要とする方式が除外されている。
- popup-only境界が決まっている。
- unavailable / unknown / unlimitedの表示契約が決まっている。
- window titleの既存経路を変更しないことが確認されている。

このゲートは完了済みであり、ユーザーのDesignレビュー後に実装を開始する。

## 4. 完了条件

- Designで承認された対象範囲だけが実装されている。
- window titleの表示内容と更新動作が完全に維持されている。
- 高コストmetricはpopup表示中だけ取得される。
- 取得失敗がmetric単位に局所化されている。
- 方式比較結果と採用理由がResearch文書へ記録されている。
- `make verify`が成功している。
- ユーザーのレビュー前にコミットしていない。
