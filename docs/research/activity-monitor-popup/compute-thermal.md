# Compute / Thermal Research

調査日: 2026-09-21
対象: CPU utilization、GPU utilization、Thermal State、CPU/GPU temperature、CPU/GPU power

## Scope

Apple Silicon macOSでの取得可能性、既存repositoryとの関係、public/private API、権限、subprocess、sampling、overhead、信頼性を調査した。最終UI、sampling architecture、production実装は対象外とした。macmonの詳細比較はDesign段階の比較候補として扱う。

## Executive Summary

- Appleの`ProcessInfo.thermalState`はpublic APIで、nominal / fair / serious / criticalのsystem thermal stateを返す。thermal stateの変化通知もpublicに提供される。[Apple ProcessInfo](https://developer.apple.com/documentation/foundation/processinfo)
- CPU utilizationはMachの`host_processor_info`等から取得するpublic kernel interface候補がある。system-wide utilizationは2時点のCPU tick差分が必要で、単発値ではない。[Apple host_processor_info](https://developer.apple.com/documentation/kernel/1502854-host_processor_info)
- macOS標準の`powermetrics`はCPU、GPU、ANE等のpower推定、thermal sampler、CPU frequency等を提供する。ただし出力には推定値が含まれ、常時3秒pollへ直接組み込む方式のcost・permission・format stabilityは未検証。
- 共有された実機で`sudo powermetrics -n 1 -i 1000 -s cpu_power,gpu_power,thermal`が成功し、CPU active residency、GPU HW active residency、CPU/GPU power、thermal pressureを1秒sampleで取得できた。GPU active residencyはGPU usageの候補として扱える可能性がある。
- CPU/GPU powerは取得できるが、1秒sampleの推定値であり、Requirements上も上位のThermal StateやPower情報に追加価値がある場合だけ表示候補である。

## 1. Existing Repository Constraints

現在のbinaryはRustで、直接依存は`crossterm`のみ。metric取得は各processの3秒同期loop内で外部commandをspawnする構造である。既存の`memory.rs` / `battery.rs`にCompute / Thermal取得層はなく、popupとwatchは別processで同じ取得を独立実行する（[Repository Survey](repository-survey.md)）。

新しい値を既存表示へ渡す場合の現行接点は、データ表現・formatが`src/parsers.rs`、取得呼び出しが`src/tui.rs`と`src/watch.rs`、window titleの更新が`src/herdr.rs`である。ここでは設計変更を行わない。

## 2. Thermal State

Appleの`ProcessInfo.thermalState`は「current thermal state of the system」を返すpublic propertyである。値はnominal、fair、serious、criticalで、Appleはcriticalをsystem performanceへ大きく影響する状態、seriousをhigh stateとして説明している。[thermalState](https://developer.apple.com/documentation/foundation/processinfo/thermalstate-swift.property)、[ProcessInfo.ThermalState](https://developer.apple.com/documentation/foundation/processinfo/thermalstate-swift.enum)

thermal stateの変化には`thermalStateDidChangeNotification`があり、Appleは通知登録前に一度`thermalState`へアクセスする必要があると説明している。[thermalStateDidChangeNotification](https://developer.apple.com/documentation/foundation/processinfo/thermalstatedidchangenotification)

これはtemperatureの数値ではなく、OSが定義した段階状態である。現行Rust binaryにはFoundation binding、notification state、FFI境界がない。

**Recommendation: 採用候補**。Requirementsの「熱による制約」を判断する材料として、CPU/GPU温度より直接的で、Apple-defined stateかつpublic APIである。FFIまたは標準commandの実装costとpopup/watch間のstate扱いはDesign前に確認する。

### Research probe result

Research-only Objective-C probeでFoundation `NSProcessInfo.processInfo.thermalState`を取得できることを確認した。共有実機の結果はraw value `0`、`nominal`だった。probe sourceは`research/experiments/power-thermal-probes/`に保存した。

Foundation framework linkのみでprobeをbuildでき、追加runtime applicationは不要だった。別state（fair / serious / critical）への遷移は今回のprobeでは発生させていない。

## 3. CPU utilization

Appleの`host_processor_info`はprocessor情報を取得するMach kernel functionである。[host_processor_info](https://developer.apple.com/documentation/kernel/1502854-host_processor_info)

CPU utilizationは通常、CPUごとのuser/system/idle等の累積tickを2回取得し、interval差分から割合を算出する種類のmetricである。したがって単一snapshotではなく、少なくとも前回sampleを保持する必要がある。

macOS標準`powermetrics`もCPU usage statistics、CPU frequency distribution等を出力対象に含むが、直接Mach APIから算出する値と同じ意味とは限らない。

| 方式 | 性質 | 主な制約 |
| --- | --- | --- |
| Mach `host_processor_info` | public kernel interface候補。FFIで直接取得 | tick差分、memory deallocation、CPU topology、OS version差を扱う必要。 |
| `top`等の標準command | subprocessのtext output | 今回の実行環境では`top`がpermission error。format・spawn cost依存。 |
| `powermetrics` | CPU usageとpower/frequencyをまとめて取得可能 | privilege、long-running sampler、推定値、output costを確認要。 |

**Recommendation: 条件付き採用候補**。判断価値は高くpublic kernel interface候補もあるが、2点サンプリングとFFIの実装・costを確認する必要がある。標準commandを3秒ごとにspawnする方式は現時点で優先しない。

## 4. GPU utilization

今回確認したApple public documentationの範囲では、Apple Silicon GPUのsystem-wide utilizationを単純snapshotとして返す一般向けpublic APIは確定できなかった。Metal APIはアプリ自身のGPU workを扱うAPIであり、system-wide monitoring metricの直接APIとは別である。

共有された実機の出力には`GPU HW active residency: 8.09%`、`GPU idle residency: 91.91%`、GPU power `42 mW`が含まれていた。したがって、少なくとも対象環境の`powermetrics`ではGPU activityに相当する値を取得できる。ただし、これは1秒・1サンプルであり、GPU utilizationという表示名・負荷との対応・LLM workloadでの診断価値は未確定である。

**Recommendation: 条件付き採用候補**。実機でGPU active residency取得を確認できた。一方、sudo権限、estimated powerとの同時取得cost、sample interval、GPU workload時の変化、`active residency`をutilizationとして表示する妥当性を追加確認する必要がある。

## 5. CPU / GPU temperature

RequirementsでもThermal StateやPower情報に追加の判断材料を提供する場合に限る候補である。今回確認したpublic Apple APIでは、`ProcessInfo.thermalState`は段階状態を提供するが、CPU/GPU温度の数値は提供しない。

共有された出力ではthermal pressureは`Nominal`だったが、CPU/GPU温度の数値は出力されなかった。`powermetrics`はsamplerとmacOS versionにより出力項目が変わり得るため、温度の具体的な取得方式は未確定である。

**Recommendation: 要追加調査**。Thermal Stateと重複する可能性があり、具体的温度の取得方式・信頼性・costを確認してから判断する。private sensor interfaceを現時点でRecommendationしない。

## 6. CPU / GPU power

`powermetrics`のhelp outputには`cpu_power`、`gpu_power`、`ane_power` samplerがあり、共有された実機でも`CPU Power: 370 mW`、`GPU Power: 42 mW`、`Combined Power: 411 mW`を表示した。一方、ツール自身がaverage powerはestimatedでdevice間比較には不正確になり得ると説明している。

RequirementsはCPU/GPU powerを、上位のThermal StateやPower関連情報に追加の判断材料を提供できる場合に限って検討する。現在のrepositoryはBattery情報も標準`pmset` subprocessで取得しているが、power historyやenergy stateは保持していない。

**Recommendation: 条件付き採用候補**。実機で取得可能性を確認できた。ただしsudo/root権限、estimated valueの意味、LLM workloadでの変化、常時sampling costが未確認であり、最終採否は保留する。

### macmon / IOReport方式（Design比較候補）

macmonの実装は、`IOReport.framework`のFFIとIOKit / CoreFoundationを使い、CPU/GPU/ANE power、active residency、RAM、swap等を通常権限で取得する。`Sampler::get_metrics(interval_ms)`は指定intervalのsample完了までblockingするため、高コストmetricをpopup側で扱う候補にはなるが、通常metricへ無条件に導入できる方式ではない。private API・metric key・OS / hardware世代差による保守リスクがあり、public API方式を優先し、実装前の方式比較で採用可否を決める。

根拠: macmon `src_lib/sources.rs`のIOReport bindingsとRAM / swap取得実装、`src_lib/lib.rs`のsampler lifecycle。

## 7. Sampling / Overhead

- Thermal Stateはsnapshotまたはnotificationを利用できる可能性があり、3秒ごとの重いcommand pollingを避けられる可能性がある。
- CPU utilizationは前回tickを保持してinterval差分を計算する必要がある。現在のprocess-local stateに収められるが、popup/watchは別々にsampleする。
- GPU utilization、温度、powerで`powermetrics`を利用する場合、sample intervalを持つcollectorとなる可能性がある。共有実機のコマンドは1秒sampleを取得したが、既存の3秒loopとの二重samplingとprocess spawn costは未計測である。
- `powermetrics`はCPU/GPU/ANE/power/thermalの複数値を一度に得られる可能性があるが、必要なsamplerに絞る方式と出力parseの保守性を比較する必要がある。

benchmarkは実施していない。

## 8. Reliability, Permissions, and Dependencies

| Metric | Candidate source | Public / private | 権限・依存 | Reliability / gap |
| --- | --- | --- | --- | --- |
| Thermal State | `ProcessInfo.thermalState` | Public | Foundation FFIまたはbridge候補 | OS-defined state。Rust integration未検証。 |
| CPU utilization | `host_processor_info` | Public kernel API候補 | Mach FFI候補 | tick差分とCPU topologyの扱い未検証。 |
| GPU utilization | `powermetrics` GPU HW active residency | command interface | 共有実機ではsudoで成功 | active residencyの表示妥当性、cost、負荷時変化は未検証。 |
| Temperature | `powermetrics`等 | 未確定 | command / privilege / sensor availability確認要 | version・sensor依存。 |
| CPU/GPU power | `powermetrics` cpu_power / gpu_power | command interface | 共有実機ではsudoで成功 | estimated values。accuracyとcost未検証。 |

`powermetrics -h`はCPU usage statistics、CPU/GPU/ANE power、thermal sampler等を提供すると表示したが、これはcommandの機能一覧確認であり、実負荷下でのmetric妥当性を確認したものではない。

## 9. Tests and Evidence Gaps

既存testsにはCompute / Thermal metricの取得、macOS API、powermetrics parse、CPU tick差分、temperature/powerの検証はない。

追加で必要な実機確認:

- `ProcessInfo.thermalState`をRustまたは小さなObjective-C/Swift probeから読む。
- idleとCPU workloadでCPU utilizationの候補値を比較する。
- `powermetrics`の最小samplerをroot / 通常権限それぞれで確認する。
- GPU workload時にGPU active residencyの出力が得られ、どの単位・意味で変化するか確認する。
- sampling intervalごとのprocess spawn、CPU、memory、power overheadを測る。

共有された実機で確認済み:

- `sudo powermetrics -n 1 -i 1000 -s cpu_power,gpu_power,thermal`が成功。
- CPU active residency、GPU HW active residency、CPU Power `370 mW`、GPU Power `42 mW`、Combined Power `411 mW`を取得。
- Thermal pressureは`Nominal`。
- これは1秒・1サンプルのidle寄りの観測であり、LLM workload時の挙動やoverheadの計測ではない。

## 10. Research Conclusions

| Metric | Recommendation | 根拠 |
| --- | --- | --- |
| CPU utilization | 条件付き採用候補 | 判断価値が高く、Mach API候補があるが、tick差分・FFI・cost未検証。 |
| GPU utilization | 条件付き採用候補 | 実機でGPU active residency取得を確認。ただし表示妥当性、負荷時変化、権限、cost未検証。 |
| Thermal State | 採用候補 | Apple-defined public stateで、Requirementsの熱制約判断に直接関係する。 |
| CPU temperature | 要追加調査 | public取得方式とThermal Stateとの差分・cost未確定。 |
| GPU temperature | 要追加調査 | public取得方式と値の意味・cost未確定。 |
| CPU power | 条件付き採用候補 | 実機で370 mW取得を確認。ただしestimated value、権限、cost未検証。 |
| GPU power | 条件付き採用候補 | 実機で42 mW取得を確認。ただしestimated value、権限、cost未検証。 |

## 11. Open Questions

- RustからFoundation `ProcessInfo.thermalState`へ接続する最小方式は何か。
- `host_processor_info`のApple Silicon CPU topologyとtick差分を、3秒samplingでどう扱うか。
- `powermetrics`のGPU active residencyはsystem-wide GPU utilizationとして解釈可能か。
- `powermetrics`にroot権限が必要なsamplerと不要なsamplerの範囲はどこか。
- Thermal StateだけでLLM workloadの熱制約判断に十分か。温度・powerを追加する価値はあるか。
- popup/watchの両processで同じexpensive samplerを実行した場合のoverheadはいくらか。
