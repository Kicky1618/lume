## 33. 最適化とビルドプロファイル

この章では、開発時の速さと本番時の最適化をどう切り替えるかを整理する。ビルドプロファイル、IR の変換、最適化の責務分担が主題になる。

```txt
fast-build     max-runtime
    \           /
     -> IR passes ->
     -> emitted artifacts
```

「何を速くしたいのか」と「どこでコストを払うのか」を分けて読むと、この章の意図が見えやすい。

Lume はビルドプロファイルを明示的に持つ。

最適化方針は大きく 2 種類に分ける。

```txt
fast-build: ビルドは高速。実行性能は標準。
max-runtime: ビルドは低速。実行性能は最大化。
```

UI 言語は開発中にすぐ反映されないと使い物にならない。一方で本番では 1ms の差で人間がなぜか売上グラフを眺め始める。したがって、Lume は開発時と本番時で最適化戦略を明確に分離する。

---

### 32.1 ビルドプロファイル

#### 32.1.1 fast-build

`fast-build` は開発体験を優先する。

```txt
目的: 高速ビルド、短い再ビルド時間、明瞭なデバッグ
主用途: 開発サーバー、ホットリロード、プロトタイピング
```

特徴。

```txt
incremental compile 有効
型チェックは必要最小限
重い最適化 pass は無効
CSS minify 無効または軽量
JS minify 無効または軽量
WASM 最適化は O0 / O1
ソースマップ高精度
診断メッセージ詳細
runtime assertion 有効
```

CLI。

```bash
lume build --profile fast-build
```

省略形。

```bash
lume dev
```

`lume dev` は既定で `fast-build` を使用する。

---

#### 32.1.2 max-runtime

`max-runtime` は実行性能を優先する。

```txt
目的: 最小バンドル、最速起動、最速更新、最速 Server Action
主用途: 本番ビルド、配布、ベンチマーク
```

特徴。

```txt
全体最適化有効
型情報を使った特殊化有効
dead code elimination 有効
CSS tree shaking 有効
JS minify 有効
WASM 最適化 O3 / Oz 選択可能
ルート単位 code splitting 有効
テンプレート事前コンパイル有効
Server Action AOT compile 有効
FFI binding 最適化有効
runtime assertion 削減
source map は任意
```

CLI。

```bash
lume build --profile max-runtime
```

または。

```bash
lume build --release
```

`--release` は `--profile max-runtime` の別名である。

---

### 32.2 プロファイル設定

`lume.toml`。

```toml
[build]
default_profile = "fast-build"

[build.profiles.fast-build]
optimize = "fast-build"
source_map = "full"
minify = false
wasm_opt = "O0"
runtime_assertions = true
incremental = true

[build.profiles.max-runtime]
optimize = "max-runtime"
source_map = "hidden"
minify = true
wasm_opt = "O3"
runtime_assertions = false
incremental = false
```

---

### 32.3 最適化パイプライン

Lume の最適化は以下の段階で行われる。

```txt
source
  -> lexer
  -> parser
  -> AST
  -> semantic analysis
  -> Lume IR
  -> UI optimization
  -> state optimization
  -> style optimization
  -> asset optimization
  -> backend optimization
  -> code generation
  -> post optimization
```

各プロファイルは、この pass 群の有効・無効・強度を切り替える。

---

### 32.4 Lume IR

Lume はソースコードを直接 JS / WASM / Native に変換しない。

中間表現として **Lume IR** を使う。

```txt
AST: 構文構造
Lume IR: 意味解析済みの UI / state / action / style / route / server 処理
Backend IR: JIT / Native backend 向け低レベル IR
```

IR を分ける理由。

1. UI 最適化とサーバー最適化を分離できる
2. HTML / JS / CSS / WASM / Native の複数出力に対応できる
3. incremental build の差分単位を小さくできる
4. 型情報を使った特殊化ができる
5. デバッグ情報を保持できる

---

### 32.5 fast-build の最適化

`fast-build` では、ビルド速度に影響が大きい最適化を避ける。

有効化される pass。

```txt
syntax cache
module dependency cache
incremental AST reuse
incremental type check
template shallow compile
style token resolve
light dead code marking
route manifest update
source map full generation
```

無効化または軽量化される pass。

```txt
global tree shaking
cross-component specialization
advanced CSS merging
WASM O3 optimization
whole-program analysis
native LTO
action inlining
layout static analysis
```

このモードでは出力が多少冗長でも許容する。

開発中に重要なのは、理論上 3% 速い DOM 更新ではなく、保存してから画面が変わるまでの短さである。人間の集中力は思っているより薄い。

---

### 32.6 max-runtime の最適化

`max-runtime` では、ビルド時間を犠牲にして実行性能を最大化する。

有効化される pass。

```txt
whole-program analysis
global dead code elimination
cross-component constant folding
state dependency graph optimization
DOM patch specialization
template static extraction
route-level code splitting
CSS tree shaking
CSS selector minimization
critical CSS extraction
JS minification
WASM O3/Oz optimization
native backend AOT compile
native LTO
FFI call boundary optimization
server action specialization
serialization schema precompile
```

---

### 32.7 UI 最適化

UI 最適化は `view` を対象にする。

#### 32.7.1 static node extraction

状態に依存しないノードは静的 HTML として抽出する。

```lume
view {
  Column {
    Text("Static title")
    Text("Count: {count}")
  }
}
```

最適化後。

```txt
Static title -> HTML に固定出力
Count        -> 動的 text patch 対象
```

---

#### 32.7.2 dynamic binding table

動的部分だけを binding table に登録する。

```json
{
  "bindings": [
    {
      "id": 4,
      "kind": "text",
      "dependsOn": ["count"],
      "template": "Count: {count}"
    }
  ]
}
```

---

#### 32.7.3 event delegation

大量の同種イベントは親要素に委譲できる。

```lume
for item in items key=item.id {
  Button(item.name) {
    on click {
      select(item.id)
    }
  }
}
```

`max-runtime` では個別 listener ではなく、delegated listener に変換できる。

```txt
1 parent listener
  -> event target lookup
  -> action dispatch
```

---

#### 32.7.4 DOM patch specialization

汎用 patch ではなく、専用更新関数を生成する。

汎用。

```js
applyPatch({ op: "setText", id: 4, value })
```

専用。

```js
function updateCount(value) {
  n4.textContent = `Count: ${value}`
}
```

`max-runtime` では専用関数を優先する。

---

### 32.8 状態最適化

Lume は state の依存関係を解析する。

```lume
state count: Int = 0
derived doubled = count * 2
```

依存グラフ。

```txt
count
  -> doubled
  -> text binding
```

状態更新時、関係する binding だけを更新する。

---

### 32.9 derived 最適化

`derived` は依存グラフに基づいて最適化される。

```lume
derived fullName = user.firstName + " " + user.lastName
```

`user.age` が変わっても `fullName` は再計算しない。

`max-runtime` では field-level dependency tracking を有効にできる。

```txt
user.firstName
user.lastName
```

単位で依存を追う。

---

### 32.10 ループ最適化

`for` は key を使って差分更新する。

```lume
for user in users key=user.id {
  UserRow(user=user)
}
```

最適化。

```txt
append only detection
remove detection
move detection
keyed reconciliation
static row template reuse
```

`fast-build` では単純な keyed update を使う。

`max-runtime` では変更パターンに応じて専用 reconciler を生成できる。

---

### 32.11 条件分岐最適化

```lume
if status == "loading" {
  Spinner()
} else {
  Content()
}
```

`max-runtime` では branch ごとの DOM fragment を事前生成し、切り替え時に最小操作で差し替える。

```txt
mount branch A
unmount branch A
mount branch B
```

静的 branch は template cache に置く。

---

### 32.12 CSS 最適化

#### 32.12.1 token folding

```lume
padding: md
```

が固定値の場合、CSS 変数参照ではなく実値へ畳み込める。

```css
padding: 16px;
```

テーマ切り替えがある場合は CSS 変数を維持する。

---

#### 32.12.2 unused style elimination

未使用 style を削除する。

```lume
style unused {
  color: red
}
```

参照がなければ出力しない。

---

#### 32.12.3 critical CSS extraction

初期表示に必要な CSS を `index.html` へ inline できる。

```html
<style data-lume-critical>
  ...
</style>
```

残りは遅延読み込みする。

---

#### 32.12.4 selector minimization

`max-runtime` では内部クラス名を短縮できる。

```css
.lume-Button-primary-large
```

から。

```css
.a
```

へ変換する。

ソースマップにより元 style 名へ戻せる。

---

### 32.13 JavaScript 最適化

```txt
constant folding
function inlining
dead branch elimination
event handler specialization
state setter specialization
DOM reference hoisting
module concatenation
route-level code splitting
```

DOM 参照は初期化時に束縛する。

```js
const n1 = document.getElementById("l1")
```

更新時に querySelector を繰り返さない。

人間が書く JS はよくここで `document.querySelector` を毎回呼ぶ。やめてほしい。機械が代わりに覚える。

---

### 32.14 WASM 最適化

WASM ターゲットでは以下を最適化する。

```txt
state transition
validation
router matching
diff calculation
serialization / deserialization
numeric processing
```

`fast-build`。

```txt
wasmOpt: O0 or O1
symbols: keep
debug info: full
```

`max-runtime`。

```txt
wasmOpt: O3 or Oz
symbols: strip
debug info: optional
bounds checks: optimized where safe
```

WASM と JS の境界呼び出しはコストがあるため、`max-runtime` では細かすぎる呼び出しをまとめる。

悪い例。

```txt
JS -> WASM set count
JS -> WASM compute text
JS -> WASM compute class
JS -> WASM patch
```

良い例。

```txt
JS -> WASM dispatch event
WASM -> returns compact patch list
JS -> apply patches
```

---

### 32.15 Server Action 最適化

Server Action は JIT / Native backend で最適化される。

#### 32.15.1 fast-build

```txt
JIT compile
lazy action compile
schema validation interpreted
FFI binding lazy resolve
transaction wrapper generic
```

#### 32.15.2 max-runtime

```txt
AOT compile
schema validation compiled
serialization specialized
auth guard inlined
transaction path specialized
FFI symbol prelinked
hot action inlining
```

例。

```lume
server action add(a: i32, b: i32): i32 {
  return a + b
}
```

`max-runtime` では型検査済みの専用関数になる。

```txt
parse i32, i32
call native add_i32_i32
serialize i32
```

---

### 32.16 FFI 最適化

FFI は境界コストが高いため、`max-runtime` では以下を行う。

```txt
symbol preloading
call signature specialization
string conversion caching
zero-copy Bytes passing
batch call generation
owned value auto-free insertion
thread-safe call parallelization
non-thread-safe call serialization
```

FFI 呼び出しがループ内にある場合、警告する。

```lume
for item in items {
  native.process(item) // warning in max-runtime analysis
}
```

可能なら batch FFI を推奨する。

```lume
native.processBatch(items)
```

---

### 32.17 Native backend 最適化

Native backend では以下を選べる。

```txt
optLevel: 0 | 1 | 2 | 3 | s | z
lto: false | thin | full
codegenUnits: number
panic: unwind | abort
strip: false | symbols | all
cpu: generic | native
```

設定例。

```toml
[backend]
mode = "native"

[backend.release]
opt_level = 3
lto = "thin"
strip = "symbols"
cpu = "generic"
```

CPU 固有最適化。

```toml
[backend]
mode = "native"

[backend.release]
cpu = "native"
```

`cpu: "native"` は実行環境が固定されている場合のみ推奨する。

---

### 32.18 JIT backend 最適化

JIT backend は実行時情報を使って最適化できる。

```txt
hot action detection
inline cache
shape specialization
route matcher specialization
schema validator specialization
FFI call caching
```

開発中は compile latency を優先する。

```txt
tier 0: interpreter or baseline JIT
tier 1: optimized JIT for hot paths
```

本番で JIT を使う場合は、ウォームアップ時間を考慮する。

---

### 32.19 Asset 最適化

```txt
image hashing
asset fingerprinting
preload hints
modulepreload hints
font subsetting
compression manifest
brotli/gzip pre-generation
```

出力例。

```txt
assets/app.8f3a1.js
assets/style.0ab21.css
assets/app.91c2e.wasm
```

---

### 32.20 Code Splitting

Lume は route 単位で分割できる。

```txt
/              -> home.js, home.css
/ranking       -> ranking.js, ranking.css
/users/:id     -> user.js, user.css
```

共有 chunk。

```txt
shared.js
shared.css
```

WASM も分割できる。

```txt
app.core.wasm
ranking.wasm
image-tools.wasm
```

---

### 32.21 Hydration 最適化

```txt
partial hydration
lazy hydration
interaction hydration
visible hydration
idle hydration
```

指定例。

```lume
component HeavyChart hydrate="visible" {
  view {
    CanvasChart(data=data)
  }
}
```

```lume
component SearchBox hydrate="interaction" {
  view {
    Input(label="Search")
  }
}
```

---

### 32.22 プリフェッチ

```lume
Link(to="/ranking", prefetch="hover") {
  Text("Ranking")
}
```

候補。

```txt
none
hover
visible
intent
immediate
```

`max-runtime` では route manifest をもとに必要な JS / CSS / WASM を事前取得できる。

---

### 32.23 コンパイルキャッシュ

Lume は以下をキャッシュする。

```txt
token stream
AST
semantic graph
type check result
Lume IR
style IR
route manifest
WASM object
native object
FFI symbol table
```

cache key。

```txt
source hash
compiler version
config hash
target triple
profile
feature flags
dependency hashes
```

---

### 32.24 Incremental Build

`fast-build` は incremental build を必須機能とする。

変更単位。

```txt
file
component
style block
theme token
route
server action
ffi module
```

例。

```txt
Button.lume changed
  -> parse Button.lume
  -> update dependency graph
  -> re-emit affected JS/CSS only
  -> preserve unrelated WASM/native objects
```

---

### 32.25 最適化ヒント

開発者は最適化ヒントを与えられる。

```lume
component UserRow memo {
  view {
    Text(user.name)
  }
}
```

```lume
derived expensive memo = compute(items)
```

```lume
for item in items key=item.id stable {
  RowView(item=item)
}
```

`stable` は順序や identity が頻繁に変わらないことを示す。

嘘をつくと表示が壊れる可能性があるため、`unsafe stable` 扱いにして警告する。

---

### 32.26 最適化診断

```txt
LUME4001: repeated querySelector detected in generated path
LUME4002: loop is missing stable key
LUME4003: FFI call inside hot loop
LUME4004: large component cannot be statically extracted
LUME4005: hydration strategy may delay interaction
LUME4006: WASM boundary call is too fine-grained
LUME4007: server action cannot be AOT-specialized
LUME4008: dynamic style prevents CSS extraction
LUME4009: route chunk exceeds configured size budget
LUME4010: native build uses cpu=native; binary may not be portable
```

---

### 32.27 サイズ予算

```toml
[budgets]
js = "100KB"
css = "30KB"
wasm = "200KB"
route = "150KB"
```

超過時。

```txt
warning[LUME4009]: route chunk exceeds budget
  route: /ranking
  budget: 150KB
  actual: 212KB
```

`max-runtime` では budget 超過を error にできる。

---

### 32.28 ベンチマークモード

```bash
lume bench
```

測定対象。

```txt
initial render
hydration time
state update latency
route transition
server action latency
FFI call overhead
WASM dispatch overhead
bundle size
```

出力例。

```txt
initial render: 18.4ms
hydration: 5.1ms
count update: 0.08ms
server action add: 0.31ms
wasm dispatch: 0.02ms
```

---

### 32.29 プロファイル別推奨用途

```txt
fast-build
  開発
  UI 試作
  ホットリロード
  デバッグ
  詳細ソースマップ

max-runtime
  本番
  CDN 配信
  ネイティブ Server Actions
  FFI 多用
  ベンチマーク
```

---

### 32.30 最適化の基本原則

Lume の最適化は以下の原則に従う。

1. `fast-build` は待ち時間を減らす
2. `max-runtime` は実行時間を減らす
3. 意味を変える最適化は禁止
4. 危険な最適化は明示指定を要求する
5. source map と診断で生成物を追跡可能にする
6. FFI と WASM 境界のコストを常に考慮する
7. DOM 更新は依存する state に限定する
8. 静的に決まるものは可能な限り HTML / CSS へ逃がす

最適化は魔法ではない。だいたい依存関係の記録と余計な仕事の削除である。魔法扱いすると、三週間後に誰も読めないビルドログが残る。

---

## 34. 内部 IR・最適化・コード生成詳細

この章では、Lume コンパイラ内部で使う IR、最適化 pass、WASM 生成、Native コード生成、LLVM / Inkwell 連携を定義する。

Lume は表面上は UI 構築言語だが、内部的には複数段階のコンパイラである。

```txt
.lume source
  -> Token Stream
  -> AST
  -> Semantic Graph
  -> Lume IR
  -> Optimized Lume IR
  -> Backend IR
  -> JS / CSS / HTML
  -> LLVM IR
  -> WASM / Native
```

要するに、ボタンを1個置くためにコンパイラパイプラインを建てる。人類はそこまで来た。

---

### 33.1 Lume IR

Lume IR は、構文解析後の意味解析済み中間表現である。

AST はソースコードの形を保持するが、Lume IR は UI、状態、イベント、スタイル、Server Actions、FFI を最適化しやすい形に正規化する。

#### 33.1.1 IR の目的

```txt
UI 構造を静的部分と動的部分に分離する
状態依存関係を明示する
イベントから状態変更までの経路を表す
Server Action を backend 向けに低レベル化する
FFI 呼び出し境界を明示する
JS / WASM / Native の複数出力に対応する
incremental build の差分単位にする
```

#### 33.1.2 IR の主要ノード

```ts
export type LumeIrNode =
  | IrModule
  | IrComponent
  | IrPage
  | IrRoute
  | IrViewBlock
  | IrElement
  | IrText
  | IrDynamicText
  | IrIf
  | IrFor
  | IrState
  | IrDerived
  | IrAction
  | IrEffect
  | IrStyle
  | IrTheme
  | IrServerAction
  | IrFfiModule
```

#### 33.1.3 IrModule

```ts
export interface IrModule {
  kind: "IrModule"
  id: ModuleId
  path: string
  imports: IrImport[]
  exports: IrExport[]
  components: IrComponent[]
  routes: IrRoute[]
  serverActions: IrServerAction[]
  ffiModules: IrFfiModule[]
  styles: IrStyle[]
  themeRefs: IrThemeRef[]
}
```

#### 33.1.4 IrComponent

```ts
export interface IrComponent {
  kind: "IrComponent"
  id: ComponentId
  name: string
  props: IrProp[]
  state: IrState[]
  derived: IrDerived[]
  actions: IrAction[]
  effects: IrEffect[]
  view: IrViewBlock
  hydration: HydrationStrategy
  optimizationHints: OptimizationHint[]
}
```

#### 33.1.5 IrElement

```ts
export interface IrElement {
  kind: "IrElement"
  id: NodeId
  tag: string
  componentRef?: ComponentId
  staticAttrs: IrAttribute[]
  dynamicAttrs: IrDynamicAttribute[]
  events: IrEventBinding[]
  children: IrViewNode[]
  styleRefs: StyleId[]
  accessibility: A11yMetadata
}
```

#### 33.1.6 IrDynamicText

```ts
export interface IrDynamicText {
  kind: "IrDynamicText"
  id: NodeId
  template: string
  dependencies: StateDependency[]
  expression: IrExpression
}
```

---

### 33.2 UI 最適化

UI 最適化は `view` を静的 HTML、動的 binding、イベント binding に分解する。

#### 33.2.1 Static / Dynamic 分離

入力。

```lume
view {
  Column {
    Text("Ranking")
    Text("Count: {count}")
  }
}
```

IR。

```txt
StaticElement Column
  StaticText "Ranking"
  DynamicText template="Count: {count}" deps=[count]
```

出力方針。

```txt
StaticElement -> HTML / template
DynamicText   -> binding table
```

#### 33.2.2 Template hoisting

同じ構造を持つ UI 断片は template として巻き上げる。

```txt
for user in users
  UserRow(user)
```

`UserRow` の静的構造は1回だけ生成し、各行では clone して binding だけ差し替える。

#### 33.2.3 DOM reference hoisting

動的 node の DOM 参照は初期化時に束縛する。

```js
const n4 = root.querySelector('[data-lume-id="4"]')
```

更新時に再検索しない。

```js
n4.textContent = value
```

#### 33.2.4 Event delegation

複数の同種イベントは親に集約できる。

```txt
click on list item
  -> parent listener
  -> data-lume-event-id lookup
  -> dispatch action
```

`fast-build` では単純な listener を許可する。

`max-runtime` では delegated listener を優先する。

---

### 33.3 状態依存グラフ

Lume は state、derived、view binding、action の関係をグラフ化する。

```txt
StateNode
DerivedNode
BindingNode
ActionNode
EffectNode
```

例。

```lume
state count: Int = 0
derived doubled = count * 2
Text("{doubled}")
```

依存グラフ。

```txt
count
  -> doubled
    -> text binding
```

#### 33.3.1 StateDependency

```ts
export interface StateDependency {
  source: StateId | PropId | DerivedId
  path?: string[]
  access: "read" | "write" | "readwrite"
}
```

#### 33.3.2 Field-level dependency

`max-runtime` ではオブジェクト単位ではなく field 単位で依存を追う。

```lume
derived fullName = user.firstName + " " + user.lastName
```

依存。

```txt
user.firstName
user.lastName
```

`user.age` の更新では `fullName` を再計算しない。

---

### 33.4 derived 最適化

`derived` は純粋式として扱う。

#### 33.4.1 constant folding

```lume
derived size = 8 * 2
```

はコンパイル時に `16` へ畳み込む。

#### 33.4.2 memoization

```lume
derived expensive memo = compute(items)
```

`memo` 指定がある場合、依存値が変わるまで再計算しない。

#### 33.4.3 recompute pruning

同じ state 更新で複数の derived が影響を受ける場合、依存順に一度だけ再計算する。

```txt
state changed
  -> affected derived set
  -> topological sort
  -> recompute once
```

#### 33.4.4 purity check

`derived` 内では副作用を禁止する。

```lume
derived bad = console.log(count) // error
```

---

### 33.5 ループ最適化

`for` は key 付き差分更新を基本とする。

```lume
for user in users key=user.id {
  UserRow(user=user)
}
```

#### 33.5.1 Loop IR

```ts
export interface IrFor {
  kind: "IrFor"
  id: NodeId
  itemName: string
  indexName?: string
  source: IrExpression
  key: IrExpression
  body: IrViewBlock
  strategy: LoopStrategy
}
```

#### 33.5.2 LoopStrategy

```txt
append-only
keyed-reconcile
stable-index
full-replace
```

`fast-build` では `keyed-reconcile` を汎用実装で行う。

`max-runtime` では変更傾向に応じて specialized reconciler を生成する。

#### 33.5.3 append-only detection

push のみで増える配列は append-only として扱える。

```lume
items.push(newItem)
```

この場合、既存 DOM を比較せず末尾に追加する。

---

### 33.6 条件分岐最適化

`if` / `match` は branch 単位で最適化する。

#### 33.6.1 Branch IR

```ts
export interface IrIf {
  kind: "IrIf"
  condition: IrExpression
  thenBlock: IrViewBlock
  elseBlock?: IrViewBlock
  branchStrategy: BranchStrategy
}
```

#### 33.6.2 BranchStrategy

```txt
inline-toggle
fragment-swap
lazy-mount
static-branch
```

#### 33.6.3 static branch elimination

条件がコンパイル時に確定する場合、不要 branch を削除する。

```lume
if env.production {
  Analytics()
} else {
  DebugPanel()
}
```

`max-runtime` では `env.production` の値に応じて片方を削除できる。

---

### 33.7 CSS 最適化

CSS 最適化は style IR を対象とする。

#### 33.7.1 Style IR

```ts
export interface IrStyle {
  kind: "IrStyle"
  id: StyleId
  name: string
  declarations: IrCssDeclaration[]
  states: IrCssStateBlock[]
  media: IrCssMediaBlock[]
  usedBy: NodeId[]
}
```

#### 33.7.2 token folding

固定 theme token は実値へ畳み込む。

```lume
padding: md
```

```css
padding: 16px;
```

テーマ切り替えが有効な場合は CSS 変数を維持する。

#### 33.7.3 CSS tree shaking

`usedBy` が空の style は削除する。

#### 33.7.4 Atomic CSS generation

`max-runtime` では重複宣言を atomic class に分解できる。

```css
.a{display:flex}.b{gap:8px}.c{padding:16px}
```

#### 33.7.5 Critical CSS

初期 route の above-the-fold に必要な CSS を inline 化できる。

---

### 33.8 JS 最適化

JavaScript 出力は DOM glue とイベント dispatch を担当する。

#### 33.8.1 JS 出力の役割

```txt
DOM reference binding
event listener setup
state container
patch application
WASM bridge
Server Action client stub
router
asset loader
```

#### 33.8.2 最適化 pass

```txt
constant folding
DCE
DOM reference hoisting
event dispatch table compression
route chunk splitting
import graph flattening
minify
property mangling where safe
```

#### 33.8.3 Event dispatch table

```js
const handlers = {
  12: e => actions.increment(),
  13: e => actions.submit(e)
}
```

`max-runtime` では numeric event id を使う。

```js
h[12](e)
```

---

### 33.9 WASM 最適化

WASM は UI の全処理を置き換えるのではなく、計算・状態遷移・差分計算を高速化するために使う。

#### 33.9.1 WASM に置くもの

```txt
state transition
schema validation
router matching
derived calculation
loop diff calculation
patch list generation
serialization
heavy numeric logic
```

#### 33.9.2 JS に残すもの

```txt
DOM API 呼び出し
event listener 接続
fetch / WebSocket / browser API
CSSOM 操作
WASM module loading
```

#### 33.9.3 WASM patch ABI

```txt
lume_init(config_ptr: u32, config_len: u32) -> void
lume_dispatch(event_id: u32, payload_ptr: u32, payload_len: u32) -> u32
lume_get_patch_len(patch_ptr: u32) -> u32
lume_free(ptr: u32) -> void
```

patch buffer は compact binary format とする。

```txt
u8 op
u32 node_id
u32 payload_len
bytes payload
```

#### 33.9.4 WASM optimization profile

```txt
fast-build:
  LLVM opt: O0/O1
  debug symbols: keep
  names section: keep
  binaryen: optional

max-runtime:
  LLVM opt: O3 or Oz
  debug symbols: strip by default
  names section: strip by default
  binaryen: wasm-opt -O3 or -Oz
```

---

### 33.10 Server Action 最適化

Server Actions は Lume IR から Backend IR へ変換される。

#### 33.10.1 Action lowering

```txt
server action
  -> auth guard
  -> csrf guard
  -> rate limit guard
  -> input decode
  -> validation
  -> transaction wrapper
  -> action body
  -> output encode
```

#### 33.10.2 fast-build

```txt
lazy JIT compile
interpreted validation
generic serializer
generic transaction wrapper
symbol lookup on demand
```

#### 33.10.3 max-runtime

```txt
AOT compile
compiled validation
specialized serializer
auth guard inlining
transaction wrapper specialization
FFI symbol prelink
```

---

### 33.11 FFI 最適化

FFI は Server Action / Native backend から C / C++ を呼ぶ境界である。

#### 33.11.1 最適化対象

```txt
symbol lookup
argument marshalling
string conversion
bytes transfer
owned value free insertion
batching
thread-safety scheduling
```

#### 33.11.2 zero-copy

`Borrowed<Bytes>` は可能な限りコピーなしで渡す。

```lume
fn hash(input: Borrowed<Bytes>): u64
```

#### 33.11.3 auto-free

`Owned<T> free=...` はスコープ終了時に自動解放を挿入する。

```lume
const out = image.resize(input)
return out
```

戻り値として返す場合は、所有権を Lume runtime に移す。

#### 33.11.4 batch FFI

ループ内 FFI 呼び出しは batch 化を推奨する。

```lume
native.processBatch(items)
```

---

### 33.12 Native backend 最適化

Native backend は Backend IR をネイティブコードへ AOT コンパイルする。

Lume の Native backend は内部で **LLVM** を使用し、Rust 実装では **Inkwell** を LLVM バインディングとして利用する。

#### 33.12.1 Native pipeline

```txt
Backend IR
  -> LLVM IR
  -> LLVM optimization pipeline
  -> object file
  -> linker
  -> native executable / shared library
```

#### 33.12.2 Inkwell の役割

Inkwell は以下に使う。

```txt
LLVM Context 管理
Module 生成
Function 生成
BasicBlock 生成
Builder による命令生成
型生成
ターゲット triple / data layout 設定
optimization pass 実行
object code emission
JIT execution engine 接続
```

#### 33.12.3 LLVM module 構成

```txt
lume_core.ll
lume_actions.ll
lume_ffi.ll
lume_validation.ll
lume_routes.ll
```

`max-runtime` では複数 module を link して whole-program optimization を行う。

#### 33.12.4 Native 最適化オプション

```txt
optLevel: 0 | 1 | 2 | 3 | s | z
lto: none | thin | full
cpu: generic | native | specific
relocation: static | pic
debugInfo: none | line-tables | full
strip: none | symbols | all
panic: unwind | abort
```

#### 33.12.5 CPU feature

```ts
backend: {
  mode: "native",
  cpu: "x86-64-v3",
  features: ["sse4.2", "avx2"]
}
```

`cpu: "native"` は配布バイナリでは非推奨である。

---

### 33.13 JIT backend 最適化

JIT backend は開発時と一部本番用途で使う。

内部では LLVM JIT を利用できる。

Rust 実装では Inkwell の ExecutionEngine、または LLVM ORC JIT 連携層を使う。

#### 33.13.1 JIT pipeline

```txt
Backend IR
  -> LLVM IR
  -> baseline optimization
  -> JIT compile
  -> function pointer
  -> runtime dispatch table
```

#### 33.13.2 Tiered JIT

```txt
tier 0: interpreter / baseline JIT
tier 1: hot action optimized JIT
tier 2: max-runtime equivalent optimized JIT
```

#### 33.13.3 Hot path detection

```txt
call count
latency
allocation count
FFI frequency
serialization cost
```

一定回数以上呼ばれる Server Action は最適化 JIT へ昇格できる。

---

### 33.14 LLVM / Inkwell による WASM 生成

Lume は WASM 生成にも LLVM を利用できる。

Rust 実装では Inkwell で LLVM IR を構築し、target triple を `wasm32` 系に設定して WebAssembly object を生成する。

#### 33.14.1 WASM pipeline

```txt
Lume IR / Backend IR
  -> LLVM IR
  -> wasm32 target
  -> LLVM optimization pipeline
  -> wasm object
  -> wasm linker
  -> app.wasm
  -> optional wasm-opt
```

#### 33.14.2 Target triple

代表例。

```txt
wasm32-unknown-unknown
wasm32-wasi
wasm32-wasip1
wasm32-wasip2
```

ブラウザ UI 用の標準は `wasm32-unknown-unknown` とする。

サーバー側 WASI 実行や sandboxed plugin 用には `wasm32-wasi` 系を使える。

#### 33.14.3 WASM memory model

WASM module は linear memory を持つ。

JS bridge は pointer / length でデータを渡す。

```txt
String -> UTF-8 encode -> wasm memory
Bytes  -> copy or shared buffer view
Patch  -> compact binary buffer
```

#### 33.14.4 Export symbols

```txt
lume_init
lume_dispatch
lume_get_patch_len
lume_free
lume_alloc
```

#### 33.14.5 Import symbols

WASM は DOM を直接操作しないため、必要な外部機能は JS import とする。

```txt
env.now_ms
env.log
env.fetch_stub
env.random_u64
```

SSR 安定性が必要な場所では `random_u64` や `now_ms` の使用を診断する。

---

### 33.15 Asset 最適化

Asset 最適化は JS / CSS / WASM / 画像 / フォントを対象にする。

```txt
content hashing
fingerprinting
brotli precompression
gzip precompression
font subsetting
image size validation
preload hint generation
modulepreload generation
cache manifest generation
```

出力。

```txt
assets/app.8f3a1.js
assets/style.0ab21.css
assets/app.91c2e.wasm
assets/font.12ab9.woff2
```

---

### 33.16 Code Splitting

Lume は route、component、WASM module、Server Action group 単位で分割できる。

#### 33.16.1 route split

```txt
/              -> home.js, home.css
/ranking       -> ranking.js, ranking.css, ranking.wasm
/users/:id     -> user.js, user.css
```

#### 33.16.2 shared chunk

```txt
shared.dom.js
shared.router.js
shared.css
shared.wasm
```

#### 33.16.3 split strategy

```txt
route
component
interaction
visibility
manual
```

例。

```lume
component HeavyEditor split="interaction" {
  view {
    Editor()
  }
}
```

---

### 33.17 Hydration 最適化

Lume は hydration 戦略をコンポーネント単位で持つ。

```txt
immediate
idle
visible
interaction
manual
server-only
static
```

例。

```lume
component Chart hydrate="visible" {
  view {
    CanvasChart(data=data)
  }
}
```

#### 33.17.1 partial hydration

静的な UI は hydration しない。

動的な island だけを起動する。

#### 33.17.2 interaction hydration

最初のユーザー操作で JS / WASM を読み込む。

```txt
pointerover
focus
click
input
```

---

### 33.18 プリフェッチ

Link は関連 chunk を先読みできる。

```lume
Link(to="/ranking", prefetch="hover") {
  Text("Ranking")
}
```

候補。

```txt
none
hover
visible
intent
immediate
```

`max-runtime` では route manifest から必要な JS / CSS / WASM を算出する。

```json
{
  "route": "/ranking",
  "assets": [
    "ranking.js",
    "ranking.css",
    "ranking.wasm"
  ]
}
```

---

### 33.19 コンパイルキャッシュ

Lume は段階ごとにキャッシュする。

```txt
Token Stream
AST
Semantic Graph
Lume IR
Optimized Lume IR
Style IR
Backend IR
LLVM IR
Object File
WASM Object
Native Object
FFI Symbol Table
Route Manifest
```

#### 33.19.1 cache key

```txt
source hash
compiler version
config hash
profile
target triple
LLVM version
Inkwell feature set
dependency hashes
environment flags
```

LLVM version が変わった場合、LLVM IR / object cache は無効化する。

---

### 33.20 Incremental Build

Incremental Build は `fast-build` の中核である。

#### 33.20.1 差分単位

```txt
file
component
style block
theme token
route
server action
ffi module
wasm function
native function
```

#### 33.20.2 invalidation

```txt
source change
  -> affected AST node
  -> affected semantic graph node
  -> affected IR node
  -> affected output chunk
```

#### 33.20.3 LLVM incremental

LLVM / Inkwell 側では関数単位 module 分割を行い、変更された関数だけを再生成できるようにする。

```txt
server_action_createPost.ll
server_action_deletePost.ll
ffi_bridge_image_resize.ll
```

`fast-build` では小さな LLVM module を多く作り、再コンパイル範囲を狭める。

`max-runtime` では最後に link / LTO でまとめる。

---

### 33.21 最適化ヒント

開発者はコンパイラへヒントを与えられる。

```lume
component UserRow memo {
  view {
    Text(user.name)
  }
}
```

```lume
derived heavy memo = compute(items)
```

```lume
for item in items key=item.id stable {
  Row(item=item)
}
```

```lume
server action search(input: SearchInput): SearchResult
  hot
{
  return await searchIndex.query(input)
}
```

```lume
ffi module fastmath unsafe hot {
  fn dot(a: Borrowed<Bytes>, b: Borrowed<Bytes>): f32
}
```

#### 33.21.1 hint の扱い

```txt
memo: 再計算抑制を優先
stable: loop reconciliation を軽量化
hot: JIT / Native 最適化優先度を上げる
cold: code size を優先する
inline: 関数 inline を要求する
noinline: inline を禁止する
```

ヒントは意味を変えてはならない。

意味を変える可能性があるヒントは `unsafe` を要求する。

---

### 33.22 サイズ予算

サイズ予算は build config で指定する。

```toml
[budgets]
js = "100KB"
css = "30KB"
wasm = "200KB"
route = "150KB"
native = "10MB"
```

超過時。

```txt
warning[LUME4009]: route chunk exceeds budget
  route: /ranking
  budget: 150KB
  actual: 212KB
```

`max-runtime` では warning を error に昇格できる。

---

### 33.23 ベンチマークモード

```bash
lume bench
```

測定対象。

```txt
parse time
semantic analysis time
IR generation time
optimization pass time
HTML emit time
CSS emit time
JS emit time
WASM generation time
LLVM optimization time
Native link time
initial render
hydration time
state update latency
route transition latency
server action latency
FFI overhead
WASM dispatch overhead
bundle size
```

出力例。

```txt
compile:
  parse: 8.2ms
  ir: 4.1ms
  js emit: 12.4ms
  wasm emit: 41.7ms
  llvm native: 308.2ms

runtime:
  initial render: 18.4ms
  hydration: 5.1ms
  count update: 0.08ms
  server action add: 0.31ms
  ffi call: 0.04ms
  wasm dispatch: 0.02ms
```

---

### 33.24 最適化診断コード

```txt
LUME4001: repeated querySelector detected in generated path
LUME4002: loop is missing stable key
LUME4003: FFI call inside hot loop
LUME4004: large component cannot be statically extracted
LUME4005: hydration strategy may delay interaction
LUME4006: WASM boundary call is too fine-grained
LUME4007: server action cannot be AOT-specialized
LUME4008: dynamic style prevents CSS extraction
LUME4009: route chunk exceeds configured size budget
LUME4010: native build uses cpu=native; binary may not be portable
LUME4011: LLVM optimization disabled for max-runtime target
LUME4012: Inkwell target triple does not match output target
LUME4013: WASM export is missing required runtime symbol
LUME4014: native object cache invalidated by LLVM version change
LUME4015: LTO requested but backend object is incompatible
LUME4016: hot Server Action was not promoted to optimized JIT tier
LUME4017: FFI boundary prevents zero-copy optimization
LUME4018: field-level dependency tracking disabled by dynamic property access
```

---

### 33.25 LLVM / Inkwell 実装方針

Lume コンパイラ本体を Rust で実装する場合、LLVM 連携には Inkwell を用いる。

#### 33.25.1 crate 構成案

```txt
crates/
  lume_parser/
  lume_ast/
  lume_ir/
  lume_opt/
  lume_codegen_js/
  lume_codegen_css/
  lume_codegen_html/
  lume_codegen_llvm/
  lume_codegen_wasm/
  lume_backend_jit/
  lume_backend_native/
  lume_cli/
```

#### 33.25.2 LLVM codegen crate

```txt
lume_codegen_llvm
  lower_ir.rs
  types.rs
  functions.rs
  blocks.rs
  values.rs
  intrinsics.rs
  debug_info.rs
  target.rs
  passes.rs
```

#### 33.25.3 Lume 型から LLVM 型への対応

```txt
Bool    -> i1 / i8 ABI
Int     -> i64 internal, explicit FFI type at boundary
Float   -> double
String  -> { i8*, usize }
Bytes   -> { i8*, usize }
Array<T> -> { T*, usize, usize }
Result<T,E> -> tagged union
Option<T> -> tagged union or nullable representation
Handle<T> -> opaque pointer
```

#### 33.25.4 LLVM IR 生成例

Lume。

```lume
server action add(a: i32, b: i32): i32 {
  return a + b
}
```

概念的 LLVM IR。

```llvm
define i32 @lume_action_add(i32 %a, i32 %b) {
entry:
  %sum = add i32 %a, %b
  ret i32 %sum
}
```

#### 33.25.5 Backend 切り替え

```txt
HTML/CSS/JS only:
  LLVM 不要

HTML/CSS/JS/WASM:
  Lume IR -> LLVM IR -> wasm32 -> app.wasm

Native backend:
  Backend IR -> LLVM IR -> target object -> executable/shared library

JIT backend:
  Backend IR -> LLVM IR -> ExecutionEngine/ORC JIT -> function pointer
```

---

### 33.26 LLVM を使わない領域

以下は LLVM に通さない。

```txt
HTML emit
CSS emit
通常の JS glue emit
source map generation
asset hashing
manifest generation
```

理由は単純で、HTML と CSS を LLVM に通す意味はほぼない。そこまでやると、最適化ではなく儀式になる。

---

### 33.27 最適化安全性

最適化 pass は以下を守る。

```txt
UI の意味を変えない
イベント順序を変えない
observable side effect を削除しない
auth / csrf / validation を削除しない
FFI 所有権を破壊しない
SSR 安定性を悪化させない
```

危険な最適化は `unsafe optimize` を要求する。

```lume
unsafe optimize {
  assumeStableObjectShape(user)
}
```

---
