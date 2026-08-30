# Rust AI開発向け検証基盤 実装計画

> **AIエージェントへの指示:** REQUIRED SUB-SKILL: この計画をタスクごとに実装するには、利用可能な `subagent-driven-development` スキル（推奨）または `executing-plans` スキルを起動して使用してください。各ステップには追跡用のチェックボックス（`- [ ]`）を使用してください。

**目標:** Rustプロジェクトに、生成AI・開発者・CIが共通して利用できる検証基盤を構築する。`Makefile` を検証コマンドの統一された入口とし、`AGENTS.md` と `docs/development/verification.md` にAI向けの検証ルールを日本語で定義する。

**アーキテクチャ:** Rust標準の `rustfmt`、`cargo check`、Clippy、`cargo test` を中心とし、依存関係のセキュリティ監査に `cargo-audit` を使用する。具体的なCargoコマンドは `Makefile` に集約し、開発者・AI Agent・CIから同じMakeターゲットを利用する。AI Agentは `AGENTS.md` を入口として検証ポリシーを参照し、変更内容に応じて適切な検証レベルを選択する。

**技術スタック:** Rust / Cargo / rustfmt / Clippy / cargo-audit / GNU Make / GitHub Actions

**仕様 (Spec):** `docs/plans/2026-08-30-rust-ai-development-verification-design.md`

**グローバル制約 (Global Constraints):**
- 開発・検証に使用する主要なRustツールはRust標準ツールを優先する。
- Formatterには `rustfmt` を使用する。
- Linterには Clippy を使用する。
- コンパイルチェックには `cargo check` を使用する。
- テストにはRust標準の `cargo test` を使用する。
- 依存関係のセキュリティ監査には `cargo-audit` を使用する。
- 検証コマンドの統一された入口として `Makefile` を使用する。
- AI Agent、人間、CIが可能な限り同じMakeターゲットを使用する。
- AI Agentへの基本的な開発ルールは `AGENTS.md` に日本語で記述する。
- 詳細な検証ルールは `docs/development/verification.md` に日本語で記述する。
- 検証を通すことだけを目的として、テスト・lint・CIを弱めてはならない。
- 初期構成ではCode Coverage、Benchmark、Fuzzing、MSRV検証などを導入しない。
- 既存プロジェクトに不要な構造変更やリファクタリングを行わない。
- 既存プロジェクトの命名規則・ディレクトリ構成・CI構成が存在する場合は、それらを優先して整合させる。
- 実装前に既存の `Makefile`、`AGENTS.md`、`Cargo.toml`、CI設定、既存テスト、および関連する開発ドキュメントを確認する。
- 既存の検証コマンドがある場合は、それらを破壊せず、今回の設計と矛盾しない形で統合する。

---

## ファイル構成

実装対象のファイルを以下の責務に分ける。

### 作成または変更するファイル

```text
AGENTS.md
Makefile
docs/development/verification.md
.github/workflows/ci.yml
```

### 状況に応じて作成・変更するファイル

```text
rustfmt.toml
Cargo.toml
Cargo.lock
```

`rustfmt.toml` は既存のFormatter設定がない場合でも、Rustfmtのデフォルト設定だけで目的を満たすなら作成しない。

`Cargo.toml` / `Cargo.lock` は、`cargo-audit` の導入方法や既存プロジェクトの依存関係に応じて必要な変更がある場合のみ変更する。`cargo-audit` 自体をプロジェクトの通常依存として追加するのではなく、開発環境またはCIで利用できる方法を優先する。

---

# タスク1: 既存プロジェクトの検証基盤を調査する

**ファイル:**

- 変更: なし

**目的:**

実装前に既存プロジェクトの構成を確認し、既存のMakefile、AI Agent向けルール、Cargo設定、CI、テストを破壊しないようにする。

### ステップ1: プロジェクト構成を確認する

- [ ] 以下を確認する。

```bash
pwd
printf '\n--- top-level ---\n'
find . -maxdepth 2 -type f | sort
printf '\n--- Cargo.toml ---\n'
sed -n '1,240p' Cargo.toml
printf '\n--- Makefile ---\n'
if [ -f Makefile ]; then sed -n '1,260p' Makefile; else echo '(none)'; fi
printf '\n--- AGENTS.md ---\n'
if [ -f AGENTS.md ]; then sed -n '1,260p' AGENTS.md; else echo '(none)'; fi
```

**期待結果:** 既存ファイルと現在の開発ルールを把握できる。

### ステップ2: CI設定を確認する

- [ ] 以下を実行する。

```bash
find .github -maxdepth 3 -type f -print 2>/dev/null | sort
```

- [ ] GitHub Actionsのworkflowが存在する場合、それぞれの検証処理を確認する。

```bash
for file in .github/workflows/*.yml .github/workflows/*.yaml; do
    if [ -f "$file" ]; then
        echo "===== $file ====="
        sed -n '1,320p' "$file"
    fi
done
```

**期待結果:** CIで現在実行されているbuild、test、lint、format等を把握できる。

### ステップ3: Rust toolchainと既存検証を確認する

- [ ] 以下を実行する。

```bash
rustc --version
cargo --version
cargo fmt --version
cargo clippy --version
cargo test --all-targets --all-features
```

**期待結果:** Rust toolchainが利用可能で、既存テストの現在の状態を把握できる。

### ステップ4: 調査結果を実装方針に反映する

- [ ] 既存のMakefileやCIがある場合、既存ターゲットを維持したまま今回のターゲットを統合する。
- [ ] 既存のAI向けルールがある場合、既存ルールを削除せず今回の検証ルールを追加する。
- [ ] 既存テストが失敗している場合は、その状態を記録し、今回の変更による失敗と区別できるようにする。

---

# タスク2: Makefileに検証インターフェースを実装する

**ファイル:**

- 作成または変更: `Makefile`

**目的:**

Rustの具体的な検証コマンドをMakeターゲットとして統一し、人間・AI・CIから共通して利用できるようにする。

**インターフェース:**

- 生産:
  - `make fmt`
  - `make fmt-check`
  - `make check`
  - `make lint`
  - `make test`
  - `make audit`
  - `make verify`
  - `make ci`

### ステップ1: Makefileの基本構造を作成する

- [ ] 以下の構造を実際のMakefileに反映する。

```makefile
.PHONY: fmt fmt-check check lint test audit verify ci

fmt:
	cargo fmt

fmt-check:
	cargo fmt --all -- --check

check:
	cargo check --all-targets --all-features

lint:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --all-targets --all-features

audit:
	cargo audit

verify: fmt-check check lint test

ci: verify audit
```

- [ ] 既存Makefileがある場合は、既存ターゲットを維持しながら統合する。
- [ ] 既存ターゲットと名前が衝突する場合は、既存の目的を確認してから統合方法を決定する。

### ステップ2: Makefileの構文とターゲットを確認する

- [ ] 以下を実行する。

```bash
make -n fmt
make -n fmt-check
make -n check
make -n lint
make -n test
make -n verify
```

**期待結果:** 各ターゲットが意図したCargoコマンドに展開される。

### ステップ3: Formatterを確認する

- [ ] 以下を実行する。

```bash
make fmt-check
```

**期待結果:** フォーマット済みであれば成功する。フォーマット違反が存在する場合は、そのファイルが明確に示される。

### ステップ4: コンパイルチェックを確認する

- [ ] 以下を実行する。

```bash
make check
```

**期待結果:** プロジェクトがコンパイルチェックを通過する。

### ステップ5: Linterを確認する

- [ ] 以下を実行する。

```bash
make lint
```

**期待結果:** Clippyが警告をエラーとして扱い、問題があれば終了コードが非0になる。

### ステップ6: テストを確認する

- [ ] 以下を実行する。

```bash
make test
```

**期待結果:** 既存のUnit Test、Integration Test、その他Cargoが対象とするテストが成功する。

### ステップ7: 総合検証を確認する

- [ ] 以下を実行する。

```bash
make verify
```

**期待結果:** `fmt-check`、`check`、`lint`、`test` が順番に実行され、すべて成功する。

---

# タスク3: AI Agent向けの基本ルールをAGENTS.mdに定義する

**ファイル:**

- 作成または変更: `AGENTS.md`

**目的:**

AI Agentが実装だけでなく検証までをタスクの完了条件として扱うようにする。

**インターフェース:**

- 消費: `docs/development/verification.md`
- 消費: `Makefile`

### ステップ1: AGENTS.mdに検証ルールを追加する

- [ ] 以下の内容を、既存のAGENTS.mdがある場合は既存ルールと統合して記述する。

```markdown
# 開発者向け指示

## 基本方針

このリポジトリでは、実装だけでなく検証までをタスクの完了条件とします。

## 検証

検証方法と検証レベルについては、以下を参照してください。

- `docs/development/verification.md`

変更内容に応じて、同ドキュメントに定義された適切な検証を実行してください。

検証コマンドは、可能な限り `Makefile` に定義されたターゲットを使用してください。

## タスク完了条件

タスクを完了とする前に、変更内容に応じた検証を実行してください。

検証を通すことだけを目的として、以下の行為をしてはいけません。

- テストを削除・弱体化する
- 正当な理由なくテストを変更する
- Clippyの警告を無条件に無視する
- lintを無効化して問題を隠す
- CIの検証を弱める
- 無関係な変更を追加する

検証に失敗した場合は、まず失敗原因を確認し、今回の変更が原因であれば修正してください。

今回の変更と無関係な既存の問題である場合は、その旨を報告してください。
```

### ステップ2: AGENTS.mdが既存ルールを壊していないことを確認する

- [ ] 既存のAI Agent向けルールを読み直す。
- [ ] 今回の検証ルールが既存ルールと矛盾していないことを確認する。
- [ ] 既存のプロジェクト固有ルールを削除していないことを確認する。

**期待結果:** AI Agentが参照する基本ルールが日本語で一貫して記述されている。

---

# タスク4: 検証ポリシーをdocs/development/verification.mdに実装する

**ファイル:**

- 作成: `docs/development/verification.md`

**目的:**

AI Agentが変更内容に応じて検証レベルを判断できるよう、検証方法と判断基準をリポジトリ内に固定する。

### ステップ1: 検証レベルを記述する

- [ ] 以下の内容を `docs/development/verification.md` に記述する。

```markdown
# 検証ポリシー

## 目的

このドキュメントでは、変更内容に応じて実行する検証を定義します。

検証は、開発中の高速なフィードバックと、タスク完了時の網羅的な検証を区別します。

## 検証レベル

### Level 0：編集時

コードを編集中に、最小限のフィードバックを得るために使用します。

```bash
make fmt-check
make check
```

### Level 1：実装完了時

1つの機能または修正を実装し終えた段階で実行します。

```bash
make fmt-check
make check
make lint
make test
```

### Level 2：PR作成前

PRを作成する前に実行します。

```bash
make verify
```

### Level 3：CI

CIではLevel 2に加えて依存関係のセキュリティ監査を実行します。

```bash
make verify
make audit
```
```

### ステップ2: 変更内容による判断表を追加する

- [ ] 以下の表を追加する。

```markdown
## 変更内容による判断

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

`△` は変更内容に応じて判断します。
```

### ステップ3: 検証失敗時のルールを追加する

- [ ] 以下を追加する。

```markdown
## 検証失敗時のルール

検証に失敗した場合は、以下の順序で対応します。

1. エラー内容を確認する
2. 今回の変更が原因であるか判断する
3. 今回の変更が原因なら実装を修正する
4. 修正後、必要な最小範囲の検証を再実行する
5. 最終的に必要な検証レベルを実行する

今回の変更と無関係な既存問題の場合は、問題を隠したり回避したりせず、その旨を報告します。
```

### ステップ4: 禁止事項を追加する

- [ ] 以下を追加する。

```markdown
## 禁止事項

検証を通すことだけを目的として、以下を行ってはいけません。

### テスト

- テストを削除する
- テストを弱体化する
- 本来検証すべきケースを検証しなくする
- 実装に合わせて無批判に期待値を書き換える

### Lint

- 正当な理由なくlintを無効化する
- `allow` を追加して問題を隠す
- CIのlint設定を弱める

### CI

- 検証失敗を隠す
- 検証対象を勝手に減らす
- CI設定を変更して検証を回避する

技術的・設計的に正当な理由がある場合は、変更理由を明示した上でルール自体を変更することは許可します。
```

### ステップ5: Makefileとの整合性を確認する

- [ ] `verification.md`に記載したすべてのMakeターゲットがMakefileに存在することを確認する。

```bash
grep -E '^(fmt|fmt-check|check|lint|test|audit|verify|ci):' Makefile
```

**期待結果:** 必要なターゲットがすべて存在する。

---

# タスク5: cargo-auditを利用したセキュリティ監査を導入する

**ファイル:**

- 変更: `.github/workflows/ci.yml`
- 必要な場合のみ変更: `Cargo.toml`
- 必要な場合のみ変更: `Cargo.lock`
- 変更: `Makefile`

**目的:**

依存crateに既知の脆弱性が存在しないことをCIで確認できるようにする。

### ステップ1: cargo-auditの利用可能性を確認する

- [ ] 以下を実行する。

```bash
cargo audit --version
```

**期待結果:** `cargo-audit` が利用可能である。

- [ ] 未インストールの場合は、プロジェクトの既存開発環境・CI方式に合わせて導入方法を決定する。
- [ ] `cargo-audit` をアプリケーションの通常依存関係として `Cargo.toml` に追加しない。

### ステップ2: Makefileのauditターゲットを確認する

- [ ] 以下が存在することを確認する。

```makefile
audit:
	cargo audit
```

### ステップ3: セキュリティ監査を実行する

- [ ] 以下を実行する。

```bash
make audit
```

**期待結果:** RustSec Advisory Databaseに基づく監査が実行される。

### ステップ4: 既知の脆弱性がある場合の扱いを確認する

- [ ] 既知の脆弱性が検出された場合、今回の実装で無理に回避しない。
- [ ] 脆弱性の原因となる依存関係、利用バージョン、修正版の有無を確認する。
- [ ] 既存問題である場合は、その事実を実装結果として報告する。

---

# タスク6: GitHub Actions CIをMakefile中心に構成する

**ファイル:**

- 作成または変更: `.github/workflows/ci.yml`

**目的:**

CIでもMakefileを利用し、ローカル・AI Agent・CIで検証コマンドが重複しないようにする。

### ステップ1: 既存CIがある場合は現在の責務を確認する

- [ ] 既存workflowがある場合、既存のcheckout、Rust toolchain setup、cache、permissions等を維持する。
- [ ] Rust検証コマンドを直接workflowに重複記述している場合、Makefileのターゲットへ移動できるか確認する。

### ステップ2: VerifyジョブをMakefile経由にする

- [ ] CIから以下を実行する構成にする。

```yaml
- name: Verify
  run: make verify
```

- [ ] `cargo fmt`、`cargo check`、`cargo clippy`、`cargo test` をCI YAMLに直接重複記述しない。

### ステップ3: AuditジョブまたはステップをMakefile経由にする

- [ ] 以下を実行する。

```yaml
- name: Security audit
  run: make audit
```

### ステップ4: CI設定を検証する

- [ ] YAMLの構文を確認する。
- [ ] GitHub ActionsでCIを実行する。
- [ ] `make verify` が成功することを確認する。
- [ ] `make audit` が成功することを確認する。

**期待結果:** CIのRust検証ロジックがMakefileに集約され、ローカルとCIで同じ検証入口を利用している。

---

# タスク7: rustfmt設定を確認する

**ファイル:**

- 必要な場合のみ作成: `rustfmt.toml`

**目的:**

Formatter設定をプロジェクトで一貫させる。

### ステップ1: 現在のrustfmt設定を確認する

- [ ] 以下を実行する。

```bash
find . -maxdepth 2 \( -name 'rustfmt.toml' -o -name '.rustfmt.toml' \) -print
```

### ステップ2: 不要な設定を作らない

- [ ] 既存設定がなく、Rustfmtのデフォルト設定で要件を満たす場合は `rustfmt.toml` を新規作成しない。
- [ ] プロジェクト固有のFormatter要件が存在する場合のみ `rustfmt.toml` を作成する。

### ステップ3: Formatterを検証する

- [ ] 以下を実行する。

```bash
make fmt-check
```

**期待結果:** プロジェクト全体が設定されたFormatterルールに従っている。

---

# タスク8: 検証基盤全体を実行して完了条件を確認する

**ファイル:**

- 変更: なし

**目的:**

今回導入した検証基盤そのものが正常に機能することを確認する。

### ステップ1: Formatterを確認する

- [ ] 実行:

```bash
make fmt-check
```

**期待結果:** PASS

### ステップ2: コンパイルチェックを確認する

- [ ] 実行:

```bash
make check
```

**期待結果:** PASS

### ステップ3: Clippyを確認する

- [ ] 実行:

```bash
make lint
```

**期待結果:** PASS

### ステップ4: テストを確認する

- [ ] 実行:

```bash
make test
```

**期待結果:** PASS

### ステップ5: 総合検証を確認する

- [ ] 実行:

```bash
make verify
```

**期待結果:** `fmt-check`、`check`、`lint`、`test` がすべてPASS

### ステップ6: セキュリティ監査を確認する

- [ ] 実行:

```bash
make audit
```

**期待結果:** PASS

### ステップ7: CI相当の検証を確認する

- [ ] 実行:

```bash
make ci
```

**期待結果:** `make verify` と `make audit` がすべてPASS

### ステップ8: 最終差分を確認する

- [ ] 実行:

```bash
git status --short
git diff --check
git diff -- AGENTS.md Makefile docs/development/verification.md .github/workflows/ci.yml rustfmt.toml Cargo.toml Cargo.lock
```

**期待結果:**

- 意図しないファイル変更がない
- whitespace errorがない
- 今回の設計に関係しない変更が含まれていない

---

# タスク9: 実装結果とドキュメントの整合性を確認する

**ファイル:**

- 変更: 必要に応じて今回作成したファイル

**目的:**

実装された検証基盤とデザインドキュメントの間に不整合がないことを確認する。

### ステップ1: Makefileの公開インターフェースを確認する

- [ ] 以下のターゲットが存在することを確認する。

```bash
make -n fmt
make -n fmt-check
make -n check
make -n lint
make -n test
make -n audit
make -n verify
make -n ci
```

### ステップ2: ドキュメントのコマンドを確認する

- [ ] `AGENTS.md` に記載された参照先が存在することを確認する。

```bash
test -f docs/development/verification.md
```

- [ ] `verification.md` に記載されたMakeターゲットがすべて存在することを確認する。

### ステップ3: CIとMakefileの責務を確認する

- [ ] CIにRustの検証コマンドが重複していないことを確認する。
- [ ] Rust検証は原則としてMakefile経由になっていることを確認する。

### ステップ4: AI Agent向けルールを確認する

- [ ] `AGENTS.md` が日本語で記述されていることを確認する。
- [ ] `verification.md` が日本語で記述されていることを確認する。
- [ ] AI Agentが検証ルールを辿れる構造になっていることを確認する。

---

# 完了条件

以下をすべて満たした時点で、この実装は完了とする。

- [ ] `Makefile` に `fmt`、`fmt-check`、`check`、`lint`、`test`、`audit`、`verify`、`ci` が定義されている
- [ ] `make fmt-check` が成功する
- [ ] `make check` が成功する
- [ ] `make lint` が成功する
- [ ] `make test` が成功する
- [ ] `make verify` が成功する
- [ ] `make audit` が成功する
- [ ] `make ci` が成功する
- [ ] `AGENTS.md` にAI Agent向けの検証ルールが日本語で記述されている
- [ ] `docs/development/verification.md` に検証レベルと変更内容による判断基準が日本語で記述されている
- [ ] CIがMakefileの検証ターゲットを利用している
- [ ] CIとローカルでRust検証コマンドが重複していない
- [ ] 検証を通すためだけにテスト、lint、CIを弱めるルールになっていない
- [ ] 不要な `rustfmt.toml` を作成していない
- [ ] Code Coverage、Benchmark、Fuzzing、MSRVなどの未要求の機能を導入していない
- [ ] `git diff --check` が成功する
- [ ] 今回の目的と無関係な変更が含まれていない

---

# 実装後の最終検証

実装完了時には、少なくとも以下を実行する。

```bash
make fmt-check
make check
make lint
make test
make audit
make verify
make ci
git diff --check
```

すべて成功した場合、検証基盤の実装が完了したものとする。

既存プロジェクト由来の失敗が存在する場合は、今回の変更による失敗と区別し、実装完了時の報告に明記する。
