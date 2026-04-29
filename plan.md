# Lume v0.1 / v0.2 実装計画

この計画は、仕様書の v0.1 MVP を最短で成立させることを目的に整理したものです。
最初の実行可能成果物は HTML + CSS + JS であり、WASM / LLVM / Native / JIT は v0.1 では skeleton までに留めます。

## 1. 目標

- `.lume` を Rust 製コンパイラで読み込み、解析し、HTML + CSS + JS に変換できる状態を作る
- コンポーネント、state、action、view、if、for、基本 UI を通して、最小の UI アプリを動かせるようにする
- 基本診断、`lume build`、`lume dev` の簡易 watch build を成立させる
- 将来拡張のために、WASM / LLVM / Native の crate だけは早めに骨組みを作る

## 2. 実装順

### 現在の実装状況

2026-04-29 時点で、v0.1 MVP は完了。v0.2 も小規模な複数ファイル UI プロジェクトを扱える範囲まで完了。

- Rust workspace と主要 crate を作成済み
- `lume init`, `lume build`, `lume check`, `lume fmt`, `lume dev` を実装済み
- `.lume` の読み込み、lexer、parser、AST、HIR lowering、最小 type check、IR、HTML/CSS/JS/manifest 出力を実装済み
- `Text`, `Button`, `Input`, `Image`, `Box`, `Row`, `Column`, `Grid`, `Stack` の最小 codegen を実装済み
- `state`、`view`、`on click`、`on input(value)`、`if`、`for` の構文処理を実装済み
- `if` / `for` を含む view では、state 更新後に root 再描画する動的 runtime path を実装済み
- `for` 内イベントで loop item / index を `data-lume-scope` 経由で action に渡せるように実装済み
- root 再描画時に `Input` の focus / selection を `data-lume-focus-key` 経由で復元できるように実装済み
- `theme` 宣言を AST / IR に載せ、CSS 変数へ lowering できるように実装済み
- `style` 宣言を `.l-style-*` class へ lowering し、`style=...` 属性から参照できるように実装済み
- `style` の `hover` / `active` / `disabled` と `at sm/md/lg/xl` の最低限 lowering を実装済み
- `lume.manifest.json` に routes / styles / themes を出力するように実装済み
- `app.js` が `lume.manifest.json` から初期 state を復元できるように実装済み
- local `.lume` import の存在確認を `lume_resolver` に追加済み
- local `.lume` import の export 検証を追加済み
- component scope の名前表を作成し、type check の未知 component 診断へ接続済み
- local import した component / style / theme を IR へ取り込み、custom component を HTML / JS codegen でインライン展開できるように実装済み
- custom component の props と default / named slot を IR 展開で扱えるように実装済み
- 複数 component を含む entry では `App` component を優先して root にするように実装済み
- diagnostics の caret 幅と error / warning count summary を改善済み
- `lume fmt --check` を追加済み
- `lume dev` で watch build と静的 Web サーバーを同時に起動できるように実装済み
- duplicate state 診断 `LUME3005` を追加済み
- `examples/` に counter、input、conditional、loop event、theme/style、gallery、scoreboard、form state のサンプルプロジェクトを追加済み
- `lume_resolver` を追加し、標準モジュール import の最小検証を実装済み
- `Image` の `alt` 欠落、`Input` の label / aria-label 欠落、空 `Button`、未宣言 state 代入の診断を実装済み
- state 型検査で `i32` / `i64` / `u32` / `u64` / `f64` などの基本数値型を許容済み
- LLVM / WASM / Native / JIT / runtime / FFI / LSP の skeleton crate を作成済み
- parser、typeck、JS codegen の最小回帰テストを追加済み

v0.1 後の主な課題:

- `if` / `for` の動的再描画は root 再描画方式。差分更新は v0.3 以降
- HIR は AST wrapper に近い。component scope の本格名前解決は v0.2 以降
- custom component の child state isolation は v0.3 以降
- theme mode、nested route、Outlet、dynamic segment、catch-all は v0.3 以降
- formatter はインデント中心。AST ベース formatter は v0.3 以降

### フェーズ 1: ワークスペースと基盤

最初に Rust workspace と共通基盤を整える。

- `lume_cli`, `lume_driver`, `lume_session`, `lume_span`, `lume_diagnostics` を用意する
- ファイル読み込み、`lume.toml` 解析、source map、span、診断表示を通す
- CLI の入口を作り、`lume build` が最小限動く状態を作る

完了条件:

- `.lume` ファイルを 1 つ指定して読み込める
- span 付きの診断を出せる

状態: 完了。

### フェーズ 2: 字句解析・構文解析・AST

仕様の構文をまず機械的に扱えるようにする。

- lexer でトークン化する
- parser で Program / Decl / ViewNode を組み立てる
- AST を仕様の EBNF と対応する形で定義する
- エラー時に位置情報付きで復帰できるようにする

対象範囲:

- module / import / component / page / layout / route / theme / style / type
- server action / form / ffi の構文予約
- view 内の if / for / match / slot / text / element 系

完了条件:

- 正常な `.lume` を AST に変換できる
- 構文エラーを分かりやすく返せる

状態: v0.1 完了。MVP 構文は解析できる。より強いエラー復帰と詳細な reserved 構文は v0.2 以降。

### フェーズ 3: 名前解決・HIR・型検査

構文木をそのまま後段に流さず、解析しやすい形へ正規化する。

- AST から HIR へ lowering する
- import / module / 標準モジュールの名前解決を行う
- component、state、action、view の最小型検査を入れる
- route パラメータ、基本属性、標準 UI コンポーネントの型を検証する

優先度の高い検査:

- 不正な型注釈
- 未知の component / standard module
- state の不正な代入
- 基本的な prop 型不一致

完了条件:

- 明らかな型エラーをビルド時に止められる
- HIR が後段の codegen に渡せる

状態: v0.1 完了。未宣言 state 代入、duplicate state、基本 a11y、未知型 warning、標準モジュール import の最小検証、local import 存在確認は実装済み。

v0.2 完了。component / page / layout の名前表を作成し、同一ファイル component と export された local import を type check で既知 component として扱えるようにした。local import が未 export item を要求した場合は `LUME6004`、検査不能な local module は `LUME6107`、raw JavaScript import は `LUME6009` として診断する。IR は local import した exported component / style / theme を取り込み、codegen は custom component の view をインライン展開できる。custom component の props と default / named slot も IR 展開で扱える。

### フェーズ 4: Lume IR と UI コア

描画と状態更新の共通表現を作る。

- component / state / derived / action / view を Lume IR に落とす
- if / for / match / slot を IR で表現する
- Text / Button / Input / Image / Form / Box / Row / Column / Grid / Stack を最小対応する
- イベントと state 更新の対応を定義する

この段階で扱う最小 UI:

- カウンタ
- 入力と送信
- 条件分岐付き表示
- 繰り返し表示

完了条件:

- 仕様のカウンタ例を IR まで落とせる
- state 更新とイベントを表現できる

状態: v0.1 完了。counter、input event、`if` / `for` の root 再描画 runtime、loop scope の event 捕捉、input focus / selection 復元は HTML/JS まで出力済み。

### フェーズ 5: スタイル・テーマ・アクセシビリティ・ルーター

UI の外形とナビゲーションを実際の出力へ結びつける。

- style DSL を CSS に lowering する
- theme を CSS 変数へ変換する
- standard module のうち `lume/std/ui`, `lume/std/layout`, `lume/std/router` の最小セットを解決する
- route / page / layout / Outlet / dynamic segment / catch-all を扱う
- 必須アクセシビリティ診断を入れる

MVP で必須の診断例:

- `Image` の `alt` 欠落
- `Input` の label / aria-label 欠落
- `Button` の空テキスト
- 不自然な見出しレベル
- 不正な role と要素の組み合わせ

完了条件:

- CSS 変数付きの style 出力ができる
- ルート定義から manifest を組める
- 代表的な a11y 問題を検出できる

状態: v0.1 完了。layout 属性、style 宣言、state style、responsive block、theme token から CSS 変数と class を生成できる。theme mode と本格 router は v0.2 以降。

### フェーズ 6: HTML + CSS + JS 出力

最初の実行可能成果物を作る。

- `index.html` を生成する
- `style.css` を生成する
- `app.js` に state、イベント、hydration glue を出す
- `lume.manifest.json` を生成する

出力先:

```txt
dist/
  index.html
  assets/
    app.js
    style.css
    lume.manifest.json
```

完了条件:

- counter 例がブラウザで動く
- state 更新後に画面が反映される
- manifest から初期復元できる

状態: v0.1 完了。counter、input、条件分岐、繰り返しの state 更新は生成 JS で動く。manifest は state / route / style / theme 情報を出力でき、runtime は manifest から初期 state を復元できる。

### フェーズ 7: CLI・watch・formatter

開発体験を整える。

- `lume build`, `lume dev`, `lume check`, `lume fmt`, `lume init` を用意する
- 簡易 watch build を実装する
- formatter を仕様の整形規則に合わせる
- 主要診断のメッセージを整える

完了条件:

- 小規模プロジェクトで `lume dev` が回る
- `lume fmt` が安定して同じ結果を返す

状態: v0.1 完了。CLI、簡易 watch build、dev Web サーバー、`fmt --check` は実装済み。formatter は暫定だが idempotent。

### フェーズ 8: skeleton バックエンド

v0.1 の外側にあるが、将来の拡張のために crate だけ先に揃える。

- `lume_codegen_llvm`
- `lume_codegen_wasm`
- `lume_backend_native`
- `lume_backend_jit`

ここでは実装を深追いしない。
初期化、型定義、最小テスト、依存配線までに留める。

状態: skeleton crate 作成済み。LLVM / WASM / Native / JIT の最小テストを追加済み。

## 2.1 v0.2 への次の実装ステップ

v0.1 は完了。次は品質と表現力を上げる。

1. 名前解決と標準モジュール検証
   - 完了: `lume_resolver` crate を追加する
   - 完了: `import { Text } from "lume/std/ui"` の exported item を検証する
   - 完了: 未知の標準モジュール / 未 export item を `LUME6101` / `LUME6102` として診断する
   - 完了: local `.lume` import の存在確認を行う
   - 完了: local `.lume` import の export item を検証する
   - 完了: component scope の名前表を作る
   - 完了: type check の未知 component 診断を名前表に接続する
   - 完了: local import した component / style / theme を IR へ取り込む
   - 完了: custom component の view を HTML / JS codegen でインライン展開する
   - 完了: custom component の props と default / named slot を扱う

2. 動的 view runtime
   - 完了: `if` / `for` を state 更新後にも反映できる rendering path を追加する
   - 完了: まずは root 再描画方式で正しさを優先する
   - v0.3: 既存の text binding 更新と統合し、差分更新へ近づける
   - 完了: `for` 内イベントで loop item / index を action に渡せるようにする
   - 完了: `Input` を含む root 再描画時の focus / selection 維持を入れる

3. style / theme lowering
   - 完了: `theme default { color primary = ... }` を CSS variables に変換する
   - 完了: `style card { ... }` を CSS class に変換する
   - 完了: `Box style=card` から style class を参照する
   - 完了: `hover` / `active` / `disabled` state style を lowering する
   - 完了: `at md` responsive block を lowering する

4. manifest / tooling
   - 完了: manifest に route / style / theme の概要を出力する
   - 完了: `lume fmt --check` を追加する
   - 完了: MVP 機能を確認するサンプルプロジェクト群を追加する
   - 完了: `lume dev` で `dist/` を配信する local Web server を起動する
   - 完了: manifest から初期 state を復元する runtime hook を追加する

5. formatter と診断の品質改善
   - v0.3: formatter を parser AST ベースに近づける
   - 完了: span 表示の caret 幅、複数診断、warning/error count を整える

## 3. v0.1 ではやらないこと

- Server Actions の本実装
- query / mutation / search の実運用
- C / C++ FFI の本格リンク
- SSR の本実装
- Native backend / JIT backend の実行機能
- raw JavaScript 埋め込み
- React / Vue / Svelte などへのコード生成

## 4. 完了基準

v0.1 の完了基準と状態。

- 完了: `.lume` を AST まで正しく解析できる
- 完了: component / state / action / view の型検査が動く
- 完了: HTML + CSS + JS を生成できる
- 完了: counter 例がブラウザで動く
- 完了: `lume dev` が変更を検知して再ビルドでき、Web サーバーで配信できる
- 完了: 基本診断と formatter が使える
