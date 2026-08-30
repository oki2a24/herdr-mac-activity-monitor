# Rust AI開発向け検証基盤 デザインドキュメント

**ファイルパス:** `docs/plans/2026-08-30-rust-ai-development-verification-design.md`

**ステータス:** 提案

**作成日:** 2026-08-30

---

## 1. 概要

本ドキュメントは、Rustプロジェクトを生成AI（AI Agent）と共同開発する際の、コード品質・テスト・セキュリティ検証の基本設計を定義する。

目的は、AIに個々のRustツールやコマンドの使い方を都度判断させるのではなく、**リポジトリ自身に検証プロトコルを定義し、人間・AI・CIが同じルールとコマンドを利用できる状態を作ること**である。

検証処理の実行インターフェースには `Makefile` を採用する。

---

## 2. 背景と課題

生成AIを利用した開発では、AIがコードを変更したあとに、

- Formatterを実行する
- コンパイル可能か確認する
- Linterを実行する
- テストを実行する
- 依存関係の安全性を確認する

といった検証が必要になる。

しかし、これらをAIのプロンプトだけで管理すると、以下の問題が発生しやすい。

- AIが検証コマンドを忘れる
- 変更内容に対して過剰な検証を実行する
- 必要な検証を実行しない
- コマンドのオプションを間違える
- 開発時とCIで検証内容が異なる
- プロジェクト固有の検証ルールがAIのセッションごとに失われる
- 検証を通すことだけを目的としてテストやlintを弱める可能性がある

そこで、以下の3つを分離して管理する。

1. **AIへの基本指示**
2. **検証方法・判断基準**
3. **実際に実行するコマンド**

---

## 3. 設計目標

### 3.1 必須目標

以下を満たす。

- Rust標準ツールを基本とする
- Formatterとして `rustfmt` を使用する
- Linterとして `Clippy` を使用する
- コンパイルチェックに `cargo check` を使用する
- テストにRust標準の `cargo test` を使用する
- 依存関係のセキュリティ監査に `cargo audit` を使用する
- Makefileを検証コマンドの統一された入口として使用する
- AI、人間、CIで可能な限り同じ検証コマンドを使用する
- 変更内容に応じて検証レベルを選択できるようにする
- 検証を通すためだけにテストやlintを弱めることを禁止する
- 検証ルールをAIのプロンプトだけに依存させない

### 3.2 非目標

初期段階では以下を必須としない。

- コードカバレッジ
- ベンチマーク
- ファジング
- 複数OS向けのローカル検証
- MSRV検証
- 高度な静的解析ツールの追加
- 多数のサードパーティ製開発ツールの導入

これらはプロジェクトの必要性が明確になった時点で追加する。

---

## 4. 採用するツール

### 4.1 Formatter

**rustfmt**

実行入口:

```bash
make fmt
make fmt-check
```

役割:

- Rustコードの自動整形
- CIでフォーマット違反を検出

---

### 4.2 Linter

**Clippy**

実行入口:

```bash
make lint
```

基本的なCI相当の検証では、警告をエラーとして扱う。

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

ただし、プロジェクト固有の正当な理由がある場合を除き、lintを無効化することで問題を隠してはならない。

---

### 4.3 コンパイルチェック

**cargo check**

実行入口:

```bash
make check
```

開発中の高速なフィードバックに使用する。

`cargo build` ではなく、コンパイル可能性の確認を主目的とする場合は `cargo check` を優先する。

---

### 4.4 テスト

**Rust標準テスト**

実行入口:

```bash
make test
```

内部では基本的に、

```bash
cargo test --all-targets --all-features
```

を使用する。

Unit Testだけでなく、プロジェクトに存在するIntegration Testなども対象とする。

---

### 4.5 セキュリティ監査

**cargo-audit**

実行入口:

```bash
make audit
```

依存crateに既知の脆弱性が存在しないかを確認する。

`cargo-audit` は初期開発時の全検証で必ず実行するのではなく、CIなど適切なタイミングで実行する。

---

## 5. 検証レベル

検証は、フィードバック速度と網羅性のバランスを取るため、複数のレベルに分ける。

### Level 0: 編集中

目的:

> 現在の変更がRustコードとして成立しているかを高速に確認する。

基本:

```bash
make fmt-check
make check
```

AIが小さな変更を行った直後など、高速なフィードバックが必要な場合に使用する。

---

### Level 1: 実装完了時

目的:

> 1つの機能または修正の実装が完了した段階で、基本的な品質を確認する。

```bash
make fmt-check
make check
make lint
make test
```

---

### Level 2: PR作成前

目的:

> PRとして提出可能な状態であることを確認する。

```bash
make verify
```

`make verify` は、Formatter、コンパイル、Linter、テストをまとめて実行する。

---

### Level 3: CI

CIではLevel 2相当の検証に加えて、プロジェクトの状況に応じた追加検証を実行する。

初期構成では、

```bash
make verify
make audit
```

を基本とする。

将来的に必要となった場合は、以下を追加できる。

- Code Coverage
- Benchmark
- Fuzzing
- MSRV
- 複数OS
- 複数Rust toolchain

---

## 6. 変更内容による検証判断

AIは変更内容を確認し、必要な検証を判断する。

基本方針:

| 変更内容 | fmt | check | clippy | test | audit |
|---|---:|---:|---:|---:|---:|
| コメントのみ | ○ | - | - | - | - |
| Rustコード変更 | ○ | ○ | ○ | ○ | - |
| ロジック変更 | ○ | ○ | ○ | ○ | - |
| API変更 | ○ | ○ | ○ | ○ | △ |
| テスト変更 | ○ | ○ | ○ | ○ | - |
| `Cargo.toml`変更 | - | ○ | ○ | ○ | ○ |
| `Cargo.lock`変更 | - | ○ | ○ | ○ | ○ |
| CI変更 | - | - | - | - | △ |
| セキュリティ関連変更 | ○ | ○ | ○ | ○ | ○ |

`△` は変更内容に応じて判断する。

---

## 7. Makefile設計

Makefileは、Rustの具体的な検証コマンドをプロジェクト共通の名前に抽象化する。

想定するターゲット:

```text
make fmt
make fmt-check
make check
make lint
make test
make audit
make verify
make ci
```

基本構造:

```text
make fmt
    ↓
cargo fmt

make fmt-check
    ↓
cargo fmt --all -- --check

make check
    ↓
cargo check --all-targets --all-features

make lint
    ↓
cargo clippy --all-targets --all-features -- -D warnings

make test
    ↓
cargo test --all-targets --all-features

make audit
    ↓
cargo audit

make verify
    ↓
fmt-check
check
lint
test

make ci
    ↓
verify
audit
```

Makefileでは、各ターゲットの責務を明確にする。

AIがCargoコマンドの詳細を毎回組み立てるのではなく、原則としてMakefileのターゲットを利用する。

---

## 8. 人間・AI・CIの共通インターフェース

本設計では、可能な限り以下の構造を維持する。

```text
                 Makefile
                    │
       ┌────────────┼────────────┐
       │            │            │
       ↓            ↓            ↓
     人間          AI Agent       CI
       │            │            │
   make verify  make verify  make verify
```

これにより、

- ローカルでは通るがCIでは通らない
- AIだけ別のコマンドを使っている
- AIが古いコマンドを使っている

といった問題を減らす。

---

## 9. AI Agent向けの設計

AI Agentに対して、個別のRustツールの使用方法を大量にInstructionとして記述しない。

AI Agentには、プロジェクトの検証ポリシーを参照するよう指示する。

入口:

```text
AGENTS.md
```

詳細:

```text
docs/development/verification.md
```

AIの基本的な行動は以下とする。

```text
コード変更
    ↓
変更内容を確認
    ↓
verification.mdを参照
    ↓
必要な検証レベルを判断
    ↓
Makefileのターゲットを実行
    ↓
検証結果を確認
    ↓
失敗していれば原因を調査
    ↓
必要なら修正
    ↓
再検証
```

---

## 10. 検証失敗時のルール

検証に失敗した場合、AIは以下の順序で対応する。

1. エラー内容を確認する
2. 今回の変更が原因であるか判断する
3. 今回の変更が原因なら実装を修正する
4. 修正後、必要な最小範囲の検証を再実行する
5. 最終的に必要な検証レベルを実行する

今回の変更と無関係な既存問題の場合は、問題を隠したり回避したりせず、その旨を報告する。

---

## 11. 禁止事項

検証を通すことだけを目的として、以下を行ってはならない。

### テスト関連

- テストを削除する
- テストを弱体化する
- 本来検証すべきケースを検証しなくする
- 実装に合わせて無批判に期待値を書き換える

### Lint関連

- 正当な理由なくlintを無効化する
- `allow` を追加して問題を隠す
- CIのlint設定を弱める

### CI関連

- 検証失敗を隠す
- 検証対象を勝手に減らす
- CI設定を変更して検証を回避する

ただし、技術的・設計的に正当な理由がある場合は、変更理由を明示した上でルール自体を変更することは許可する。

---

## 12. ファイル構成

初期構成は以下とする。

```text
project/
│
├── AGENTS.md
│
├── Cargo.toml
├── Cargo.lock
├── rustfmt.toml
├── Makefile
│
├── src/
│
├── tests/
│
├── docs/
│   └── development/
│       └── verification.md
│
└── .github/
    └── workflows/
        └── ci.yml
```

### `AGENTS.md`

AI Agent向けの基本的な開発ルール。

詳細な検証方法そのものは記載せず、`verification.md`を参照させる。

### `docs/development/verification.md`

検証レベル、変更内容ごとの検証判断、検証失敗時のルールなどを定義する。

### `Makefile`

検証コマンドの実行インターフェース。

### `rustfmt.toml`

プロジェクト固有のrustfmt設定。

初期段階では不要なカスタマイズを避け、必要になった場合のみ設定を追加する。

### `.github/workflows/ci.yml`

CIでMakefileの検証ターゲットを呼び出す。

---

## 13. CI設計

CIとローカルの検証内容を可能な限り一致させる。

基本:

```text
CI
 │
 ├── make verify
 │
 └── make audit
```

CI固有の環境設定やOSごとの処理が必要になった場合のみ、CI側に追加処理を記述する。

Rustの検証ロジックそのものをCI YAMLに重複して記述しない。

例えば、

```yaml
run: cargo clippy --all-targets --all-features -- -D warnings
```

をCIに直接記述するのではなく、

```yaml
run: make lint
```

とする。

これにより、検証コマンドの変更時にMakefileだけを修正すればよい構成にする。

---

## 14. 将来の拡張

プロジェクトの成長に応じて、以下のターゲットを追加できる。

```text
make coverage
make bench
make fuzz
make msrv
```

### Code Coverage

`cargo-llvm-cov` を候補とする。

### Benchmark

Criterionを候補とする。

### Fuzzing

`cargo-fuzz` を候補とする。

### MSRV

プロジェクトでMinimum Supported Rust Versionを明確に定義する必要が生じた場合に導入を検討する。

これらは初期構成では導入しない。

---

## 15. 設計原則

本設計では、以下を重要な原則とする。

### 原則1: 検証ルールはリポジトリに置く

AIのセッションや使用するAgentに依存しない。

### 原則2: コマンドはMakefileに集約する

AIに毎回Cargoコマンドを組み立てさせない。

### 原則3: 開発時は高速なフィードバックを優先する

小さな変更に対して常に全検証を実行しない。

### 原則4: タスク完了時は十分な検証を行う

高速化を理由に最終検証を省略しない。

### 原則5: CIを最終的な品質ゲートとする

ローカル・AIによる検証をCIの代替とは考えない。

### 原則6: 検証を通すために品質を下げない

テスト・lint・CIを弱めて問題を隠すことを禁止する。

### 原則7: 必要以上にツールを増やさない

Rust標準ツールを中心とし、追加ツールは明確な目的ができた時点で導入する。

---

## 16. 完成状態

この設計が実装された状態では、開発者またはAI Agentが以下のように操作できることを目標とする。

### 開発中

```bash
make check
```

### Formatter確認

```bash
make fmt-check
```

### Lint

```bash
make lint
```

### テスト

```bash
make test
```

### PR前の総合検証

```bash
make verify
```

### CI相当

```bash
make ci
```

AI Agentは変更内容に応じて適切なターゲットを選択し、検証結果を確認した上でタスク完了を判断する。

---

## 17. デザイン上の結論

本プロジェクトでは、Rustの開発ツールを個別にAIへ指示するのではなく、

```text
                 リポジトリ
                     │
          ┌──────────┴──────────┐
          │                     │
    開発ルール              検証ルール
     AGENTS.md          verification.md
          │                     │
          └──────────┬──────────┘
                     │
                  Makefile
                     │
       ┌─────────────┼─────────────┐
       ↓             ↓             ↓
     人間          AI Agent        CI
       │             │             │
       └─────────────┼─────────────┘
                     ↓
             Rust標準ツール群
                     │
        ┌────────────┼────────────┐
        ↓            ↓            ↓
     rustfmt       Clippy      cargo test
        │            │            │
        └────────────┼────────────┘
                     ↓
                品質ゲート
```

という構造を採用する。

これにより、生成AIを利用した開発においても、検証方法・検証基準・実行コマンドをリポジトリ側で一貫して管理できるようにする。
