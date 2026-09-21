# Memory Research

調査日: 2026-09-21
対象: RequirementsのMemory候補（Memory used / available / pressure / swap / top memory processes）

## Scope

現在のrepositoryのmemory取得経路を起点に、Apple Silicon macOSで候補metricを取得する方式、値の意味、public/private API、権限、subprocess、依存、sampling / overhead、既存architectureとの関係を調査した。

今回はCompute / Thermal、Power、`macmon`の方式比較、popupの最終UI、production実装は対象外とした。

## Executive Summary

- 現在のMemory usedは`sysctl` 2回と`vm_stat` 1回をsubprocessで取得し、`src/parsers.rs::compute_memory`で算出している。既存経路は変更していない。
- Appleの`vm_statistics64_data_t`にはfree / inactive / speculative / purgeable、compressor、pageins / pageouts、swapins / swapouts等のカウンタがある。`vm_stat`も同系統のカウンタを表示する。
- Memory pressureはAppleのpublicな`DispatchSource` memory-pressure eventとして取得可能だが、これは状態snapshotを3秒ごとに読むAPIではなく、normal / warn / criticalの変化を通知する仕組みである。現行Rust binaryへ導入する場合のFFI・状態保持・初期値取得は未検証。
- `vm_stat`のSwapins / Swapoutsは累積カウンタであり、現在のswap使用量（bytes）そのものではない。ただし共有された実機では`sysctl vm.swapusage`でcurrent usageを取得できた。
- top memory processesはmacOS標準`ps`のRSSで取得できることを共有された実機で確認した。ただし、RSSはphysical footprintと同じ意味ではない。
- 現時点のRecommendationはmetric単位で「採用候補」「条件付き採用候補」を示すが、最終採否ではない。特にpressure、swap current usage、process metricは追加検証が必要である。

## 1. Existing Repository Path

既存実装は`src/memory.rs:27-34`の`get`で次を順に行う。

1. `sysctl -n hw.pagesize`
2. `sysctl -n hw.memsize`
3. `vm_stat`
4. `src/parsers.rs::parse_vm_stat`
5. `src/parsers.rs::compute_memory`

`VmStat`はfree / inactive / speculative / purgeableの4つのページ数だけを保持する（`src/parsers.rs:28-35`）。`compute_memory`は`total_pages - (free + inactive + speculative + purgeable)`をused pagesとして、10進GBと整数percentへ変換する（99-123）。

この結果はpopupの`format_lines`とwindow titleの`format_window_title`へ渡される。詳細なruntime / process分離は[Repository Survey](repository-survey.md)に記録されている。

## 2. Memory used

### Known

現在のrepositoryではused memoryをページカウンタから算出している。`vm_stat`の実機出力には、少なくとも`Pages free`、`Pages active`、`Pages inactive`、`Pages speculative`、`Pages wired down`、`Pages purgeable`、compressor関連のカウンタが含まれる。macOSの`vm_stat(1)`はこれらをsystem-wide VM statisticsとして説明している。

Appleの`vm_statistics64_data_t`にも`free_count`、`inactive_count`、`speculative_count`、`purgeable_count`、`compressor_page_count`、`total_uncompressed_pages_in_compressor`などが定義されている。[Apple Developer Documentation](https://developer.apple.com/documentation/kernel/vm_statistics64_data_t)

### 候補方式

| 方式 | 性質 | このrepositoryとの関係 |
| --- | --- | --- |
| 現行の`vm_stat` subprocess | macOS標準commandのtext outputをparse | 既存方式。追加metricを同じcommand outputから読む余地があるが、format依存が増える。 |
| Mach host statisticsをRust FFIから読む | kernel interfaceの構造化データを直接取得 | subprocessを減らせる可能性があるが、FFI・型・OS version差の検証が必要。 |
| `sysctl`等の別system counter | counterごとにkeyを読む | keyの存在・意味・permissionをmetricごとに検証する必要がある。 |

### Assessment

既存Memory usedはすでにpopupとtitleで利用されており、Requirementsの「Memory used」候補に対応する実装上の基盤はある。ただし、Requirementsが最終表示で必要とする「Unified Memoryの負荷」をこの計算だけで十分に表せるかは、Memory PressureやSwapとの組み合わせを含めて後続Designで判断する事項である。

**Recommendation: 採用候補（既存値の維持）**。根拠は既存動作、追加runtime dependencyなし、既存formatterとの結合が確認できるため。ただし、Activity Monitorとの意味の完全一致や、compressor / wiredの扱いを新metric追加時に再確認する。

## 3. Memory available

### Known

`vm_stat`はfree、inactive、speculative、purgeable等を個別に示す。一方、これらを単純加算した値をmacOSの「available memory」と同一視できるかは、このrepositoryと今回確認した一次資料だけでは確定できない。Memory Pressureの判定は複数のVM状態を含むOS内部評価であり、free pagesだけでは表現できない可能性がある。

### 候補方式

- `vm_stat`の複数カウンタからavailable相当値を算出する。
- Mach VM statisticsを直接取得して、同じカウンタを構造化データとして扱う。
- macOS標準の別command / APIが提供するavailable値を利用する。

### Unknown / Why unresolved

この調査では、macOSの「available」の定義と、Activity Monitorが表示する値との対応を一次資料・実機比較で確定していない。単純な式を採用するとMemory Pressureと重複または矛盾する可能性がある。

**Recommendation: 要追加調査**。Knownは`vm_stat`で構成要素を取得できること、Unknownは表示値の定義と妥当な算式である。次にAppleのMemory Pressure定義または実機のActivity Monitor / command出力との比較が必要である。

## 4. Memory Pressure

### Known

Appleは`DispatchSource.makeMemoryPressureSource(eventMask:queue:)`を、memory pressure conditionの変化を監視するdispatch sourceとして公開している。[DispatchSourceMemoryPressure](https://developer.apple.com/documentation/dispatch/dispatchsourcememorypressure)

イベントフラグにはnormal、warn、criticalがあり、Appleのdocumentationではcriticalを「system memory pressure condition is at the critical stage」と説明している。[DISPATCH_MEMORYPRESSURE_CRITICAL](https://developer.apple.com/documentation/dispatch/dispatch_memorypressure_critical)

これはpollingで現在値を返す単純なsnapshot APIではなく、イベントを受けて最新状態を保持する形のAPIである。Appleのdispatch source資料も、event handlerがqueueへ非同期に投入され、イベントがcoalesceされることを説明している。[Dispatch Sources](https://developer.apple.com/library/archive/documentation/General/Conceptual/ConcurrencyProgrammingGuide/GCDWorkQueues/GCDWorkQueues.html)

### 候補方式

1. AppleのDispatch Source memory-pressure eventをFFI経由で登録し、最新状態をprocess-local stateへ保持する。
2. macOS標準commandの出力がsnapshotとして利用可能かを追加調査する。
3. `vm_stat`等のcounterから独自にpressureを推定する。ただしRequirementsはmacOS自身が定義する状態を表示してよいとしており、独自推定はAppleのpressure stateと同一ではない。

### Constraints

- 現行binaryは3秒pollの同期loopで、dispatch sourceを保持するstateやbackground event handlerはない。
- popupとwatchが別processなので、pressure stateを各processで別々に保持することになる。
- event APIを導入する場合、初期状態、イベント欠落・coalesce、process終了時のcleanup、Rust FFI境界が未検証。

**Recommendation: 条件付き採用候補**。Apple-defined stateであり判断価値が高い一方、現行architectureにsnapshotとしてそのまま差し込めない。public API利用の実現性、初期値取得、FFI負荷、popup/watchのprocess-local stateを小規模experimentで確認することが条件である。

## 5. Swap

### Known

Appleの`vm_statistics64_data_t`には`swapins`と`swapouts`が含まれる。[Apple Developer Documentation](https://developer.apple.com/documentation/kernel/vm_statistics64_data_t)

`vm_stat(1)`はSwapinsを「compressed pages swapped back in from disk」、Swapoutsを「compressed pages swapped out to disk」と説明する。これらは累積イベントカウンタであり、現在ディスク上で使用しているswap bytesを直接表す値ではない。

共有された実機では`vm_stat`に`Swapins: 0`、`Swapouts: 0`が表示され、`sysctl vm.swapusage`も成功した。結果は`total = 0.00M`、`used = 0.00M`、`free = 0.00M (encrypted)`だった。swap発生時の変化とOS version差は未確認である。

### 候補方式

- `vm_stat`のSwapins / Swapoutsを累積カウンタとして表示または差分化する。
- macOSのswap usageを返すsystem interface / commandを追加調査する。
- 2時点のcounter差分をsamplingして、直近のswap activityを表示する。

### Assessment

Requirementsの「Swap使用量」は、workloadが物理memoryに収まらず退避を伴うかを判断する目的である。その目的には累積値より、現在のswap usageまたは直近intervalのswap activityの意味を明確にした値が必要である。`vm_stat`のcounterを「使用量」と表示するのは意味が異なる。

**Recommendation: 条件付き採用候補**。標準`sysctl vm.swapusage`でcurrent usageを取得できる実機証拠が得られた。ただし、swap発生時の更新、OS version差、取得costは追加確認が必要である。

## 6. Top memory processes

### Known

macOSの`ps`には`rss`キーワードがあり、man pageはRSSをprocessのreal memory（resident set）sizeとして説明している。共有された実機では`ps -axo pid=,rss=,comm=`が成功し、Chrome、Codex、Siri AI等をRSS順に一覧できた。少なくともこの環境では通常権限でtop process取得が可能である。

Appleのprocess resource usage構造体には`ri_phys_footprint`と`ri_resident_size`がある。[rusage_info_v3](https://developer.apple.com/documentation/kernel/rusage_info_v3) ただし、今回の調査では全processのphysical footprintを一覧するpublicな単純snapshot APIや、Rustからの安定した取得方式までは確認していない。

### 候補方式

| 方式 | 値の意味 | 主な注意点 |
| --- | --- | --- |
| `ps`のRSS | resident set size | command subprocess、text parsing、RSSとphysical footprintの意味差、process visibility / permission。 |
| processごとのresource usage API | resident size / physical footprint候補 | PIDごとの呼び出し、API availability、FFI、permission、process churn。 |
| private / undocumented interface | Activity Monitorに近い値の可能性 | version risk、保守性、fallback。今回未調査。 |

Requirementsは「何がmemoryを使用しているのか」を調べる手掛かりを求めるが、RSSとphysical footprintのどちらを採用するかはTechnical Researchで調査すると明記している。したがって現時点でtop processを特定の値に固定できない。

**Recommendation: 条件付き採用候補（方式比較済み）**。`footprint`の`phys_footprint`を表示値の第一候補とする。共有実機ではsudoなしで3 processを約0.194秒で取得でき、各processの`Footprint`と`phys_footprint`が一致した。全PIDを候補にし、通常権限で取得できないPIDはprocess単位で除外し、取得成功分のTop 3を表示する。popup-only snapshotに限定し、RSSは表示値に使わず、process groupingと履歴は初期版では行わない。

## 7. Sampling / Overhead

現行のMemory取得は1回の`get`で`sysctl` 2回と`vm_stat` 1回のsubprocessを生成する。popupとwatchは同じcollectorをprocessごとに呼ぶため、両方が動作すれば同じsamplingが重複する（[Repository Survey](repository-survey.md)）。

metric別の観察:

- Memory used / available候補: `vm_stat`一回の出力を複数metricへ再利用できる可能性がある。ただしavailableの定義は未確定。
- Pressure: event driven方式なら3秒ごとのpressure subprocess pollingを避けられる可能性があるが、現行loopへの統合costは未計測。
- Swap: cumulative counterの差分には過去sampleを保持するstateが必要。current usage方式ならsampleごとの取得costを別途確認する。
- Top processes: process一覧取得とsortingが必要。対象process数、RSS/footprintの取得回数、subprocess spawnのcostは未計測。

CPU / memory / power overheadのbenchmarkは実施していない。Requirementsのmonitoring overhead判断に必要な場合は、候補方式を絞った後に最小限の実機計測を行う。

## 8. Reliability, Permissions, and Dependencies

| Metric | Public / private | 追加権限 | subprocess | Reliability observation |
| --- | --- | --- | --- | --- |
| Existing used | 標準command。underlying API種別は今回未確定 | 通常利用の範囲で既存実装は動作 | `sysctl` 2回 + `vm_stat` | 実機で`vm_stat`出力を確認。format変化へのtext parse依存あり。 |
| Pressure | Apple public Dispatch Source | 今回未確認 | 必須ではない | event APIだが、初期状態・FFI・process-local保持は未検証。 |
| Swap activity | `vm_stat`標準出力は利用可能 | 今回未確認 | `vm_stat` | 累積counterでありcurrent usageではない。 |
| Swap current usage | `sysctl vm.swapusage`を実機で確認 | 共有実機では通常権限で成功 | `sysctl` | idle時に0.00Mを確認。swap発生時・OS差は未確認。 |
| Top process RSS | macOS標準`ps` | 共有実機では通常権限で成功 | `ps` | top 20取得を確認。RSSの意味とprocess churnに注意。 |
| Top process footprint | API候補あり | 今回未確認 | 必須とは限らない | publicな一覧取得方式とRust FFIを未確認。 |

## 9. Tests and Evidence Gaps

既存テストは`parse_vm_stat`と`compute_memory`の純粋処理を対象にしているが、`memory::get`のsubprocess output、macOS version差、pressure API、swap current usage、process enumerationはテストしていない。

今回の実機確認は次の範囲である。

- Apple Silicon arm64 macOS上で`vm_stat` commandの実行と出力項目を確認。
- 共有実機で`sysctl vm.swapusage`が成功し、total / used / freeがすべて`0.00M`、encryptedであることを確認。
- 共有実機で`ps -axo pid=,rss=,comm=`が成功し、top 20 processをRSS順に取得できることを確認。

swapが0の状態しか確認していないため、swap発生時の値の変化と、top processのRSSがphysical footprintの代替として妥当かは未確定である。

## 10. Research Conclusions

| Metric | Recommendation | 根拠 |
| --- | --- | --- |
| Memory used | 採用候補 | 既存動作があり、`vm_stat` / parser / formatterの経路が確立している。 |
| Memory available | 要追加調査 | availableの定義と算出方法が未確定。 |
| Memory Pressure | 条件付き採用候補 | Apple public APIで意味が明確だが、event APIと現行poll/process構造の接続が未検証。 |
| Swap使用量 | 条件付き採用候補 | `sysctl vm.swapusage`のcurrent usage取得を実機確認。ただしswap発生時・OS差・costは未確認。 |
| Top memory processes | 条件付き採用候補 | 標準`ps`でtop 20取得を実機確認。RSS/footprint、grouping、costは未確定。 |

これらはResearch段階のRecommendationであり、最終UI・最終architecture・最終採否ではない。

## 11. Open Questions for Cross-cutting / Design

- Pressureをevent-driven stateとして各processに持たせるか、別のsnapshot方式を使えるか。
- popupとwatchの両方でmemory metricを取得する際、`vm_stat`一回の結果をどこまで共有できるか。
- swap current usageとswap activityのどちらがRequirementsの判断に適切か。
- top processの値としてRSSとphysical footprintのどちらが、Apple SiliconのUnified Memory診断に適切か。
- 全process enumerationのpermissionと、rootなしで取得できる範囲を実機で確認できるか。
- 3秒周期のprocess scanとsortingがmonitoring overheadの許容範囲か。
