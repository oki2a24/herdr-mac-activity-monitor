# Cross-cutting Review

調査日: 2026-09-21
対象: Repository Survey、Memory Research、Compute / Thermal Research、Power Researchの横断整理

## 1. Scope and Status

Technical Researchの各領域で確認した取得方式、sampling、履歴、private API、runtime dependency、権限、error handling、overhead、保守性を横断して整理する。

各個別Researchの詳細は以下を参照する。

- [Repository Survey](repository-survey.md)
- [Memory Research](memory.md)
- [Compute / Thermal Research](compute-thermal.md)
- [Power Research](power.md)

## 2. Common Existing Architecture

現行binaryはpopup processとwindow-title watch processに分かれ、どちらも3秒周期でmetricを取得する。各processはMemoryとBatteryを独立に取得し、共有state、履歴、channel、collector registryはない。

既存取得はmacOS標準commandのsubprocess中心である。

- Memory: `sysctl` 2回 + `vm_stat`
- Battery: `pmset -g batt`
- Herdr title: Unix socketへJSON-RPC

実機確認では、`sysctl vm.swapusage`と`ps`が通常権限で利用でき、`sudo powermetrics`では1秒sampleのCPU/GPU active residency、CPU/GPU power、thermal pressureを取得できた。

## 3. Acquisition Method Comparison

| 方式 | 利点 | 制約 |
| --- | --- | --- |
| macOS標準command | 既存実装と整合し、Rustからsubprocessで利用しやすい | spawn、text parse、format変更、権限、長時間samplerのcost。 |
| Apple public API / kernel interface | structured data、OS-defined state、subprocess削減の可能性 | Rust FFI、CF/Mach memory管理、OS version差、process-local state。 |
| private / undocumented API | Activity Monitorに近い値を得られる可能性 | update risk、fallback、保守cost。現時点ではRecommendationしていない。 |

同じmetricでも、値の意味が一致するとは限らない。例として、RSSとphysical footprint、swap activityとcurrent swap usage、thermal stateとtemperature、GPU active residencyとutilizationは別概念として扱う必要がある。

## 4. Sampling and History

### Snapshot候補

Memory used、available候補、Battery percentage、charging state、swap current usage、Thermal Stateはsnapshot取得候補である。ただしThermal Stateはnotification/eventでも扱える。

### Two-point / history候補

CPU utilizationはCPU tickの2時点差分、Battery drainはcapacityまたはcurrentの時間差分、Average Battery drainはwindow付き履歴を必要とする。

このため、すべてのmetricを現在の3秒同期pollだけに揃えると、履歴metricの意味が不安定になる。特にpopupとwatchが別processなので、同一の履歴を共有しない。

### Expensive sampler

`powermetrics`はCPU/GPU/ANE/power/thermalの複数値をまとめて取得できるが、共有実機で確認したのは`sudo`による1秒・1サンプルだけである。3秒常時実行、popupとwatchの二重実行、LLM負荷時のcostは未計測。

## 5. Runtime Dependency and Privilege

- 既存repositoryの直接Rust dependencyは`crossterm`のみ。
- IOPowerSources / ProcessInfo / Mach APIを使う場合、Foundation / IOKit / MachのFFIまたはbridgeが必要になる可能性がある。
- `powermetrics`は共有実機ではsudoで成功した。通常権限で必要samplerが動くかは未確認である。
- `ps`と`sysctl vm.swapusage`は共有された実機で通常権限により成功した。
- Researchの候補は、第三者monitoring applicationのインストールをruntime dependencyにしないことを前提とする。

## 6. Overhead Observations

既存実装ではpopupとwatchが同じcommandを別々にspawnする。新metric追加時に注意すべきcostは次の通り。

- subprocess spawnとstdout parse
- `powermetrics`の長いsample window、sudo、multiple sampler
- process一覧取得、RSS sorting、process churn
- IOKit / Mach / Foundation FFIの呼び出しとCF/Mach resource管理
- CPU tickやBattery drainの履歴保持
- popupとwatchでの重複sampling

現在のrepositoryではbenchmarkはなく、今回もcandidate方式の実測比較は実施していない。

## 7. Error and Unavailable Metric Handling

現行のfallbackはmetricごとに異なる。

- Memory: `Result`をcallerが`Option`へ変換し、popupは`Memory ?`、titleは`n/a`。
- Battery: 取得層で`Battery::Absent`へ吸収し、popupは`n/a`、titleも`n/a`。
- Herdr title送信: failure countを記録し、socket消失または閾値超過でwatchを終了。

追加metricでは、値が未取得、unknown、unlimited、権限不足、temporarily unavailable、staleの区別が必要になる。特にIOPS estimated runtimeはunknown / unlimitedが仕様上存在し、Thermal Stateはeventを受けるまで初期stateをどう扱うかが論点になる。

## 8. Metric Overlap and Information Value

- Memory used、available、pressure、swapは関連するが同じ値ではない。usedだけでpressureを代替しない。
- Thermal StateはOSの熱制約状態、temperatureは物理値、CPU/GPU powerは消費推定である。数値を増やしても判断情報が必ず増えるわけではない。
- GPU active residencyとGPU powerは同時に取得できる可能性があるが、active residencyの定義とpower推定の精度を分けて評価する。
- Battery percentage / stateは既存表示、estimated runtimeはOS推定、drainはworkloadの時間変化であり、相互に補完するが同一ではない。

## 9. Maintainability and macOS Update Risk

- 標準commandはtext output formatへの依存があるが、OSに標準搭載される。
- Public APIは意味と契約が明確になりやすいが、FFIとOS version availabilityを管理する必要がある。
- `powermetrics`の出力・sampler・権限はOS version / hardwareに依存し得る。実機出力をparseする場合は unavailable fallbackが必要。
- private sensor interfaceはResearch段階で採用確定していない。Design比較候補として確認したmacmonは、IOReport.framework / IOKit / CoreFoundationのprivate・低レベルAPIで通常権限取得を行うが、OS・hardware世代差と保守リスクがある。`get_metrics(interval_ms)`はinterval分blockingするため、高コストmetric向けの候補である。

## 10. Cross-cutting Recommendations

Research段階の横断Recommendationは次の通りである。

1. 既存Battery percentage / charging stateとMemory usedは維持候補として扱う。
2. Apple-defined stateであるThermal Stateは、直接温度を導入する前の有力候補である。
3. `powermetrics`はGPU active residencyとCPU/GPU powerの実機取得に成功したが、常時sampling・sudo・estimated valueの検証なしに採用確定しない。
4. current snapshotと履歴metricを同じ3秒pollとして扱うかは、Designで明示的に決める必要がある。
5. popupとwatchの二重取得があるため、expensive samplerは特にoverheadとprocess lifecycleを評価する必要がある。
6. unavailable / unknown / unlimited / staleを一律のzeroや推定値に置き換えない。

これらは最終architectureやUI設計ではなく、Designへ引き渡す判断材料である。

## 11. Remaining Questions for Design

- popupとwatchが同じmetricを取得する責務と、expensive samplerの重複実行をどう評価するか。
- 3秒更新を全metricへ適用するのか、event-driven / slower history samplingを分けるのか。
- IOKit / Foundation / Mach FFIを許容するか、標準command方式を維持するか。
- `powermetrics`のsudo依存をruntime要件として許容するか。
- RSS、physical footprint、GPU active residency、power estimateなどの値をどの名称・注記で表示するか。
- unknown / unavailable metricが存在する場合のpopup表示と既存window title維持方法。
- cross-cutting overheadを実機でどの候補セットについて計測するか。
