## 29. 低レベル拡張ポリシー

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
Manifest: ルート、Server Actions、assets、hydration 情報
```

現行の `lume.manifest.json` はこれに加えて `version`、`component`、`target`、`backends`、`routeTree`、`styles`、`themes`、`queries`、`ffi` 系の配列を含む。`lume.backend.json` も同時に生成され、server action と native bridge の実行情報を持つ。

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

### 31.4 WASM 出力 [未実装]

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

### 31.6 ルーティング出力

ルーティングは client-side router または static multi-page 出力に変換する。

```txt
spa: 1 つの index.html + JS router
mpa: route ごとに HTML を生成
hybrid: 静的 route は HTML、動的 route は JS router
```

現行実装は `spa` に寄せた client-side router を生成する。`mpa` と `hybrid` の route ごとの HTML 分割は [未実装]。

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
