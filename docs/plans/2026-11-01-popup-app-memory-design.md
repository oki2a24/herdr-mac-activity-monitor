# Popup app memory display design

## Intent

ポップアップの利用者が、メモリを多く使っているアプリを名前から直感的に把握できるようにする。表示は上位3件のまま、helperを含む同一アプリのプロセスを合算する。

## Approaches

- **アプリ単位で集約（採用）:** 全プロセスのphysical footprintを取得し、実行ファイルが属する最上位の`.app` bundle単位で合計する。アプリ名と合計値を表示でき、helperの利用分も一つの項目になる。
- **上位プロセス名をアプリ名に置換:** 個々の上位プロセスだけを表示する。アプリ名は見やすくなるが、helperに分散したメモリ量をアプリ全体の消費として把握できない。
- **起動中のGUIアプリだけを集約:** `NSRunningApplication`のアプリPIDに属するものを表示する。取得しやすい一方、helperやバックグラウンドプロセスを漏らすため採用しない。

ユーザーの意図はプロセスの識別ではなく、アプリ単位での原因把握である。よって、`.app` bundleに属するプロセスをbundle単位に合算し、bundleの表示名を使う。bundleに属さない項目は`macOS · <名前>`または`Process · <名前>`として区別する。

## Scope and constraints

- 表示件数は集約後の上位3件とする。
- メモリ量は既存のphysical footprintを使い、アプリ内プロセス分を合算する。
- 一覧とfootprint取得はpopup表示中だけ行い、権限昇格は求めない。
- 取得できないPIDは除外し、取得できた項目だけを表示する。
- window titleとMemory / Battery / CPU / Thermalの既存表示は維持する。

## Implementation

`ps`で現在のユーザーに属するPIDを列挙し、`footprint --pid <PID>`を一つのsnapshot commandへ複数指定して取得する。各PIDの実行パスから`.app` bundleを特定する。Foundationの`NSFileManager.displayNameAtPath`で表示名を取得し、bundle pathを集約キーにする。アプリ外のプロセスはプロセス名とPIDで個別表示する。
