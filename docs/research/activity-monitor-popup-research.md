# Activity Monitor Popup Technical Research

この文書はActivity Monitor Popup Technical Researchのliving summaryである。詳細なRepository Surveyは[repository-survey.md](activity-monitor-popup/repository-survey.md)を参照する。

## Research Status

- [x] Repository Survey
- [x] Memory Research
- [x] Compute / Thermal Research
- [x] Power Research
- [x] Cross-cutting Review

## Current Findings

- 現在のbinaryは`--watch`でwindow title更新daemon、引数なしでpopup TUIとして動作する。
- Memoryは`sysctl` 2回と`vm_stat`、Batteryは`pmset -g batt`を各processが3秒周期で取得する。両process間に共有stateや履歴はない。
- Memory/Batteryは`parsers.rs`のデータ型とformatterを共有する。window titleはHerdr Unix socketへのJSON-RPC、popupはmanifestのpane command stdout/TUIであり、transportは別である。
- 現在の直接依存は`crossterm`のみで、metric取得はRust crateではなくmacOS標準commandのsubprocess実行である。
- 詳細なfile/function、error handling、tests、overhead観察、後続Researchの問いは[Repository Survey](activity-monitor-popup/repository-survey.md)に記録した。
- Memory Researchでは、既存の`vm_stat`経路をMemory usedの採用候補として確認した。共有された実機で`sysctl vm.swapusage`と`ps -axo pid=,rss=,comm=`が成功し、swap current usageとtop process RSSの取得可能性を確認した。Memory PressureはApple public APIがあるがevent-drivenで、RSS / physical footprint、swap発生時の挙動、costは引き続き確認が必要である。詳細は[Memory Research](activity-monitor-popup/memory.md)を参照する。
- Compute / Thermal Researchでは、Apple publicなThermal Stateは採用候補、CPU utilizationは条件付き採用候補、GPU active residencyとCPU/GPU powerは実機取得に成功したため条件付き採用候補、温度は要追加調査とした。詳細は[Compute / Thermal Research](activity-monitor-popup/compute-thermal.md)を参照する。
- Power Researchでは、既存のBattery percentageとcharging stateは採用候補、OS estimated runtimeは条件付き採用候補、Battery drainと平均drainは要追加調査とした。詳細は[Power Research](activity-monitor-popup/power.md)を参照する。
- Cross-cutting Reviewでは、snapshot / two-point history / expensive samplerを分けて評価する必要、popup/watchの二重取得、FFI・sudo・estimated value・unavailable handlingをDesignへ引き渡す事項として整理した。詳細は[Cross-cutting Review](activity-monitor-popup/cross-cutting.md)を参照する。
- 共有された実機で`sudo powermetrics -n 1 -i 1000 -s cpu_power,gpu_power,thermal`が成功し、CPU/GPU active residency、CPU/GPU power、thermal pressure（Nominal）を取得できた。GPU utilizationとCPU/GPU powerは条件付き採用候補へ更新したが、常時sampling costと負荷時の妥当性は未確認である。
- Design段階の追加確認としてmacmonの実装を確認した。macmonはsudo不要でIOReport.framework等からCPU/GPU/ANE power、residency、RAM、swap等を取得するが、private・低レベルAPI依存で、指定intervalのsamplingをblockingする。public API優先、macmonは高コストmetricの方式比較候補とする。
- Battery駆動状態の追加確認で、`IOPSGetTimeRemainingEstimate`が`37560`秒（626分）を返し、同時刻の`pmset`の`10:26 remaining`と一致した。約2分後には`38760`秒（646分）へ変化したため、Estimated Battery runtimeは初期版の条件付き候補として扱い、数値・`unknown`・`unlimited`を区別する。
- Research-only Objective-C probesで、IOPowerSourcesからbattery capacity `78/100`、Battery Power、charging false、estimated runtime `unlimited`、time-to-empty `793`を取得し、Foundation `thermalState`から`nominal`を取得できることを確認した。production codeは変更していない。

## Feasibility Matrix

Power Researchまで完了した。Cross-cutting Review以外は未調査項目を推測で埋めない。

| Metric | Purpose / Decision | Feasibility | Source / API | Public / Private | Privilege | Runtime dependency | Sampling / Cost | Reliability | Recommendation |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Memory used | Unified Memory負荷 | 取得可能性を確認 | `vm_stat`（現行実装） | 未確定 | 未確定 | macOS標準command | 既存3秒poll、subprocess 3回 | 既存動作あり。text parse依存 | 採用候補 |
| Memory available | Unified Memory余力 | 未調査 | 未調査 | 未調査 | 未調査 | 未調査 | 未調査 | 未調査 | 未調査 |
| Memory Pressure | macOSのmemory状態 | public event APIを確認、統合は未検証 | DispatchSource memory pressure | Public | 未確認 | Rust FFI候補、追加command不要の可能性 | event-driven候補、cost未計測 | 初期値・coalesce・FFI未検証 | 条件付き採用候補 |
| Swap使用量 | 退避発生状況 | `sysctl vm.swapusage`取得成功、swap発生時は未検証 | `sysctl vm.swapusage`、`vm_stat` counters | 未確定 | 共有実機では通常権限で成功 | macOS標準command | current snapshot候補、cost未計測 | idle時0.00Mを確認 | 条件付き採用候補 |
| Top memory processes | 使用原因の手掛かり | `ps` RSS一覧取得成功 | `ps` RSS / rusage候補 | 未確定 | 共有実機ではtop 20取得成功 | `ps`またはAPI候補 | process scan・sorting cost未計測 | RSSとfootprintの意味差 | 条件付き採用候補 |
| CPU utilization | 計算資源使用状況 | 取得候補あり、統合未検証 | `host_processor_info` / `powermetrics`候補 | Public候補 / command | 未確認 | Mach FFIまたは標準command | 2点tick差分、cost未計測 | CPU topology・FFI未検証 | 条件付き採用候補 |
| GPU utilization | 計算資源使用状況 | active residency取得成功 | `powermetrics` GPU active residency | command | sudoで成功 | subprocess候補 | cost未計測 | 1秒sampleで8.09%を確認 | 条件付き採用候補 |
| Thermal State | 熱による制約 | public APIを確認 | `ProcessInfo.thermalState` | Public | 未確認 | Foundation FFI候補 | snapshot / notification候補 | Apple-defined state | 採用候補 |
| CPU temperature | 熱状態の追加材料 | 未確定 | `powermetrics`等 | 未確定 | 未確認 | command候補 | cost未計測 | sensor・version依存未検証 | 要追加調査 |
| GPU temperature | 熱状態の追加材料 | 未確定 | `powermetrics`等 | 未確定 | 未確認 | command候補 | cost未計測 | sensor・version依存未検証 | 要追加調査 |
| CPU power | 電力・熱の追加材料 | estimated sampler取得成功 | `powermetrics cpu_power` | command | sudoで成功 | subprocess候補 | cost未計測 | 1秒sampleで370 mWを確認 | 条件付き採用候補 |
| GPU power | 電力・熱の追加材料 | estimated sampler取得成功 | `powermetrics gpu_power` | command | sudoで成功 | subprocess候補 | cost未計測 | 1秒sampleで42 mWを確認 | 条件付き採用候補 |
| Battery percentage | 残量 | 既存取得済み | `pmset -g batt` / IOPowerSources | 標準command / Public API | 未確認 | subprocessまたはIOKit FFI | 既存3秒poll | 現行動作あり | 採用候補 |
| Charging / discharging state | 電源状態 | 既存取得済み | `pmset -g batt` / IOPowerSources | 標準command / Public API | 未確認 | subprocessまたはIOKit FFI | 既存3秒poll | 現行動作あり | 採用候補 |
| Battery drain | 消費速度 | 履歴差分候補、未確定 | percentage差分 / IOPowerSources current候補 | Public候補 | 未確認 | 履歴またはIOKit FFI | 履歴必要、cost未計測 | noise・transition未検証 | 要追加調査 |
| Average Battery drain | 平均消費速度 | 履歴window候補、未確定 | percentage差分の移動平均 | 実装metric | 未確認 | process-local history等 | window・履歴必要 | lifecycle未検証 | 要追加調査 |
| Estimated Battery runtime | 継続可能時間 | OS estimate取得候補 | `IOPSGetTimeRemainingEstimate` | Public API | 未確認 | IOKit FFI候補 | snapshot / notification候補 | unknown / unlimitedあり | 条件付き採用候補 |

## Current Recommendations

Power Researchまで完了。Memory used・Thermal State・既存Battery percentage / charging stateは採用候補。CPU utilization・GPU utilization・CPU/GPU power・Memory Pressure・Swap使用量・top memory processes・Estimated Battery runtimeは条件付き採用候補。Memory available、temperature、Battery drain、Average Battery drainは要追加調査。これは最終採否ではない。

## Unresolved Questions

- Memory Pressure、Swap、available、top processを取得する具体的な方式と信頼性は何か。
- CPU/GPU utilization、Thermal State、temperature、powerの取得方式・API種別・権限・costは何か。
- Battery drainと推定runtimeに必要な履歴を、現在のprocess-localな3秒poll構成でどう評価するか。
- popupとwindow titleの別process取得による重複cost、および新metric追加時のsampling・error handlingの制約は何か。
- `prefix + m`のtriggerとpane lifecycleの契約をHerdr側仕様でどう確認するか。
- Memory availableの定義、swap発生時の`vm.swapusage`の挙動、RSSとphysical footprintの選択、process grouping、pressure eventの初期状態とFFIを追加確認する必要がある。
- CPU utilizationのMach tick差分、GPU utilizationの取得方式、Thermal StateのRust integration、powermetricsの権限・cost・estimated valueの扱いを追加確認する必要がある。
- `powermetrics`の実機結果は1秒・1サンプルのみであり、LLM workload時の変化、常時sudo依存、process spawnとpower overheadは未確認である。
- IOPowerSourcesの実機fields、estimated runtimeのunknown / unlimited、Battery drainの履歴・noise・charging transition・process lifecycleを追加確認する必要がある。

詳細なKnown / Unknown / Evidence / Implicationは[Repository Survey](activity-monitor-popup/repository-survey.md)の各節とOpen Questionsを参照する。

## Next Step

Technical Researchは完了。次はDesignで、各metricの最終採否、取得方式、sampling、popup表示、既存window title維持を決定する。
