# Activity Monitor Popup Repository Survey

調査日: 2026-09-21
対象: `herdr-mac-activity-monitor` の Activity Monitor Popup 拡張に関係する現在の実装

## 1. Scope

このSurveyでは、後続のMemory / Compute-Thermal / Power / Cross-cutting Researchが参照できるように、現在のentry point、metric取得、データ表現、更新ライフサイクル、window title、popup、Herdr連携、依存関係、テスト、エラー処理、macOS依存コード、取得コストに関係する実装を確認した。

対象はrepository内の実装と設定であり、主に`src/`、`herdr-plugin.toml`、`Cargo.toml`、関連READMEおよび既存テストである。

以下は今回の対象外とした。

- Memory / Compute-Thermal / Power各metricの本格的なTechnical Research
- `macmon`その他外部monitoring toolの調査・比較
- metricの採否、最終UI、最終architecture、sampling architectureの設計
- production codeの実装、refactoring、repository全体のarchitecture review
- Apple Siliconでのmetricの信頼性・取得方式の外部調査

## 2. Repository Structure

| Path | 現在の責務 |
| --- | --- |
| `src/main.rs` | CLI entry point。`--watch`の有無でwatch daemonまたはpopup TUIを選択する。 |
| `src/memory.rs` | `sysctl`と`vm_stat`をsubprocessで実行し、memory raw dataを取得して`MemInfo`へ変換するI/O層。 |
| `src/battery.rs` | `pmset -g batt`をsubprocessで実行し、`Battery`へ変換するI/O層。 |
| `src/parsers.rs` | `VmStat`、`MemInfo`、`Battery`、`ChargingState`のデータ表現、parse、memory計算、popup/titleの文字列整形。主要な純粋ロジックとテストが集約されている。 |
| `src/tui.rs` | popup pane commandの実行体。crosstermでraw mode / alternate screenを使い、3行のTUIを3秒周期で再取得・再描画する。 |
| `src/watch.rs` | startup commandの実行体。pid lockを確認し、3秒周期で取得・window title更新を行う。Herdr接続断等で終了する。 |
| `src/herdr.rs` | HerdrのUnix domain socketへwindow title用JSON-RPCを送信するtransport層。 |
| `herdr-plugin.toml` | plugin metadata、startup watch、popup pane、workspace actionを定義する。 |
| `Cargo.toml` | Rust package設定。直接依存は`crossterm = 0.27`のみ。 |
| `docs/development/verification.md` / `Makefile` | repositoryの検証レベルと`make verify`等の検証入口。 |

`src/main.rs`のmodule宣言はすべて同一binary内のprivate moduleであり、現時点でmetricごとの共有state storeやcollector abstractionは存在しない。

## 3. Runtime / Update Lifecycle

### 起動経路

`src/main.rs:12-20`の`main`が引数を確認し、`--watch`があれば`watch::run()`、それ以外は`tui::run()`を呼ぶ。

`herdr-plugin.toml:8-9`の`[[startup]]`は`./bin/herdr-activity-monitor --watch`を起動する。したがってplugin起動時にwindow title更新プロセスが起動する。`[[panes]]`の`command`（同ファイル11-17）は引数なしで同じbinaryを起動するため、popup表示時はTUI経路になる。`[[actions]]`（19-23）はHerdr CLIの`plugin pane open --plugin ... --entrypoint monitor`を呼ぶ。manifestには`prefix + m`という文字列やkey handlerはなく、prefix操作のtriggerはHerdr側のpane/action機構にあると読める。

### Popup lifecycle

`src/tui.rs:18-27`でraw modeとalternate screenを開始し、終了時にdisable / leave / showを行う。`run_loop`（31-48）は初回ループでmetricを取得して描画し、その後`event::poll(Duration::from_secs(3))`で最大3秒待つ。キー入力があれば`q`またはEscで終了し、それ以外は継続する。タイムアウトまたは無関係な入力の後、次のloopで再取得する。

実質的には初回取得後、3秒単位でmemoryとbatteryを再取得するpollingである。独立したbackground task、thread、履歴保持、共有stateはない。

### Window title lifecycle

`src/watch.rs:51-80`はlock取得後に無限loopを開始する。各loopでmemory、batteryを順番に取得し、titleを整形してHerdrへ送信し、3秒sleepする。したがって初回更新はwatch起動直後、以後は処理時間に加えて3秒間隔で行われる。

socketへの送信失敗時は連続失敗数を増やす。`HERDR_SOCKET_PATH`のmetadataが消えていれば即終了、socketが存在していれば`failed > 3`で終了する（`should_exit`、8-13）。終了時にはwindow title clear requestを一度送る（78行）。

watchの重複起動防止は`HERDR_PLUGIN_STATE_DIR/watch.pid`（32-48）で行う。既存pidは`kill -0` subprocess（17-26）で生存確認される。環境変数がない場合はlockなしで動作する。

## 4. Memory Data Flow

現在の実装で追跡できるmemory data flowは次の通りである。

```text
sysctl -n hw.pagesize       ┐
sysctl -n hw.memsize        ├─ src/memory.rs::get
vm_stat                     ┘
  -> src/parsers.rs::parse_vm_stat -> VmStat (page counters)
  -> src/parsers.rs::compute_memory
  -> MemInfo { used_gb, total_gb, percent }
  -> Option<MemInfo> (取得失敗時は None)
  ├─ format_window_title -> watch -> herdr::set_window_title
  └─ format_lines -> tui::render_frame -> popup pane
```

取得は`src/memory.rs:27-34`。`hw.pagesize`と`hw.memsize`を個別の`sysctl -n` subprocessで読み、`total_pages`を計算した後、`vm_stat` subprocessのstdoutを`parse_vm_stat`へ渡す。必要な`free`、`inactive`、`speculative`、`purgeable`の4項目を`VmStat`として保持する。

加工は`src/parsers.rs:99-123`。4種のページ数を加算してfree相当量を求め、`total_pages - freed`をused pagesとし、bytesを10進GBへ変換する。percentはused / totalをroundした整数である。加算overflowとunderflowは`Err`になる。

stateは永続的な共有stateではない。`watch::run`（60行）は`memory::get().ok()`でそのloop内だけ`Option<MemInfo>`を保持し、`tui::run_loop`（34行）も同様に各frameのローカル変数へ保持する。

window titleへの経路は`parsers::format_window_title`（149-160）で、`used_gb / total_gb / percent`を1行へ整形し、`watch`から`herdr::set_window_title`へ渡す。

popupへの経路は`parsers::format_lines`（129-143）で、同じ`MemInfo`を3行の一部へ整形し、`tui::render_frame`がcrosstermで出力する。

## 5. Battery Data Flow

```text
pmset -g batt
  -> src/battery.rs::get
  -> 対象行（% と presentを含む行）
  -> parsers::split_present / parse_battery_line
  -> Battery::Present { percent, state } または Battery::Absent
  -> 同じloop内のローカル値
  ├─ format_window_title -> watch -> herdr::set_window_title
  └─ format_lines -> tui::render_frame -> popup pane
```

取得は`src/battery.rs:10-20`。`pmset -g batt`を1 subprocessで実行し、stdoutのうち`%`と`present`を含む最初の行を選ぶ。subprocess起動失敗、対象行不在、parse失敗は`Battery::Absent`へフォールバックする。

表現は`src/parsers.rs:37-50`の`ChargingState`（Charging / Discharging / Charged）と`Battery`（PresentまたはAbsent）。`parse_battery_line`（61-85）は`%`直前の数字列を`u8`として読み、文字列に応じて状態を分類する。`present: false`は`split_present`でAbsent扱いになる。

window titleでは`format_window_title`（154-158）が`percent`と小文字のstateを表示する。popupでは`format_lines`（137-142）が`Battery  {}% {}`または`Battery  n/a`へ整形する。memoryと同様、履歴や共有stateへの保存はない。

## 6. Window Title Rendering

titleの文字列生成は`src/parsers.rs:149-160`の`format_window_title`に集中している。現在の形式は`🧠 USED/TOTALGB PERCENT% | 🔋 PERCENT% STATE`で、memory欠損は`🧠 n/a`、battery欠損は`🔋 n/a`となる。

更新呼び出しは`src/watch.rs:60-64`。取得値をこのformatterへ渡し、成功時に`herdr::set_window_title`を呼ぶ。transportは`src/herdr.rs:42-45`でJSON-RPC requestを作り、`HERDR_SOCKET_PATH`のUnix domain socketへ1行送信する。request生成は`client.window_title.set`、終了時のclearは`client.window_title.clear`（`herdr.rs:20-30,48-49`）。

既存title表示を維持する場合に直接影響する箇所は、`watch.rs`の取得・loop、`parsers.rs::format_window_title`、`herdr.rs`のJSON-RPC形式、およびmanifestのstartup commandである。popupの描画経路とはformatter関数が同じデータを受ける点で共有されるが、Herdrへのtransportはwindow title専用で、popupはHerdr paneがプロセスのstdoutを表示する構造である。

## 7. Popup Rendering

`prefix + m`自体のキーバインド実装はrepositoryにない。manifestの`[[actions]]`は`toggle-monitor`というworkspace actionを定義し、Herdr CLIのpane openを呼ぶ（`herdr-plugin.toml:19-23`）。要求文書が示すprefix操作はHerdr側の操作として、このpluginは`monitor` entrypointを提供している。

内容生成は`src/tui.rs:31-36`から`parsers::format_lines`（129-143）へ進む。`format_lines`は`Activity Monitor`、Memory行、Battery行の3行固定`Vec<String>`を返す。`render_frame`（53-60）は画面全体をclearし、originへ移動し、各行をCRLF付きで出力してflushする。

Herdrへ渡す経路は、`herdr.rs`のsocket JSON-RPCではなく、manifestのpane command stdoutである。`herdr-plugin.toml:11-17`が`./bin/herdr-activity-monitor`をpopup paneとして起動し、プロセス自身がcrosstermのalternate screenへ描画する。popupはwindow titleのような共有transportや共有render stateを持たない。

## 8. State and Module Boundaries

- I/O境界: `memory.rs`と`battery.rs`が外部commandを呼ぶ。
- 表現・純粋処理境界: `parsers.rs`がraw outputのparse、memory計算、2種類の表示文字列生成を担う。
- popup runtime境界: `tui.rs`がterminal lifecycle、入力、poll、描画を担う。
- title runtime境界: `watch.rs`がlock、poll loop、failure policyを担う。
- Herdr integration境界: `herdr.rs`がtitle JSON-RPCとUnix socket送信を担う。

現在のstateは各loopのローカル変数だけであり、collector結果を複数consumerへ配信するstore、metric registry、履歴、cross-thread channelは確認できない。MemoryとBatteryは各consumer（popup / watch）が独立に取得するため、popupを開いている間はwatchとの間で値を共有しない。

## 9. Dependencies and macOS Interfaces

`Cargo.toml`の直接依存は`crossterm 0.27`のみ。`cargo tree --depth 2`では`bitflags`、`libc`、`mio`、`parking_lot`、`signal-hook`、`signal-hook-mio`等がtransitive dependencyとして確認できる。metric取得用のRust crateはない。

既存のmacOS / Unix interfaceは以下である。

- `/usr/sbin/sysctl`相当の`sysctl -n hw.pagesize` / `hw.memsize` subprocess（`memory.rs:17-22,27-31`）
- `vm_stat` subprocess（`memory.rs:31-32`）
- `pmset -g batt` subprocess（`battery.rs:10-12`）
- `kill -0` subprocessによるpid生存確認（`watch.rs:17-26`）
- `std::os::unix::net::UnixStream`によるHerdr socket連携（`herdr.rs:1-2,33-40`）
- crosstermのraw mode、alternate screen、event polling、ANSI terminal rendering（`tui.rs:5-14,18-60`）

plugin manifestは`platforms = ["macos"]`であり、`min_herdr_version = "0.7.3"`を指定している（`herdr-plugin.toml:1-6`）。Intel対応やApple Silicon専用分岐は現実装にはない。

## 10. Tests

既存テストは主に純粋なparse / 計算 / formatと、watch・transport・描画の一部を対象とする。

- `src/parsers.rs`: `vm_stat` parse、battery line parse、memory計算のoverflow / underflow、fuzz的な任意文字列でのpanic回避、popup 3行出力、window titleの正常値・欠損値をテストする。
- `src/tui.rs`: clear / origin移動、CRLFによる各行の左端揃え、battery表示をテストする。terminal実機とのinteractive lifecycleはテストしていない。
- `src/herdr.rs`: JSON-RPC requestの形、特殊文字のescape、socket環境変数未設定時のerrorをテストする。実socketとの成功通信はテストしていない。
- `src/watch.rs`: exit policy、pid生存判定、pid lock取得・重複防止をテストする。実際の3秒loop、外部command取得、Herdr socketとの連続更新はテストしていない。

`memory::get`、`battery::get`のsubprocess I/O自体をmockするテストはなく、macOS commandの実機出力・OS version差・取得時間・monitoring overheadもテストされていない。manifestのstartup / pane / actionが実際のHerdrで正しく起動するintegration testも確認できない。

## 11. Monitoring Overhead Observations

今回確認できた事実は以下である。benchmarkは実施していない。

- popupは各更新で`sysctl` 2回、`vm_stat` 1回、`pmset` 1回の外部command invocationを行う（`tui.rs:34-36`、`memory.rs:27-33`、`battery.rs:10-20`）。
- watchも各更新で同じmemory / battery取得を行う（`watch.rs:60-62`）。popupとwatchは値を共有しないため、両方が動作している場合は同じ種類の取得が別プロセスで発生する。
- `Command::output`でsubprocessのstdoutを丸ごと読み、UTF-8 lossily decodeしてStringを生成する。parseはそのStringを行単位で走査する。
- `format_lines`、`format_window_title`、JSON request生成、crossterm出力はいずれも更新ごとにString / Vec等の一時allocationを行う。
- watchは3秒sleepでpollし、popupも3秒のevent pollで次の取得まで待つ。履歴保持や追加の常時pollはない。
- pid lockの`kill -0`はwatch起動時の競合確認時に実行される。title送信は更新ごとにUnix socketへconnectし、1行を書き込む。
- 外部commandの失敗時はmemoryが`None`、batteryが`Absent`となり、表示を継続する。ただしcommand timeoutやstderr内容を保持する処理はない。

## 12. Extension Points and Constraints

現状から後続Researchが把握しておくべきextension pointと制約は以下である。ここでは新architectureを提案しない。

- 新しい取得値を既存の表示経路へ載せる場合、データ表現は`parsers.rs`、取得I/Oは`memory.rs` / `battery.rs`に相当するmodule、popup表示は`format_lines`、title表示は`format_window_title`が現行の接点になる。
- popupとwindow titleは同じ`MemInfo` / `Battery`値を各loop内で別々に作り、2つのformatterが別形式へ変換する。既存titleを維持するにはtitle formatterとwatchの更新経路を壊さない必要がある。
- popupはpane process、titleはstartup watch processであり、process間共有stateはない。新metricが履歴や複数consumer間の整合性を必要とするかは後続Researchで確認が必要である。
- 取得は外部commandに依存し、各更新でsubprocessを生成する。新metricの方式がさらにcommand、権限、private API、長時間処理を必要とする場合、popupとwatchの両方のlifecycleへ影響しうる。
- memory取得失敗とbattery取得失敗の扱いが異なる。memoryは`Result`をcallerが`Option`化し、batteryは取得層で`Absent`へ吸収する。新metricのunavailable/error表現は現状一律ではない。
- `herdr.rs`は現時点でwindow title用JSON-RPCに限定される。popup内容はsocketへ渡さず、stdout/TUIをHerdr paneへ表示する。
- `herdr-plugin.toml`のstartup commandとpane commandは同じbinaryだが引数が異なる。新しい処理を追加する場合、両モードで実行されるか、watchだけ・popupだけで実行されるかが制約になる。

## 13. Open Questions for Later Research

Repository Surveyだけでは次を確定できないため、後続Researchへ引き渡す。

- Memory: 現在の`vm_stat`ベースのused値に加え、RequirementsのMemory Pressure、Swap、available、top memory processをどのmacOS interfaceで取得でき、どの程度信頼できるか。
- Compute / Thermal: Apple Silicon向けCPU/GPU utilization、Thermal State、temperature、powerの取得方式、public/private API、権限、command依存、cost。
- Power: `pmset -g batt`で現在取得している値以外に、drain、平均drain、推定runtimeをどのように取得・表現できるか。履歴が必要な場合、現在のprocess-local・no-history lifecycleで足りるか。
- 共通: popupとwatchが別processで同時に同じmetricを取得することのoverheadと、各metricのsampling頻度を同一の3秒pollに揃えられるか。
- 共通: 新metricの取得失敗、macOS version差、Apple Silicon対象制約を現在の`Option` / `Absent`中心の表示処理へどう反映するかは、Design前に事実確認が必要である。
- Herdr integration: `prefix + m`の実際のtriggerとpane lifecycle、startup/watchの停止・再起動契約はHerdr側の仕様確認が必要で、repository単体からは確定できない。
- Overhead: 外部command方式を継続する場合の実測CPU、spawn回数、memory、power overheadは未計測である。
