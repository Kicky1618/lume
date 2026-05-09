## 29. 低レベル拡張ポリシー

この章は、Lume がどの実行形態を取るか、そしてどこまで低レベルへ降りるかを説明する。ターゲット選択と runtime の境界をここで決める。

```txt
source
  -> target selection
  -> DOM / JS / WASM / native
  -> runtime execution
```

実装の細部よりも、「何を許可し、何を禁止するか」という境界線を見ると全体の設計意図がつかみやすい。

Lume は JavaScript の直接埋め込みを許可しない。

低レベル拡張は **C / C++ FFI** に限定する。

禁止されるもの。

```txt
raw js
inline JavaScript
script block
JavaScript module import
NPM package import
DOM API の手書き呼び出し
```

許可されるもの。

```txt
Lume 標準モジュール
Lume source module
C / C++ FFI module
CSS は Lume style / theme 経由
HTML は view / component 経由
UnsafeHTML は明示的な sanitizer 付きのみ
```

実装では raw JavaScript 埋め込みは受け付けない。

low-level 描画連携は `Canvas` ではなく `NativeCanvas` / `GpuCanvas` の専用要素で表現する。

`data-lume-native-*` 属性は runtime の内部表現であり、Lume ソース上の public API としては扱わない。

### 28.1 禁止例

```lume
raw js {
  console.log("raw")
}
```

これはエラーである。

```txt
error[LUME6010]: raw JavaScript embedding is not allowed
```

### 28.2 C / C++ FFI を使う例

```lume
ffi module fastmath {
  language "c"
  sources ["./native/fastmath.c"]

  fn sin_fast(x: f64): f64
}

server action compute(x: f64): f64
  runtime "native"
{
  return fastmath.sin_fast(x)
}
```

### 28.3 ブラウザ API へのアクセス

ブラウザ API は Lume 標準モジュール経由でのみ使う。

```lume
import { navigate } from "lume/std/router"
```

直接の JavaScript 呼び出しは禁止する。

```lume
window.location.href = "/" // error
```

コンパイラは必要な最小限の JavaScript glue を生成できるが、開発者が任意の JavaScript を埋め込むことはできない。

---

## 30. コンパイルターゲット

Lume v0.1 以降の正式コンパイルターゲットは、以下の 2 系統に限定する。

```txt
HTML + JavaScript + CSS
HTML + JavaScript + CSS + WebAssembly
```

React / Vue / Svelte / Solid / Angular などの UI フレームワーク向けコード生成は正式ターゲットに含めない。

つまり、Lume は「また別の JSX 製造機」ではない。そこまで来ると人類は本当に何も学んでいないことになる。

---

### 29.1 HTML + JavaScript + CSS ターゲット

標準ターゲット。

```txt
.lume
  -> index.html
  -> app.js
  -> style.css
  -> lume.manifest.json
```

役割。

```txt
HTML: 初期 DOM 構造
JavaScript: 状態管理、イベント、差分更新、ルーティング、Server Action 呼び出し
CSS: テーマ、レイアウト、スタイル、アニメーション
Manifest: ルート、Server Actions、assets、hydration / resumability 情報
```

現行の `lume.manifest.json` はこれに加えて `version`、`component`、`target`、`backends`、`routeTree`、`styles`、`themes`、`queries`、`ffi` 系の配列を含む。`lume.backend.json` も同時に生成され、server action と native bridge の実行情報を持つ。

resumable build では、manifest に `resumeGraph`、`symbols`、`eventBindings`、`serializedState` の概要を追加する。これらは実行コードそのものではなく、DOM 上の marker と遅延ロード対象の symbol を対応付ける索引である。

---

### 29.2 HTML + JavaScript + CSS + WebAssembly ターゲット

高性能ターゲット。

```txt
.lume
  -> index.html
  -> app.js
  -> style.css
  -> app.wasm
  -> lume.manifest.json
```

WASM は以下に使える。

1. テンプレート評価
2. 状態更新ロジック
3. 差分計算
4. バリデーション
5. ルーティング matcher
6. FFI / native bridge の共通 ABI
7. 高負荷な計算処理

DOM 操作自体は JavaScript 経由で行う。

WebAssembly が DOM を直接操作できない環境を考えると、ここを無理に神格化すると面倒が増える。WASM は速い部品、JS はブラウザとの接着剤として扱う。

実装では WASM はまだ一枚岩の patch runtime ではなく、`lume_init` の初期化と補助関数の提供が主である。`lume_dispatch` ベースの差分適用や WASM route matcher は [未実装]。

---

### 29.3 禁止ターゲット

以下は正式出力ターゲットにしない。

```txt
React TSX
Vue SFC
Svelte component
Solid JSX
Angular component
Next.js app router
Remix route module
```

互換レイヤーや実験的プラグインとして外部実装することは妨げないが、Lume 本体仕様には含めない。

---

### 29.4 出力ディレクトリ

```txt
dist/
  index.html
  assets/
    app.js
    style.css
    app.wasm
    lume.manifest.json
```

WASM を使わない場合は `app.wasm` を生成しない。

---

## 31. DOM 生成・更新モデル

Lume は UI を直接 DOM に反映する。

React 風の仮想 DOM を必須とはしない。

実装バックエンドは以下から選択できる。

```txt
fine-grained: 依存単位で DOM を直接更新する
block-diff: コンパイル済みブロック単位で差分更新する
full-render: 小規模 UI 向けに部分木を再生成する
wasm-diff: WASM 側で差分計算し JS が DOM に適用する
```

---

### 31.1 HTML 出力

Lume。

```lume
component App {
  state count: Int = 0

  view {
    Column gap=12 padding=16 {
      Text("Count: {count}")

      Button("増やす") {
        on click {
          count += 1
        }
      }
    }
  }
}
```

HTML 出力例。

```html
<div data-lume-component="App" data-lume-id="c0">
  <div class="l-column" data-lume-id="n1">
    <span data-lume-text="count">Count: 0</span>
    <button data-lume-on="click:count.increment">増やす</button>
  </div>
</div>
```

---

### 31.2 JavaScript 出力

```js
const state = {
  count: 0
}

const nodes = {
  countText: document.querySelector('[data-lume-text="count"]'),
  button: document.querySelector('[data-lume-on="click:count.increment"]')
}

function renderCount() {
  nodes.countText.textContent = `Count: ${state.count}`
}

nodes.button.addEventListener("click", () => {
  state.count += 1
  renderCount()
})
```

実際の生成コードでは、querySelector の乱用を避け、初期化時に DOM 参照を一度だけ束縛する。

実装では `captureFocus()` / `restoreFocus()`、`navigate()`、`updateNavLinks()`、`callServerAction()`、`loadLumeWasm()`、`restoreInitialState()` などのヘルパーを含む。静的レンダーでは一部で `querySelector` を使うが、生成コードは `data-lume-id` と event binding を前提にしている。

---

### 31.3 CSS 出力

```css
.l-column {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
}
```

テーマトークンは CSS 変数として出力する。

```css
:root {
  --lume-color-primary: #4f46e5;
  --lume-space-md: 16px;
}
```

---

### 31.4 WASM 出力 [一部実装]

WASM ターゲットでは、状態遷移と差分計算を WASM に配置できる。

概念モデル。

```txt
JS event
  -> call wasm action
  -> wasm updates state
  -> wasm returns patch list
  -> JS applies patches to DOM
```

patch 形式。

```ts
type Patch =
  | { op: "setText"; id: number; value: string }
  | { op: "setAttr"; id: number; name: string; value: string }
  | { op: "insert"; parent: number; index: number; html: string }
  | { op: "remove"; id: number }
```

WASM ABI。

```txt
lume_init(memory_ptr, memory_len) -> void
lume_dispatch(event_id, payload_ptr, payload_len) -> patch_ptr
lume_free(ptr) -> void
```

---

### 31.5 Hydration

HTML は初期表示に使われる。

JavaScript は起動時に以下を行う。

1. `data-lume-id` を走査する
2. DOM 参照テーブルを作成する
3. 初期 state を manifest から復元する
4. event listener を接続する
5. 必要なら WASM を初期化する

実装では state は `lume.manifest.json` から復元され、フォーカスは `data-lume-focus-key` で簡易復元される。

---

### 31.6 Resumable 起動モデル [一部実装]

Lume は通常の hydration に加えて、Qwik 風の **resumable** 起動モデルを持てる。

resumable は「サーバーで生成した HTML をクライアントで即座に再実行して再構築する」のではなく、サーバー実行時に得られた UI の実行状態、DOM marker、event binding、遅延ロード可能な action symbol を HTML と manifest に直列化し、ブラウザでは必要な瞬間までコードを起動しないモデルである。

目的。

1. 初期ロード時の JavaScript 評価量を減らす
2. event handler をユーザー操作時まで遅延ロードする
3. SSR 済み DOM を破棄せず、そのまま実行可能状態として再開する
4. route、component、action 単位の細かい code splitting を可能にする
5. hydration mismatch を「再実行差分」ではなく「直列化境界の不一致」として診断する

resumable は SSR の上に成立する。CSR only の出力では resumable を有効にしても通常の lazy hydration と同等に扱う。

---

#### 31.6.1 activation mode

frontend の起動方式は `activation` で指定する。

```toml
[frontend]
routing = "spa"
activation = "resume"
hydration = "partial"
```

指定可能値。

```txt
hydrate: 起動時に対象 component を即時 hydration する
partial-hydrate: 可視性、idle、interaction に応じて部分 hydration する
resume: DOM と serialized state から実行を再開し、handler は操作時に読み込む
```

既定値は `hydrate` とする。`hydration = "partial"` は `activation = "partial-hydrate"` の旧別名として扱えるが、新しい仕様では `activation` を優先する。

---

#### 31.6.2 DOM marker

resumable 出力では、SSR HTML に resume 用 marker を付与する。

```html
<div
  data-lume-component="Counter"
  data-lume-id="c0"
  data-lume-r="b0"
>
  <span data-lume-id="n1" data-lume-bind="s0.count">Count: 0</span>
  <button
    data-lume-id="n2"
    data-lume-on="click:sym_counter_increment"
    data-lume-state="s0"
  >増やす</button>
</div>
```

marker の意味。

```txt
data-lume-r: resume boundary id
data-lume-id: DOM node id
data-lume-bind: state / derived binding id
data-lume-on: event type と action symbol の対応
data-lume-state: 参照する serialized state id
```

これらの属性は生成物の内部 ABI であり、Lume ソース上の public API ではない。

---

#### 31.6.3 serialized state

state は HTML 内の JSON script、または manifest 参照の外部 JSON として直列化する。

```html
<script type="application/lume-state" id="lume-state-s0">
{"count":0}
</script>
```

外部化する場合。

```json
{
  "serializedState": {
    "s0": {
      "url": "/assets/state/counter.s0.json",
      "hash": "sha256-..."
    }
  }
}
```

直列化可能な値は JSON 互換値、Lume primitive、record、array、enum-like object に限定する。関数、DOM node、opaque handle、FFI pointer、stream、AbortController、Promise は直列化できない。

直列化できない値を `state`、`derived memo`、event closure が捕捉する場合、コンパイラは resumable 不適合として診断する。

```txt
error[LUME1021]: value captured by resumable action is not serializable
```

---

#### 31.6.4 action symbol と lazy event

event handler は直接インライン化せず、symbol として分割する。

Lume。

```lume
component Counter {
  state count: Int = 0

  action increment() {
    count += 1
  }

  view {
    Button("増やす") {
      on click {
        increment()
      }
    }
  }
}
```

resumable 出力の概念。

```json
{
  "symbols": {
    "sym_counter_increment": {
      "chunk": "/assets/chunks/counter.increment.js",
      "captures": ["s0.count"],
      "boundary": "b0"
    }
  },
  "eventBindings": [
    {
      "node": "n2",
      "event": "click",
      "symbol": "sym_counter_increment",
      "state": "s0"
    }
  ]
}
```

ブラウザ runtime は初期化時に全 handler を import しない。最初の `click` で `sym_counter_increment` の chunk を読み込み、対応する serialized state を復元し、action を実行し、patch を DOM に適用する。

```txt
user event
  -> global delegated listener
  -> lookup data-lume-on
  -> load symbol chunk
  -> restore state scope
  -> run action
  -> apply DOM patch
  -> persist updated state scope
```

---

#### 31.6.5 resume boundary

resume boundary は、直列化、復元、chunk 分割、error isolation の単位である。

既定では route root、page、layout、component の stateful subtree が boundary 候補になる。コンパイラは以下を考慮して boundary を自動決定する。

1. state を持つ component
2. event handler を持つ subtree
3. server query の結果を参照する subtree
4. lazy route の root
5. ClientOnly の fallback 境界

明示 boundary 構文は v0.1 では導入しない。必要になった場合は将来 `resume boundary` ブロックまたは component modifier として追加する。

---

#### 31.6.6 resumable と Server Actions

Server Actions は resumable handler から呼び出せる。

```lume
Button("保存") {
  on click {
    await saveUser.mutate(form)
  }
}
```

この場合、client chunk には action 本体を含めず、既存の Server Action stub と action id のみを含める。秘密情報、DB 接続、server query 実装はクライアントへ直列化してはならない。

Server Action の結果で cache invalidation が発生した場合、runtime は該当 boundary の state scope を invalid にし、必要な query / HTML fragment / patch を再取得する。

---

#### 31.6.7 resumable と WASM

WASM ターゲットでは、action symbol の実体を JS chunk ではなく WASM export にできる。

```json
{
  "symbols": {
    "sym_counter_increment": {
      "wasmExport": "lume_sym_counter_increment",
      "captures": ["s0.count"],
      "boundary": "b0"
    }
  }
}
```

この場合も DOM 操作は JS runtime が行う。WASM は state 復元、action 実行、patch list 生成を担当できる。

```txt
event
  -> JS delegated listener
  -> ensure WASM initialized
  -> call wasm symbol
  -> receive patch list
  -> JS applies patches
```

---

#### 31.6.8 preload と prefetch

resumable runtime は、ユーザー操作の直前に必要な symbol を事前取得できる。

既定の prefetch trigger。

```txt
pointerover
focus
viewport enter
route intent
idle budget
```

prefetch はヒントであり、正しさに影響してはならない。prefetch できなかった場合も最初の event 時に chunk を取得して実行する。

---

#### 31.6.9 制約

resumable component では以下を禁止または警告する。

```txt
module top-level side effect
SSR と client で結果が変わる view 式
非直列化値の state 保存
event handler からの DOM 直接参照
handler closure に巨大 object を捕捉すること
server-only 値の client capture
```

代表的な診断。

```txt
LUME1020: resumable boundary cannot be inferred
LUME1021: captured value is not serializable
LUME1022: event handler captures server-only value
LUME1023: top-level side effect prevents resumability
LUME1024: resume marker mismatch
LUME1025: symbol chunk is missing from manifest
```

---

#### 31.6.10 通常 hydration との関係

`activation = "resume"` は hydration の完全な置き換えではない。以下の場合、runtime は boundary 単位で通常 hydration にフォールバックできる。

1. 直列化不能な legacy component を含む
2. dev server で詳細な runtime assertion を有効にしている
3. manifest と HTML の hash が一致しない
4. browser capability が不足している
5. userland adapter が resumable chunk を提供できない

フォールバック時は警告を出す。

```txt
warning[LUME1026]: boundary fell back to hydration
```

---

### 31.7 ルーティング出力

ルーティングは client-side router または static multi-page 出力に変換する。

```txt
spa: 1 つの index.html + JS router
mpa: route ごとに HTML を生成
hybrid: 静的 route は HTML、動的 route は JS router
```

現行実装は `frontend.routing` に応じて `spa` / `mpa` / `hybrid` を切り替え、`hybrid` では静的 route ごとの HTML と client router を併用する。`server` は dev server 側で route IR を使って解決する。`base_path` と `trailing_slash` は HTML 生成と dev 配信の両方に反映される。

---

## 32. バックエンド実行モデル

Lume のバックエンド実行モデルは **JIT** または **Native** に限定する。

ここでいうバックエンドとは、Server Actions、server query、FFI bridge、SSR、ビルド済みサーバー処理を実行する Lume ランタイムである。

```txt
jit
native
```

Node.js / Bun / Deno / Edge Worker は Lume の正式バックエンドではない。

それらの上で動かす adapter を外部実装することは可能だが、仕様上の実行モデルは JIT / Native の 2 つだけとする。

実装では dev server と native bridge がこのモデルに対応する。JIT は server action の一部でのみ使われ、native は FFI bridge を含むサーバー実行を担当する。

---

### 32.1 JIT backend

JIT backend は Lume IR を実行時に最適化・コンパイルして実行する。

用途。

1. 開発サーバー
2. ホットリロード
3. 動的 Server Actions
4. プラグイン実行
5. 高速な試行錯誤

入力。

```txt
.lume source
  -> AST
  -> Lume IR
  -> JIT compiled function
```

JIT backend はネイティブコード、WASM JIT、または VM 内部表現を使える。

---

### 32.2 Native backend

Native backend は Lume IR を事前コンパイルして、単体実行可能なサーバーバイナリまたは共有ライブラリを生成する。

用途。

1. 本番環境
2. Server Actions
3. SSR
4. FFI
5. 高負荷 API

出力例。

```txt
server
server.exe
liblume_server.so
liblume_server.dylib
lume_server.dll
```

---

### 32.3 Backend IR

Lume はサーバー側処理を Lume IR に変換する。

```txt
Server Actions
server query
validation
auth guard
transaction
FFI call
redirect
revalidate
```

IR は以下のどちらかへ渡される。

```txt
Lume IR -> JIT backend
Lume IR -> Native backend
```

---

### 32.4 Backend runtime manifest

```json
{
  "backend": {
    "mode": "native",
    "entry": "./server",
    "actions": "./lume.actions.json",
    "ffi": "./lume.ffi.json"
  }
}
```

JIT の場合。

```json
{
  "backend": {
    "mode": "jit",
    "entry": "./.lume/cache/server.ir",
    "watch": true
  }
}
```

---

### 32.5 SSR 仕様

### 32.5.1 SSR 安全性

`view` は同じ入力に対して同じ出力を生成しなければならない。

以下は SSR 不安定値として警告する。

```lume
Date.now()
Math.random()
window.innerWidth
document.cookie
localStorage.getItem("x")
```

### 32.5.2 client-only

```lume
ClientOnly fallback=Skeleton() {
  BrowserWidget()
}
```

### 32.5.3 server query

```lume
server query user = db.user.find(id)
```

サーバー専用 query はクライアントバンドルに含めてはならない。

---
