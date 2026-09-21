# Power Research

調査日: 2026-09-21
対象: Battery percentage、charging / discharging state、Battery drain、平均drain、推定runtime

## Scope

既存の`pmset -g batt`経路を起点に、Apple Silicon macOSでのpower metric取得方式、API、権限、依存、sampling、履歴、overhead、既存表示との関係を調査した。Compute / ThermalのCPU/GPU powerそのもの、最終UI、production実装は対象外とした。

## Executive Summary

- AppleのIOPowerSources APIは、power sourceのcurrent capacity、maximum capacity、charging state、power source type、time remaining等を公開している。[IOPowerSources.h](https://developer.apple.com/documentation/iokit/iopowersources_h)
- `IOPSGetTimeRemainingEstimate`はOSが推定した残り秒数を返し、unknown / unlimitedを表す特別値もある。[IOPSGetTimeRemainingEstimate](https://developer.apple.com/documentation/iokit/1523835-iopsgettimeremainingestimate)
- 現在のrepositoryは`pmset -g batt`をsubprocessで実行し、percentageとcharging stateだけをparseしている。IOKit APIへの直接依存はない。
- Battery drainはcapacityの2時点差分と時間差から求める履歴metricであり、単一`pmset`出力から直接得られない。瞬間的なdrain、平均drain、estimated runtimeは別の意味を持つ。
- estimated runtimeはOSが既に提供する候補があるが、AC接続時はunlimited、計算中はunknownになり得る。表示できない状態を前提にする必要がある。
- **Recommendation:** percentageとcharging stateは既存維持の採用候補、OS estimated runtimeは条件付き採用候補、Battery drainと移動平均は要追加調査。常時履歴を持つことのoverheadとprocess間共有は未検証。

## 1. Existing Repository Path

`src/battery.rs:10-20`は`pmset -g batt`を1回実行し、`%`と`present`を含む行を選ぶ。`src/parsers.rs::parse_battery_line`はpercentageを`u8`へ変換し、`discharging` / `charging` / その他を`ChargingState`へ分類する。

データ型は`Battery::Present { percent, state }`または`Battery::Absent`。popupでは`format_lines`、window titleでは`format_window_title`が同じ値を別形式にする。取得失敗時は`Battery::Absent`で表示を継続し、履歴は保持しない（[Repository Survey](repository-survey.md)）。

## 2. Battery percentage / charging state

### Known

AppleのIOKit power source keysには`kIOPSCurrentCapacityKey`、`kIOPSMaxCapacityKey`、`kIOPSIsChargingKey`、`kIOPSIsChargedKey`、`kIOPSPowerSourceStateKey`等がある。[IOPS keys](https://developer.apple.com/documentation/iokit/iopskeys_h/defines)

`kIOPSPowerSourceStateKey`はAC、battery、offlineの状態を表す。[kIOPSPowerSourceStateKey](https://developer.apple.com/documentation/iokit/kiopspowersourcestatekey)

現在の`pmset`方式は既にpercentageとcharging stateを取得できるが、text output formatに依存する。IOKit APIはCFDictionary / CFTypeベースのstructured interfaceであり、Rustから使う場合はCF/IOKit FFIまたはbridgeが必要になる。

**Recommendation: 採用候補（IOPowerSourcesへ切替）**。Requirementsが既存表示の維持を明記しているため、`Battery`型・formatter・window title文字列は維持する。IOPowerSourcesのraw snapshotとstate mappingを分離し、API failureは`Battery::Absent`へ変換する。`pmset` fallbackは採用せず、pmset専用parser / fixture testはIOPowerSources変換テストへ置換する。

## 3. Battery drain

### Meaning

Battery drainは、少なくとも次の2つを区別する必要がある。

- capacity percentageの時間差分: 表示しやすいが、percentageの丸め・OS更新間隔・capacity estimationの影響を受ける。
- electrical current / powerの瞬間値: IOPowerSources keysにはcurrent、voltage等が定義されているが、値のavailabilityとMacBook実機での意味を今回確認していない。

Battery percentageだけからdrainを出す場合、`(previous_percent - current_percent) / elapsed_time`を計算し、少なくとも前回sampleを保持する必要がある。これは現在のprocess-local・no-history architectureには存在しない。

### Recommendation

**Recommendation: 要追加調査**。Requirements上の判断価値は高いが、percentage差分の分解能、charging transition、sleep、AC接続、短いintervalでのnoise、popup/watch別processの履歴不一致を確認する必要がある。常時samplingしても、表示値が安定するとは限らない。

## 4. Average Battery drain

移動平均や長時間平均は、drainの短期noiseを減らせる可能性がある。一方で、window長、charging transition時の扱い、履歴の保存先、process再起動時の初期化が必要になる。

現在の3秒loopをそのまま履歴samplingへ使うと、popupとwatchがそれぞれ履歴を持ち、同じworkloadでも別結果になる。サンプルを長く保持するほどmemoryは小さいが増加し、欠損やsleep wakeの時間差を扱う処理も必要になる。

**Recommendation: 要追加調査**。短期差分より判断に役立つ可能性はあるが、window・state・overheadの設計判断を要するため、Power Research段階では採否を確定しない。

## 5. Estimated Battery runtime

Appleの`IOPSGetTimeRemainingEstimate`は、全power sourceが空になるまでの推定秒数を返す。limited powerで計算中は`kIOPSTimeRemainingUnknown`、AC等のunlimited powerでは`kIOPSTimeRemainingUnlimited`を返す。[IOPSGetTimeRemainingEstimate](https://developer.apple.com/documentation/iokit/1523835-iopsgettimeremainingestimate)

IOPowerSourcesはbattery / UPSのstateへuniformにアクセスし、power source変更notificationも提供する。[IOPowerSources.h](https://developer.apple.com/documentation/iokit/iopowersources_h)

**Recommendation: 条件付き採用候補**。OS推定値を独自計算せず利用できる可能性があり、Requirementsの「あとどの程度使用を継続できそうか」に直接関係する。ただしunknown / unlimitedを`n/a`等で扱う必要があり、IOKit FFI・MacBook実機での値の更新・AC / battery transitionを確認することが条件である。

### Research probe result

Research-only Objective-C probeで、Apple Silicon Mac上のIOPowerSources APIを実行できることを確認した。結果は次の通り。

- source: `InternalBattery-0`
- state: `Battery Power`
- current / max capacity: `78 / 100`
- charging: false
- estimated runtime: `unlimited`
- `time_to_empty`: `793` on the first run and `783` on a rerun

この結果は通常権限のprobeで取得できた。`time_to_empty`が短時間で793秒から783秒へ変動したため、estimated runtime関連fieldは動的推定値として扱う必要がある。`unlimited`と`time_to_empty`が同時に出力された理由を含め、API fieldsの意味・更新タイミング・OS version差については、表示値として採用する前にさらに確認が必要である。probe sourceは`research/experiments/power-thermal-probes/`に保存した。

追加の実機確認では、電源ケーブルを外した状態で`IOPSGetTimeRemainingEstimate`が`37560`秒（626分）を返し、`pmset -g batt`の`10:26 remaining`と一致した。その約2分後には`38760`秒（646分）となり、`time_to_empty=626`から`646`へ変化した。limited battery状態で数値が返ること、推定値が時間経過で更新されることを確認した。AC接続時などの`unknown` / `unlimited`は引き続き状態として扱う。

## 6. Sampling / Overhead

- 現行は各3秒loopで`pmset` subprocessを1回生成する。popupとwatchが同時に動けば重複する。
- IOPowerSourcesのsnapshot APIを直接呼ぶ方式は、subprocess spawnとtext parseを避けられる可能性があるが、CF/IOKit FFIのcostは未測定。
- power source change notificationを使える場合、battery stateの変化に対してevent-driven更新が可能だが、drain計算には一定間隔の履歴sampleがなお必要になる可能性がある。
- drainの履歴は、更新頻度よりも時間差・欠損・charging transitionの扱いが値の品質に大きく影響する。

benchmarkは実施していない。

## 7. Reliability, Permissions, and Dependencies

| Metric | Candidate source | Public / private | 権限・依存 | Reliability / gap |
| --- | --- | --- | --- | --- |
| Percentage | `pmset -g batt` / IOPowerSources | 標準command / Public API | subprocessまたはIOKit FFI | 現行動作あり。text parseまたはFFI依存。 |
| Charging state | `pmset -g batt` / IOPowerSources | 標準command / Public API | 同上 | AC / charging / chargedの意味を実機確認する必要。 |
| Instant drain | IOPowerSources current等 / percentage差分 | Public候補 | FFIまたは履歴 | current keyのavailability未確認。 |
| Average drain | percentage差分の履歴 | 実装metric | process state / history | window・sleep・transition処理未確定。 |
| Estimated runtime | `IOPSGetTimeRemainingEstimate` | Public API | IOKit FFI | unknown / unlimitedを返す。推定値の精度未確認。 |

## 8. Tests and Evidence Gaps

既存testsはbattery line parser、percentage、charging state、Absent fallback、formatterを対象とする。`pmset` subprocess I/O、IOPowerSources FFI、drain history、estimated runtimeはテストしていない。

追加で必要な実機確認:

- AC接続、充電中、満充電、battery dischargingでIOPowerSources fieldsを確認する。
- `IOPSGetTimeRemainingEstimate`のlimited / unlimited / unknown結果を確認する。
- batteryで一定時間動作させ、percentage差分drainの分解能とnoiseを測る。
- popup / watchを同時実行した場合の履歴差を確認する。
- IOKit FFIまたは標準command方式の取得costを比較する。

Research probeで確認済み:

- Objective-Cから`IOPSCopyPowerSourcesInfo`、`IOPSCopyPowerSourcesList`、`IOPSGetPowerSourceDescription`、`IOPSGetTimeRemainingEstimate`を呼び出せる。
- Foundation / IOKit framework linkのみでprobeをbuildでき、追加runtime applicationは不要だった。

## 9. Research Conclusions

| Metric | Recommendation | 根拠 |
| --- | --- | --- |
| Battery percentage | 採用候補（IOPowerSourcesへ切替） | public APIで取得し、既存`Battery`型・formatter・window titleを維持する。API failureは`Absent`、pmset fallbackなし。 |
| Charging / discharging state | 採用候補（IOPowerSourcesへ切替） | public keyで取得し、state mappingをunit testする。API failureは`Absent`、pmset fallbackなし。 |
| Battery drain | 要追加調査 | 履歴・差分・noise・transitionの扱いが未確定。 |
| Average Battery drain | 要追加調査 | 履歴windowとprocess lifecycleの設計判断が必要。 |
| Estimated Battery runtime | 条件付き採用候補（実機確認済み） | battery駆動時に`pmset`と一致する秒数を取得できた。推定値、unknown / unlimited、FFIを扱う必要。 |

## 10. Open Questions

- `IOPSCopyPowerSourcesInfo` / `IOPSGetPowerSourceDescription`からApple Silicon MacBookの必要fieldsをrootなしで取得できるか。
- `IOPSGetTimeRemainingEstimate`の値がbattery discharging中にどの頻度・条件で更新されるか。
- percentage差分から求めるdrainと、current / voltage fieldsのどちらがRequirementsの判断に適切か。
- 3秒samplingでdrainを計算する場合、丸め・sleep・charging transitionをどう扱うか。
- popup/watchのprocess-local historyを統合せずに表示することの一貫性をどう評価するか。
