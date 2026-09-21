# Activity Monitor Popup Technical Research Requirements

## 1. この文書の目的

この文書は、`herdr-mac-activity-monitor` のActivity Monitor Popup拡張に
向けたTechnical Researchの進め方、成果物、品質基準、完了条件を定義する。

機能そのものの要求は以下のRequirements documentで定義する。

- `docs/requirements/activity-monitor-popup.md`

Technical Researchの目的は、Requirementsで挙げられた候補metricについて
技術的な事実と選択肢を明らかにし、その後のDesignで十分な根拠をもって
採否や実装方式を判断できる状態にすることである。

Technical ResearchではDesignやproduction implementationには進まない。


## 2. Researchの基本方針

Technical Researchでは、各候補metricについて単に
「取得できる / 取得できない」を調べるだけでは不十分とする。

最低限、Requirementsに照らしてそのmetricを採用するかどうかを
判断できるだけの材料を揃える。

必要なmetricについては、具体的な実装方式の候補まで踏み込んで調査してよい。

ただし、Research段階ではarchitectureや最終UIを決定せず、
production codeの実装にも進まない。


## 3. Researchで明らかにすること

各候補metricについて、必要十分な範囲で以下を明らかにする。

- そのmetricは何を判断するために必要なのか
- 実際に何を測定・表現するmetricなのか
- Apple Silicon Macで取得可能か
- どのような取得方法が存在するか
- public API / private APIのどちらを利用するか
- root権限等の追加権限が必要か
- subprocessを必要とするか
- 外部dependencyを必要とするか
- 値の意味と信頼性
- 適切なsampling方法やsampling interval
- 取得によるCPU / Memory / Power等のoverhead
- herdrの既存architectureとの関係
- 既存のwindow title表示への影響
- 保守性やmacOS updateに対するリスク
- Requirementsに照らしたRecommendation

すべての項目について同じ深さまで調査する必要はない。

Recommendationを判断するために必要な深さまで調査する。


## 4. Recommendation

Technical Researchでは事実を整理するだけでなく、
Requirementsに照らしたRecommendationを提示する。

Recommendationには、原則として以下の分類を使用する。

- 採用候補
- 条件付き採用候補
- 見送り候補
- 要追加調査

Recommendationには必ず根拠を付ける。

例えば、

- ユーザーの判断にどの程度役立つか
- 取得方法の信頼性
- monitoring overhead
- private APIへの依存
- 権限
- runtime dependency
- 保守性

などを考慮する。

Researchで示すRecommendationは最終決定ではない。

Research成果をレビューした後、Designへ進む際に最終的な採否を判断する。


## 5. Researchの進行単位

Researchは技術要素ではなく、
Requirementsで定義した情報領域を中心に進める。

以下の順序を基本とする。

### Step 0: Repository Survey

現在のherdrのうち、今回のfeatureに関係するarchitectureを把握する。

### Step 1: Memory Research

Memory関連metricを調査する。

### Step 2: Compute / Thermal Research

ComputeおよびThermal関連metricを調査する。

### Step 3: Power Research

BatteryおよびPower関連metricを調査する。

### Step 4: Cross-cutting Review

各領域の調査結果を横断して評価する。

それぞれは別のCodexセッション等で実施してよい。

各Researchが独立して完了し、その成果が文書として残ることを重視する。


## 6. Repository Survey

最初に、後続ResearchのためのRepository Surveyを行う。

目的はrepository全体のarchitecture reviewではなく、
今回のfeatureに関係する部分の「地図」を作ることである。

少なくとも以下を確認する。

- entry point
- 現在のMemory取得
- 現在のBattery取得
- data collectionの流れ
- sampling / update lifecycle
- state management
- herdrとのintegration
- popup rendering
- window title rendering
- module boundaries
- 関連dependency
- 関連するtests

また、

- 既存window titleを壊さずpopupを拡張する場合に影響しそうな箇所
- monitoring overheadに関係しそうな現在の処理

についても把握する。

Repository Surveyでは、今回のfeatureと無関係なコード品質評価、
architecture改善、refactoring提案には範囲を広げない。


## 7. macmonの扱い

`macmon` はApple Silicon monitoringの重要な参考実装として扱う。

ただし、`macmon` の方式をあらかじめ正解とはみなさない。

各情報領域のResearchにおいて必要な範囲で、

- 対象metricをどのように取得しているか
- どのAPI / framework / libraryを利用しているか
- public / privateのどちらか
- root権限が必要か
- どの程度のsamplingを行っているか
- herdrで同じ方式を利用できるか

などをsource codeレベルで確認する。

`macmon` だけでは判断できない場合は、
Appleの情報や他のOSS実装も調査してよい。

ただし、macOS monitoring toolを網羅的に比較すること自体を
Researchの目的にはしない。


## 8. 情報源とEvidence

重要な技術判断では一次情報を優先する。

特に以下を優先的なEvidenceとして扱う。

- herdr自身のsource code
- Apple公式documentation
- macOS SDK / header
- 参照するOSSの実際のsource code
- Apple Silicon Macでの実機検証

GitHub Issue / Discussion、ブログ、その他の二次情報も
補助的な情報源として利用できる。

ただし、Design判断を左右する重要なRecommendationを
二次情報だけに基づいて確定しない。

すべての文章について複数sourceを要求するものではない。

重要なfindingについて、

> なぜその判断に至ったのか

を後から追跡できることを重視する。

必要に応じて以下のような形で記録する。

- Finding
- Evidence
- Confidence
- Implication
- Recommendation


## 9. private API

publicな取得方法を優先する。

ただし、private APIであることだけを理由に
取得方法やmetricを除外しない。

private APIによってRequirementsを満たすうえで十分な価値が得られる場合は、
採用候補として評価してよい。

private APIをRecommendationする場合は、少なくとも以下を調査する。

- publicな代替手段の有無
- private APIを利用することで得られる価値
- macOS versionへの依存
- macOS updateによって壊れる可能性
- failure時の影響
- fallbackの可能性
- private API依存を局所化できるか
- 保守コスト

価値とリスクの両方を明示したうえでRecommendationを行う。


## 10. runtime dependency

herdrを実行するために、
`macmon`等の第三者monitoring applicationのインストールを要求しない。

一方で、以下は候補として許容する。

- RustからmacOS API等を直接利用する
- 妥当なRust crate / libraryを利用する
- macOSに標準で存在するcommandを利用する

例えば標準commandをsubprocessとして実行する方式であっても、
それだけを理由に除外しない。

逆にRustのみで完結すること自体も目的にはしない。

取得コスト、信頼性、保守性、dependency、実装複雑性を比較して判断する。


## 11. 実機検証

Technical Researchはsource codeやdocumentationを使った
静的調査を基本とする。

ただし、Recommendationを出すために必要な場合は、
Apple Silicon Mac上で小規模な実機検証を行ってよい。

例えば、

- metricが実際に取得できるか
- idle時とlocal LLM workload時で値が妥当に変化するか
- samplingによるoverheadはどの程度か
- 複数の取得方式に実用上の差があるか

などを検証してよい。

すべての候補metricについて実機検証することは要求しない。

実験がRecommendationを判断するために必要な場合にのみ行う。


## 12. Monitoring Overheadの評価

Requirementsで定義した通り、
monitoring機能自体が無視できない負荷を発生させないことを重視する。

有力な取得方式についてoverheadが採否を左右する場合は、
「軽量と思われる」といった推測だけで判断せず、
可能な範囲で簡易計測を行う。

必要に応じて、

- CPU usage
- Memory usage
- process spawn
- sampling frequency
- その他明らかに重要なcost

を確認する。

可能であれば現在のherdrをbaselineとして、
metric collection追加時にどの程度costが増えるかを見る。

Research段階では、

> CPU overheadは必ずX%以下

といった固定の合格基準は設けない。

計測結果をDesign時の判断材料として残す。

厳密なbenchmark suiteの構築はResearchの目的としない。


## 13. Research用Experiment Code

Technical Research中にproduction codeは変更しない。

ただし、技術的な主張を検証し、
後から再現することに価値がある場合は、
Research専用の小さなexperiment codeをrepositoryに保存してよい。

例えば以下のような構成を利用できる。

    research/
      experiments/
        <experiment-name>/

ただし、このdirectory構成自体は現時点で固定しない。

一度きりの簡単な確認であれば、
experiment codeをrepositoryに保存する必要はない。

保存する場合はproduction codeと明確に分離し、
Research documentから少なくとも以下を追跡できるようにする。

- Purpose
- Environment
- Procedure / Command
- Result
- Conclusion

Research用experiment codeを、
Design前にproduction implementationへ昇格させない。


## 14. Research成果物

Researchの詳細結果は必要に応じて個別文書へ分割する。

想定する構成は以下。

    docs/research/
      activity-monitor-popup/
        repository-survey.md
        memory.md
        compute-thermal.md
        power.md
        cross-cutting.md

      activity-monitor-popup-research.md

個別文書には調査の詳細、Evidence、experiment、
metricごとの検討などを記録する。


## 15. Research Summary

`docs/research/activity-monitor-popup-research.md` は
Technical Research全体のliving documentとして扱う。

すべてのResearchが終了してから作るのではなく、
各調査が終了するたびに更新する。

例えば、

    Repository Survey
        ↓
    Research Summary更新
        ↓
    Memory Research
        ↓
    Research Summary更新
        ↓
    Compute / Thermal Research
        ↓
    Research Summary更新
        ↓
    Power Research
        ↓
    Research Summary更新
        ↓
    Cross-cutting Review
        ↓
    Research Summary最終整理

という流れを基本とする。

Research Summaryには詳細を重複してコピーしない。

主に以下を保持する。

- Research status
- 重要なfindings
- feasibility matrix
- 各metricのRecommendation
- 重要なEvidenceへの参照
- unresolved questions
- Designへ引き渡す事項

後続Researchによって以前のRecommendationを変更してよい。

変更する場合は、その理由を追跡できるようにする。

Research Summaryは常に、

> 現時点でTechnical Researchから何が分かっているか

を把握するための入口として機能する。


## 16. Feasibility Matrix

Research Summaryには、
候補metricを横断的に比較できるfeasibility matrixを維持する。

具体的な列はResearch中に調整してよいが、
少なくとも以下の情報を比較できるようにする。

- Metric
- Purpose / Decision
- Feasibility
- Source / API
- Public / Private
- Privilege
- Runtime dependency
- Sampling / Cost
- Reliability
- Recommendation

必要に応じてEvidenceやNotesへの参照を追加する。


## 17. 未解決事項の扱い

十分に調査した結果としての「未解決」を、
Technical Researchの正常な成果として認める。

無理に採用・見送りの結論を作らない。

Recommendationを「要追加調査」とする場合などは、
少なくとも以下を明記する。

### Known

現在分かっていること。

### Unknown

まだ分からないこと。

### Evidence

Known / Unknownの判断根拠。

### Why unresolved

なぜ現時点では結論を出せないのか。

### Next step

追加Research、追加experiment、
Design上の見送りなど、次に取り得る対応。

これによって、

> 調査した結果として未解決

なのか、

> 単に調査が不足している

のかを区別できるようにする。


## 18. Cross-cutting Review

Memory、Compute / Thermal、Powerの各Research終了後に、
Cross-cutting Reviewを行う。

ここでは個別metricを再調査することより、
領域をまたぐ技術的な論点を整理する。

少なくとも以下を確認する。

- 共通して利用できる取得方式やlibrary
- sampling architectureに影響する事項
- history保持が必要なmetric
- private APIへの依存
- runtime dependency
- privilege
- monitoring overhead
- error / unavailable metricの扱いに影響する事項
- metric間で重複している情報
- 一緒に取得することでcostを削減できるmetric
- macOS updateに対する保守性
- Designで決定すべき事項

このReviewによって、
個別Researchだけでは見えない全体的なtrade-offを明らかにする。


## 19. Researchで行わないこと

Technical Researchでは以下を行わない。

- production featureの実装
- production codeの変更
- 最終的なpopup UIの設計
- 最終architectureの決定
- 大規模なrefactoring提案
- repository全体のcode review
- LLM backend固有機能の調査
- Intel Mac対応の調査
- macOS monitoring toolの網羅的比較
- 不要なbenchmark infrastructureの構築

Research中にこれらの必要性が見つかった場合は、
実行するのではなくResearch成果物に記録し、
後続工程へ引き渡す。


## 20. Technical Researchの完了条件

Technical Researchは、

> すべての疑問に答えが出たとき

ではなく、

> Design担当が技術的事実を推測で補わなくても、
> 十分な根拠をもって設計判断できる状態になったとき

に完了とする。

そのため、Research完了時には、
各候補metricについて必要十分な範囲で以下が整理されていること。

- 目的
- 取得可能性
- 取得方法候補
- 信頼性
- privilege / dependency
- monitoring overhead
- Evidence
- Recommendation

未解決事項については、
Known / Unknown / Evidence / Why unresolved / Next stepが
明確になっていること。

さらにCross-cutting Reviewによって、

- 共通技術
- sampling
- private APIリスク
- runtime dependency
- monitoring overhead
- metric間の重複
- 保守性
- Designで決定すべき事項

が整理されていること。

この状態を満たしたらTechnical Researchを終了し、
次のDesign工程へ引き渡す。
