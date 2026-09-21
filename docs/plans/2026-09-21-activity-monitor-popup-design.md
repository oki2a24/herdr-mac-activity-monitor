# Activity Monitor Popup Design

作成日: 2026-09-21
状態: 初期版metric・取得方式・表示契約確定。実装計画レビュー待ち

## 1. Design Scope

Technical Researchで「採用候補」または「条件付き採用候補」となったmetricを初期Designの対象とする。「要追加調査」のmetricは、方式比較の結果として必要になった場合にのみ再検討する。

初期Design対象:

- Memory used
- Memory Pressure
- Swap使用量
- Top memory processes
- CPU utilization
- Thermal State
- Battery percentage
- Charging / discharging state
- Estimated Battery runtime

初期Design対象外:

- Memory available
- CPU temperature / GPU temperature
- Battery drain
- Average Battery drain

対象外metricを実装上の理由で無理に代替値へ置き換えない。

## 2. User-visible Behavior

### Window title

既存window titleの表示内容と更新動作を完全に維持する。

現在の形式:

```text
🧠 USED/TOTALGB PERCENT% | 🔋 PERCENT% STATE
```

新metricをwindow titleへ追加しない。取得元を変更する場合でも、既存のMemory / Battery表示、欠損時の表示、3秒周期の更新、Herdr socket連携を壊さない。

### Popup

popupは詳細な診断情報を表示する場所とする。数秒で理解できる量に抑え、Requirementsの次の判断に直接関係するmetricだけを表示する。意味領域の境界には空行を入れる。

表示順は次の領域順に固定する。

1. Memory: Memory used、Memory Pressure、Swap
2. Memory consumers: Top memory processes（上位3件）
3. Compute / Thermal: CPU utilization、Thermal State
4. Power: Battery状態、OS推定runtime

Memory consumersはMemory領域の直後に置き、Memory状態と原因調査の手掛かりを連続して確認できるようにする。

総合health判定、`GOOD / BAD`、LLM backend情報、model情報、context、token throughputは追加しない。

## 3. Acquisition Architecture

### 主案A: 通常metric独立取得 + 高コストmetricはpopup専用

既存のprocess構成を基本維持する。

- watch process: 既存window titleに必要なMemory / Batteryだけを取得する。新しい詳細metricや`powermetrics`を取得しない。
- popup process: 既存Memory / Batteryに加えて、popupに必要な詳細metricを取得する。
- `powermetrics`等の高コストmetricはpopupが開いている間だけ取得する。
- IOPowerSources / ProcessInfo.thermalState等の軽量なpublic APIは、方式比較でcostとFFIが許容できる場合に通常metric候補として扱う。

この主案は、window titleを常時維持しながら、popupを閉じているときのmonitoring overheadを増やさないことを優先する。通常metricまで共有collectorへ集約しないため、現行のprocess lifecycleとmodule boundaryへの変更を抑える。

### 比較案B: 高コストmetric専用collectorを共有

popup起動時に高コストmetric専用collectorを起動し、必要なconsumerへIPC等で結果を共有する。`powermetrics`の重複実行を避けられる可能性がある一方、collectorのlifecycle、socket、stale data、権限、終了処理が追加される。

window titleは完全維持のため、watchが高コストmetricを要求しない場合、この案の共有範囲は限定的である。主案より複雑性が高いため比較対象とする。

### 比較案C: 全metric共有collector

Memory、Battery、履歴、Thermal、Compute、Powerを単一collectorへ集約する。samplingと履歴を一元化できるが、現在のpopup/watch独立process構成を大きく変更し、IPC・再起動・stale data・failure isolationを導入する。現時点では採用しない比較対象とする。

## 4. Cost and Privilege Policy

方式比較では、次の順序を採用した。

1. 通常権限で動作する方式
2. 初回のみ明示的なユーザー設定が必要で、その後の監視中に再承認が不要な方式
3. 監視中にsudoまたは再承認が必要な方式

`powermetrics`はResearch実機でsudo実行に成功したが、初期版では採用しない。runtime sudoとsampling costを導入しない方針により、CPU / GPU powerとGPU utilizationを対象外とした。

macmonはsudo不要でCPU/GPU/ANE power、residency、RAM、swap等を取得する比較候補として調査した。ただし実装は`IOReport.framework`、IOKit、CoreFoundation等のprivate・低レベルAPIに依存する。CPU / GPU powerはRequirements上もThermal Stateやutilizationへの補助情報であるため、初期版ではprivate APIを採用せず、表示対象から外す。

GPU utilizationは初期版の表示対象から外す。実機で確認できた`powermetrics`方式はsudoを必要とし、macmon / IOReport方式はprivate APIに依存するため、通常権限・public APIまたは保守可能な方式という初期版条件を満たさない。将来、条件を満たす方式が確認できた場合は再検討する。

Thermal Stateは`ProcessInfo.thermalState`のsnapshotを初期版で採用する。`Nominal`、`Fair`、`Serious`、`Critical`をApple-defined stateとして表示し、取得不能時は`unavailable`とする。notificationは必須にせず、既存の3秒更新loopへsnapshot取得として統合する。`powermetrics`や温度センサーは使用せず、window titleへは追加しない。

CPU utilizationは初期版で採用する。Mach public interfaceの累積CPU tickを2時点で取得し、差分からsystem-wide utilizationを算出する。前回tickはpopup process内に保持し、初回sample、差分不正、sleep / wake後は`unavailable`とする。`powermetrics`やmacmonは使用せず、window titleとwatch processには追加しない。

Memory usedはwindow titleとpopupで同じ`vm_stat`由来の計算式・単位・欠損意味を使う。ただし、両processが独立取得するため、取得時刻の差による数値差は許容する。Memory PressureはMemory usedの補正値や代替値にせず、popup process内の`DispatchSource` event stateとして分離する。event受信前は`unavailable`とし、`normal / warn / critical`を推測表示しない。

Swap usageは`sysctl vm.swapusage`のcurrent snapshotを初期版で採用する。`used`と`total`をpopupに表示し、`used = 0`も有効値として扱う。`vm_stat`のSwapins / Swapoutsは累積counterであり、current usageとして表示しない。取得失敗時は`unavailable`とし、watch / window titleでは取得しない。履歴や差分は初期版では持たない。

Top memory processesは`footprint`の`phys_footprint`を初期版で採用する。RSSではなくphysical footprintを表示し、表示値の意味をラベルと文書で明示する。全PIDを候補にしてpopup表示中にsnapshot取得し、通常権限で取得できないPIDはprocess単位で除外する。取得成功分からphysical footprint上位3件を選び、表示上は「取得可能なprocess内のTop 3」と明示する。watch / window titleでは取得せず、process groupingや履歴は初期版では行わない。

Battery percentageとCharging / discharging stateは、`IOPowerSources`を使用する。既存の`Battery`型、`ChargingState`、popup formatter、window title formatterの表示契約は変更しない。IOPowerSourcesのraw snapshot取得、state mapping、既存`Battery`型への変換を分離し、OS API failureは取得層で`Battery::Absent`へ変換する。`pmset` fallbackは採用しない。`pmset`専用parser / fixture testはIOPowerSources raw snapshot変換テストへ置き換え、既存のformatterとwindow title文字列・欠損時`n/a`テストは維持する。Estimated runtimeは`unknown` / `unlimited`を含む別metricとして独立に判断する。

Estimated runtimeは初期版の条件付き候補として採用する。`IOPSGetTimeRemainingEstimate`の秒数は実機で`pmset -g batt`の表示と一致し、時間経過で変化した。数値が返る場合だけruntimeを表示し、`unknown` / `unlimited`はその状態を明示する。runtimeを独自計算せず、取得不能時にzeroや推定値で代替しない。

高コストmetricはwatchでは取得せずpopup中心を第一候補とする。popupを閉じてもバックグラウンドで取得して共有する方式は、Design上の比較対象だが低優先とする。

権限が必要な方式しか成立しない場合は、まず初期版の表示対象から外す。採用済みmetricで一時的に取得できない場合はmetric単位で`unavailable`を表示し、zeroや推定値で代替しない。

## 5. Data Flow and State

通常metricは既存の取得・parse・format経路を尊重する。

```text
collector / command / API
  -> metric-specific data representation
  -> popup state
  -> popup formatter
  -> crossterm rendering
```

window titleは既存の経路を維持する。

```text
Memory / Battery collector
  -> existing title formatter
  -> existing Herdr JSON-RPC transport
```

高コストmetricのstateはpopup process内に限定する。watchへ渡さず、window titleのdata flowへ混ぜない。

履歴が必要なCPU utilizationはpopup process内のtwo-point historyとして扱う。Battery drain等の未採用metricには履歴を追加しない。sample timestamp、欠損、process restart、sleep / wakeを明示的に扱う。

## 6. Implementation-time Verification Details

方式と表示契約は確定している。実装時には以下の詳細を検証する。

- Memory Pressure: event登録、初期`unavailable`、event受信後のstate保持、process終了時cleanup。
- Top process: 全PID候補、取得不能PIDの除外、`phys_footprint` parse、process churn。
- CPU utilization: Mach tick差分、前回sample保持、初回・sleep / wake後の`unavailable`。
- Thermal State: Foundation APIのFFIとsnapshot mapping。
- Battery: IOPowerSources raw snapshotから`Battery` / `ChargingState`へのstate mappingとAPI failure。
- Estimated runtime: `unknown` / `unlimited`、`time_to_empty`との関係。

## 7. Error and Unavailable Handling

metric単位で取得状態を扱う。少なくとも以下を区別できるようにする。

- present value
- unavailable
- unknown
- unlimited
- stale（履歴metricのみ）

取得失敗が1つのmetricで起きても、window titleの既存Memory / Battery更新とpopup全体を停止させない。既存のMemory `Option`とBattery `Absent`の挙動を壊さず、追加metricの失敗を局所化する。

## 8. Verification Direction

production implementation前に、少なくとも次を検証対象とする。

- window titleの文字列と更新周期が変わらないこと
- popupが既存Memory / Batteryを表示し続けること
- popupを閉じた状態で`powermetrics`が起動しないこと
- 高コストmetricの取得失敗・権限不足でwatchが停止しないこと
- unknown / unlimited / unavailableをzeroや誤った正常値として表示しないこと
- metric取得方式ごとのsubprocess数、sampling interval、失敗時のtimeout / cleanup
- existing parser / formatter / Herdr transport testsの維持

具体的なテストケースと実装手順はImplementation Planに定義する。

## 9. Explicit Non-goals

- window titleへの新metric追加
- 全metric共有collectorの導入
- CPU / GPU powerの表示
- private APIを前提にした実装
- Intel Mac対応
- LLM backend統合
- 総合health判定
- Researchで未確定のmetricを推測値で埋めること
