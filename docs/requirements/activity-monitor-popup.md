# Activity Monitor Popup Requirements

## 背景

`herdr-mac-activity-monitor` は現在、メモリとバッテリーの情報を
herdr のウィンドウタイトルと、`prefix + m` で開くポップアップに表示している。

ウィンドウタイトルは、少量の情報を常時確認する用途に適している。
一方、ポップアップにはより多くの情報を表示できるスペースがある。

Apple Silicon Mac でローカルLLMを実行すると、
Unified Memory、GPU、熱、バッテリーなどに大きな負荷がかかることがある。

単純なメモリ使用量だけでは、
Macが現在どの程度厳しい状態にあるのかを十分に判断できない。

例えば、モデル自体は正常にロードできていても、
大量のSwapが発生している可能性がある。

その場合、ローカルLLMは技術的には動作しているものの、
現在のモデルや設定がそのMacに対して過剰な負荷をかけている可能性がある。

ポップアップでは、このような状態を判断するために必要な情報を提供する。

ただし、汎用的なシステムモニターを目指すものではない。


## 目的

ポップアップを見たときに、次の問いを短時間で判断できるようにする。

> 今このApple Silicon Macで、
> ローカルLLMを走らせ続けて大丈夫なのか？

ポップアップは、ユーザー自身がこの判断をするために必要な情報を提供する。

herdr自身がその判断を代行するものではない。


## 対象環境

Apple Silicon Macのみを対象とする。

Intel Macへの対応は要件としない。


## 対象外

この機能では、以下を目的としない。

- macOS Activity Monitorの代替
- 網羅的なシステム監視
- LLM自体の監視
- 実行中モデルの検出
- Context使用量やtoken throughputの監視
- Ollama、llama.cpp、MLX、UnslothなどのLLM backendとの統合
- `GOOD / BAD` や `LLM READY / NOT READY` といった総合判定
- Intel Macへの対応


## 設計原則

### 判断につながる指標を選ぶ

取得可能だからという理由だけで指標を追加しない。

表示する各指標は、ローカルLLMを実行しているときの
具体的な判断に役立つものでなければならない。


### 数秒で理解できる

ポップアップを見て、数秒以内に現在の状態を理解できることを目指す。

表示領域に余裕があるからといって、
取得可能なシステム情報をすべて表示することはしない。


### 状態と、その原因を判断する材料を提供する

可能な範囲で、次の両方を判断できるようにする。

1. Macが現在どのような状態にあるのか
2. 何がその状態に影響しているのか

例えばMemory Pressureは現在の状態を示し、
メモリ使用量の多いプロセスはその原因を調べる手掛かりになる。


### 独自の総合health判定を行わない

herdr独自の総合的なhealth classificationは導入しない。

一方で、Memory PressureやThermal Stateなど、
macOS自身が意味を定義している状態については表示してよい。


### Monitoring overheadを小さくする

監視機能そのものが、
無視できないCPU、メモリ、電力消費を発生させてはならない。

そのため、各指標の取得方法だけでなく、
取得頻度やsampling方法も設計対象とする。


### 既存動作を維持する

現在のウィンドウタイトルに表示している
メモリとバッテリー情報は引き続き動作させる。

ウィンドウタイトルは簡潔な常時表示、
ポップアップはより詳細な診断情報を確認する場所として扱う。


## 情報領域

### Memory

次の問いを判断するために必要な情報を提供する。

> 現在のworkloadはUnified Memoryに
> 過剰な負荷をかけていないか？

候補となる指標：

- Memory used
- Memory available
- Memory Pressure
- Swap使用量
- メモリ使用量の多いプロセス

Memory usedだけでは十分ではない。

Swapを見ることで、
workloadが物理メモリだけでは収まらず、
メモリ退避を伴いながら動作している状態を把握できる可能性がある。

Memory Pressureは、
macOS自身が現在のメモリ状況をどのように評価しているかを
判断する材料になる。

メモリ使用量の多いプロセスを表示することで、

> 何がメモリを使用しているのか？

という次の問いを調べる手掛かりを提供する。

プロセスごとのメモリ使用量として
RSS、physical footprintなどのどの値を採用するか、
また関連プロセスをまとめて扱うべきかについては、
Technical Researchで調査する。


### Compute / Thermal

次の問いを判断するために必要な情報を提供する。

> 現在のworkloadにおいて、
> 計算資源の使用状況や熱による制約が問題になっていないか？

候補となる指標：

- CPU utilization
- GPU utilization
- Thermal State
- CPU/GPU temperature
- CPU/GPU power

これらすべてを最終的なUIに表示することを
現時点では要求しない。

特に具体的な温度やCPU/GPU個別の消費電力については、
より上位のThermal StateやPower関連情報に対して
追加の判断材料を提供できる場合にのみ表示する。

各指標の取得可否、信頼性、取得コスト、
ローカルLLM実行時の診断価値をTechnical Researchで調査したうえで、
最終的な採否を決定する。


### Power

次の問いを判断するために必要な情報を提供する。

> MacBookをバッテリーで使用している場合、
> 現在のworkloadはどの程度の速度でバッテリーを消費しており、
> あとどの程度使用を継続できそうか？

候補となる指標：

- Battery percentage
- Charging / discharging state
- 現在のBattery drain
- 平均Battery drain
- 推定残り稼働時間

現在すでに存在するBattery percentageと
Charging / discharging stateは維持する。

平均Battery drainや推定残り稼働時間など、
過去のsamplingを必要とする指標については、
十分な精度と小さなmonitoring overheadで実現できる場合に採用する。

そのために常時samplingを行い履歴を保持するべきかどうかは、
Technical Researchで調査したうえで決定する。


## 指標候補の優先度

### 優先度：高

- Memory used
- Memory available
- Memory Pressure
- Swap
- Top memory processes
- CPU utilization
- GPU utilization
- Thermal State
- Battery percentage
- Charging / discharging state
- Battery drain


### Technical Researchの結果によって判断

- Battery drainの移動平均
- 推定Battery runtime
- CPU temperature
- GPU temperature
- CPU power
- GPU power


### 現時点では不要

- Disk capacity
- 一般的なDisk使用量
- Network statistics
- LLM model information
- Context usage
- Token throughput
- Backend固有の指標

Technical Researchによって、
これらの指標が中心となる問いに直接役立つことが判明した場合のみ、
再検討する。


## Technical Researchで調査する事項

以下については、現時点では実装方法を決定しない。

実装前にTechnical Researchを行う。

- 現在のherdr実装は既存の指標をどのように取得しているか
- `macmon` はApple Siliconの各指標をどのように取得しているか
- 利用候補となるmacOS APIのうち、public APIとprivate APIはどれか
- root権限を必要とする指標があるか
- subprocessの実行を必要とする取得方法があるか
- 各指標を取得するruntime costはどの程度か
- 適切なsampling intervalはどの程度か
- macOSにおける「Memory available」を何として定義するべきか
- プロセスごとのメモリ使用量として何を採用するべきか
- 関連するプロセスをまとめて表示するべきか
- Memory Pressureをどのように取得・表現するべきか
- Thermal Stateをどのように取得・表現するべきか
- CPU/GPU temperatureは十分に信頼でき、判断に役立つか
- Battery discharge powerをどのように取得できるか
- macOS自身が有用なBattery remaining runtimeを提供しているか
- remaining runtimeを独自推定する場合、どの程度のsampling/historyが必要か
- 常時バックグラウンドsamplingを行う価値があるか
- `macmon` が使用している手法のうち、herdrで利用・応用できるものは何か
- private macOS APIを利用する場合、どのような保守リスクがあるか


## Technical Researchにおける制約

`macmon` は重要な参考実装として扱う。

ただし、あらかじめ依存先や実装方式として採用することは決めない。

Technical Researchでは少なくとも以下を明らかにする。

1. `macmon` が各指標をどのように取得しているか
2. その取得方法がherdrに適しているか
3. `macmon` 自体への依存、基盤となるlibraryの再利用、
   必要な機能のみの独自実装のうち、どの方式が適切か

この調査を行う前に実装方式を決定しない。


## 成功条件

最終的なポップアップによって、
Apple Silicon MacでローカルLLMを実行しているユーザーが、
短時間で少なくとも以下を判断するための情報を得られること。

- メモリに余裕があるか、逼迫しているか
- Swapが大きく使用されていないか
- どのプロセスがメモリを多く使用しているか
- CPU/GPUがどの程度使用されているか
- 熱による制約が発生していないか
- バッテリー駆動時に、どの程度の速度で電力を消費しているか

同時に、

- 汎用的なシステムモニターにならないこと
- LLM backend固有の監視機能を持ち込まないこと
- 監視機能自体が無視できない負荷を発生させないこと
- 既存のウィンドウタイトル表示を壊さないこと

を満たす。
