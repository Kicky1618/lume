## 19. スタイル

この章では、Lume の見た目をどう組み立てるかを扱う。`style` と `theme` を軸に、CSS へ落とし込まれる前の意図を明示する。

```txt
theme tokens
  -> style rules
  -> component props
  -> generated CSS
```

スタイルだけでなく、標準コンポーネント、アクセシビリティ、ルーティングまで含めて「UI を画面として成立させる層」をまとめて読むと分かりやすい。

Lume style は CSS の完全互換サブセットではなく、CSS に lowering される宣言 DSL である。

DSL には以下を含む。

1. token 参照
2. responsive ブロック (`at sm` など)
3. state style (`hover`, `active`, `disabled`)
4. semantic prop (`radius`, `columns`)

最終出力は常に CSS であり、DSL 固有機能はコンパイル時に CSS へ変換される。

実装状況としては、`style` 宣言、`theme` 宣言、`hover` / `active` / `disabled` の state style、`at sm|md|lg|xl` の responsive ブロック、`style=` 参照、基本的なトークン参照までは実装済みである。`inline style` ブロックや、より高度な semantic prop / cascade 制御は [未実装] として扱う。

### 19.1 インライン style

```lume
Box {
  style {
    padding: 16
    background: surface
    radius: md
  }
}
```

### 19.2 style 参照

```lume
Box style=card {
  Text("Hello")
}
```

### 19.3 style 宣言

```lume
style card {
  padding: md
  radius: lg
  background: surface
}
```

### 19.4 単位

数値だけの場合、プロパティに応じて既定単位を適用する。

```lume
padding: 16
```

は CSS 出力では通常 `16px` となる。

明示単位。

```lume
width: 100px
height: 50vh
fontSize: 1.2rem
```

### 19.5 状態スタイル

```lume
style button {
  background: primary

  hover {
    background: primaryHover
  }

  active {
    transform: scale(0.98)
  }

  disabled {
    opacity: 0.5
  }
}
```

### 19.6 レスポンシブ

```lume
style grid {
  columns: 1

  at md {
    columns: 2
  }

  at lg {
    columns: 3
  }
}
```

---

## 20. テーマ

### 20.1 theme 宣言

```lume
theme default {
  color primary = "#4f46e5"
  color primaryHover = "#4338ca"
  color surface = "#ffffff"
  color text = "#111827"
  color muted = "#6b7280"

  space xs = 4
  space sm = 8
  space md = 16
  space lg = 24

  radius sm = 6
  radius md = 12
  radius lg = 20

  breakpoint sm = 640
  breakpoint md = 768
  breakpoint lg = 1024
}
```

### 20.2 トークン参照

```lume
Text("Hello", color=text)
Box padding=md radius=lg
```

### 20.3 カラーモード

```lume
theme default {
  mode light {
    color surface = "#ffffff"
    color text = "#111827"
  }

  mode dark {
    color surface = "#0f172a"
    color text = "#f8fafc"
  }
}
```

### 20.4 CSS 変数出力

CSS ターゲットでは以下のような変数を生成する。

```css
:root {
  --lume-color-primary: #4f46e5;
  --lume-space-md: 16px;
}
```

---

## 21. 標準レイアウトコンポーネント

### 21.1 Box

任意のコンテナ。

```lume
Box padding=md {
  Text("Hello")
}
```

### 21.2 Row

横方向 flex。

```lume
Row gap=8 align=center justify=between {
  Text("A")
  Text("B")
}
```

### 21.3 Column

縦方向 flex。

```lume
Column gap=12 {
  Text("A")
  Text("B")
}
```

### 21.4 Stack

重ね合わせ。

```lume
Stack {
  Image(src=cover, alt="cover")
  Badge("New")
}
```

### 21.5 Grid

```lume
Grid columns=3 gap=16 {
  for item in items key=item.id {
    Card { Text(item.name) }
  }
}
```

---

## 22. 標準 UI コンポーネント

### 22.1 Text

```lume
Text("Hello", as="p")
```

属性。

```txt
as?: "span" | "p" | "label" | "strong" | "em" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
size?: String | Number
weight?: String | Number
color?: Color
align?: "start" | "center" | "end"
```

### 22.2 Button

```lume
Button("Save", variant="primary") {
  on click {
    save()
  }
}
```

Button は標準で keyboard accessible である。

### 22.3 Input

```lume
Input(value=name, placeholder="Name") {
  on input(value) {
    name = value
  }
}
```

### 22.4 Image

```lume
Image(src="/cat.png", alt="Scratch cat")
```

`alt` は必須である。

### 22.5 Router Link

`Link` は `lume/std/router` で定義するナビゲーション要素であり、`lume/std/ui` には含めない。

```lume
Link(to="/users/1") {
  Text("User")
}
```

### 22.6 Form

```lume
Form {
  on submit(event) {
    event.preventDefault()
    save()
  }

  Input(value=name)
  Button("Save", type="submit")
}
```

実装では `Form action=save method="post"` のような Server Action 直結フォームも扱う。`action` が相対 URL であれば通常の `<form action="...">` として、識別子であれば `"/__lume/actions/<name>"` へ変換する。

### 22.7 Canvas

`Canvas` は描画命令を書くための API ではなく、描画結果を表示するための surface である。

`Canvas` 自体は renderer を持たない。通常の UI tree の中に `<canvas>` 相当の描画領域を確保するだけである。

```lume
Canvas(
  width=800,
  height=600,
  ariaLabel="Preview"
)
```

`Canvas` は width / height / ariaLabel / class / style などの表示属性のみを受け取る。native bridge、WebGPU、WASM、CPU renderer への接続は `Canvas` ではなく、専用の派生要素で扱う。

#### 22.7.1 NativeCanvas

`NativeCanvas` は `CanvasSurface` を C / C++ FFI または native backend の描画関数へ渡すための要素である。

```lume
ffi module renderkit {
  language "c"
  sources ["./native/mandelbrot.c"]

  extern fn mandelbrot_render(
    surface: CanvasSurface,
    width: i32,
    height: i32,
    scale: f64
  ): StatusCode
}

component MandelbrotView() {
  state scale: f64 = 1.0

  view {
    NativeCanvas(
      width=800,
      height=600,
      renderer=renderkit.mandelbrot_render,
      args={
        width: 800,
        height: 600,
        scale
      },
      fallback="cpu"
    )
  }
}
```

`renderer` は `CanvasSurface` を第一引数に取る関数でなければならない。

`args` は文字列ではなく構造化された引数 object とし、各値は FFI 関数の型に対してコンパイル時に検査する。

`args` に含まれる state / props が変化した場合、runtime は対象 `NativeCanvas` の再描画をスケジュールする。

`CanvasSurface` は Lume runtime が所有する不透明型である。ユーザーコードは `CanvasSurface` を保存してはならない。描画関数の呼び出し中だけ有効な borrowed handle として扱う。

旧形式の `nativeModule`、`nativeSymbol`、`nativeArgs` は非推奨とする。`data-lume-native-*` 属性は runtime の内部表現であり、Lume ソース上の public API ではない。

#### 22.7.2 GpuCanvas

`GpuCanvas` は `lume/std/gpu` の GPU graph を表示先 surface に接続するための要素である。

WebGPU は最初の実装 target とする。ただし、Lume ソース上では WebGPU API を直接公開しない。

```lume
import { GpuCanvas } from "lume/std/gpu"

GpuCanvas(
  width=1280,
  height=720,
  graph=MandelbrotFrame,
  fallback="cpu"
)
```

`GpuCanvas` は `graph`、`width`、`height`、`fallback` を受け取る。`graph` は GPU pass の依存関係を持つ Lume GPU IR の値でなければならない。

WebGPU が利用できない環境では `fallback` に従う。`fallback` が未指定で WebGPU が利用できない場合は、runtime error ではなく、コンパイル時または起動時診断を出す。

#### 22.7.3 Canvas diagnostics

```txt
LUME5201: Canvas cannot use nativeModule/nativeSymbol/nativeArgs
LUME5202: NativeCanvas renderer must take CanvasSurface as first argument
LUME5203: NativeCanvas args do not match renderer signature
LUME5204: CanvasSurface cannot escape renderer call
LUME5205: GpuCanvas graph must be a GPU graph value
LUME5206: GpuCanvas requires gpu.render capability
```

---

## 23. アクセシビリティ

### 23.1 原則

Lume はアクセシビリティを標準仕様に含める。

### 23.2 必須診断

以下はコンパイルエラーまたは警告とする。

1. `Image` に `alt` がない
2. `Input` に label または aria-label がない
3. `Box` に click handler があるが keyboard handler がない
4. `Button` が空テキストかつ aria-label もない
5. heading レベルが不自然に飛ぶ
6. `role` と要素の組み合わせが不正
7. `Modal` にタイトルがない
8. `Dialog` に閉じる手段がない

実装状況としては、`Image` の `alt`、`Input` の `label` / `aria-label`、`Link` の URL 安全性、`Form` の action 検査、`Button` の空ラベル警告までは実装済みである。`Box` の keyboard handler 検査、heading/role の整合性、`Modal` / `Dialog` / `Landmark` 系の診断は [未実装]。

### 23.3 例

```lume
Image(src="/cat.png") // error: alt is required
```

```lume
Box {
  on click {
    submit()
  }
} // warning: use Button or add keyboard handler and role
```

### 23.4 明示的抑制

```lume
Image(src="/decorative.png", alt="", decorative)
```

---

## 24. ルーター

Lume はルーターを第一級機能として持つ。

ルーターは URL と `page` / `layout` / `server action` / `query` / asset chunk を結びつける。

React Router や Next.js Router へ変換するのではなく、Lume コンパイラが route manifest と client router runtime を生成する。

---

### 24.1 ルーターの出力方式

```txt
spa     1つの index.html と client router
mpa     route ごとに HTML を生成
hybrid  静的 route は MPA、動的 route は SPA fallback
server  Native / JIT backend が routing と SSR を担当
```

`lume.toml`。

```toml
[frontend]
routing = "hybrid"
base_path = "/"
trailing_slash = "never"

[frontend.router]
scroll_restoration = true
focus_main_on_navigation = true
```

現行実装は `spa` 相当の client-side router を生成し、`mpa` / `hybrid` の route ごとの HTML 分割や、`server` の専用 trie matcher は [未実装] である。`focus_main_on_navigation` も、現在は focus の再捕捉と復元に寄せた簡易実装である。

---

### 24.2 route 宣言

```lume
route "/" {
  HomePage()
}

route "/users/:id" {
  UserPage(id=params.id)
}
```

`params` は route pattern から推論される。

```txt
/users/:id -> params.id: String
```

---

### 24.3 page 宣言

```lume
page UserPage(id: String) {
  view {
    Text("User: {id}")
  }
}
```

`page` は route から直接呼び出される component である。

`page` は `query`、`server query`、`metadata`、`layout` を持てる。

---

### 24.4 layout / Outlet

```lume
layout AppLayout {
  view {
    Column {
      Header()
      Outlet()
      Footer()
    }
  }
}
```

route に適用する。

```lume
route "/" layout=AppLayout {
  HomePage()
}
```

nested route では layout が継承される。

---

### 24.5 nested routes

親 route に子 route がある場合、親 route 自身の page は明示的に `index` ブロックで定義する。

`index` がない場合、親 route は layout ノードのみを持ち、直接表示される page は存在しない。

```lume
route "/settings" layout=SettingsLayout {
  index {
    SettingsHomePage()
  }

  route "profile" {
    ProfilePage()
  }

  route "security" {
    SecurityPage()
  }
}
```

内部的には以下の route tree へ変換する。

```txt
/settings
  (index)
  /profile
  /security
```

nested route の子 path は相対 path とする。

---

### 24.6 dynamic segment

```lume
route "/users/:id" {
  UserPage(id=params.id)
}
```

型指定。

```lume
route "/posts/:id<i64>" {
  PostPage(id=params.id)
}
```

対応する型。

```txt
:id<String>
:id<i32>
:id<i64>
:slug<String>
```

型変換に失敗した場合は `notFound()` を返す。

実装では typed dynamic segment の構文は受け取るが、数値型への厳密なキャストや `notFound()` による分岐は [未実装] で、JS router は `String` か数値っぽい文字列かの簡易判定に留まる。

### 24.6.1 ルート衝突解決

同一 depth で複数 route が競合する場合、matcher 優先順位は以下とする。

```txt
1. static segment
2. typed dynamic segment (:id<i64>)
3. untyped dynamic segment (:id)
4. catch-all segment (*path)
```

例。

```txt
/users/new     -> static route
/users/:id     -> dynamic route
/users/*path   -> catch-all route
```

`/users/new` は常に static route を選ぶ。

---

### 24.7 catch-all segment

```lume
route "/docs/*path" {
  DocsPage(path=params.path)
}
```

`path` は `String[]` として扱う。

catch-all segment は route の最後にのみ置ける。

---

### 24.8 search params [未実装]

```lume
page SearchPage {
  search q: String = ""
  search page: i32 = 1

  view {
    Text("Query: {q}")
  }
}
```

`search` 宣言は URL query string と同期される。

```txt
/search?q=lume&page=2
```

---

### 24.9 navigation

標準 `Link` は router と統合される。

```lume
Link(to="/ranking", prefetch="hover") {
  Text("Ranking")
}
```

プログラム遷移。

```lume
action goUser(id: String) {
  navigate("/users/{id}")
}
```

`navigate` は `lume/std/router` の標準関数であり、Lume router runtime へ lowering される。

`NavLink` は現在の path と一致したときに `aria-current` と `is-active` を付与する。`Anchor` は `Link` と同様に扱う。

---

### 24.10 redirect / notFound [未実装]

```lume
page PrivatePage {
  guard auth.required

  view {
    Text("private")
  }
}
```

明示 redirect。

```lume
server query user = auth.currentUser()

if user == null {
  redirect("/login")
}
```

404。

```lume
if post == null {
  notFound()
}
```

`redirect` と `notFound` は `Never` を返す制御フローとして扱う。

---

### 24.11 route guard

```lume
guard requireLogin {
  if auth.user == null {
    redirect("/login")
  }
}

route "/dashboard" guard=requireLogin {
  DashboardPage()
}
```

実装では route guard の構文は manifest に残るが、guard 自体の実行や `redirect` / `notFound` の制御フローは [未実装] である。

複数 guard。

```lume
route "/admin" guard=[requireLogin, requireAdmin] {
  AdminPage()
}
```

---

### 24.12 loader / server query

```lume
page UserPage(id: String) {
  server query user = db.user.find(id)

  view {
    if user == null {
      notFound()
    } else {
      UserCard(user=user)
    }
  }
}
```

client-side query。

```lume
page RankingPage {
  query ranking = fetchJson<Ranking>("/api/ranking")

  view {
    RankingView(data=ranking.data)
  }
}
```

`query` は現在の compiler/runtime で client cache まで実装済みだが、`server query` の専用実行路は [未実装] で、現状は manifest と AST 情報として保持する段階である。

---

### 24.13 metadata [未実装]

```lume
page UserPage(id: String) {
  metadata {
    title: "User {id}"
    description: "User profile"
  }

  view {
    Text("User")
  }
}
```

metadata は HTML head または SSR head patch へ出力される。

実際の `lume.manifest.json` には、`version`、`component`、`target`、`backends`、`state`、`routes`、`routeTree`、`styles`、`themes`、`actions`、`queries`、`ffi`、`ffiStructs`、`ffiEnums`、`ffiOpaques` が出力される。

---

### 24.14 route manifest

コンパイラは route manifest を生成する。

```json
{
  "routes": [
    {
      "id": "home",
      "path": "/",
      "page": "HomePage",
      "layout": "AppLayout",
      "chunk": "home.js",
      "css": ["home.css"],
      "wasm": []
    },
    {
      "id": "user",
      "path": "/users/:id",
      "params": {
        "id": "String"
      },
      "page": "UserPage",
      "chunk": "user.js"
    }
  ]
}
```

---

### 24.15 route matcher [一部実装]

route matcher は以下のいずれかへ lowering される。

```txt
fast-build: JS router table
max-runtime without WASM: optimized JS trie matcher
max-runtime with WASM: WASM trie matcher
server: Native / JIT route matcher
```

内部表現。

```txt
RouteTrie
  static segment
  dynamic segment
  typed dynamic segment
  catch-all segment
```

---

### 24.16 prefetch [未実装]

```lume
Link(to="/users/{user.id}", prefetch="visible") {
  Text(user.name)
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

prefetch 対象。

```txt
HTML fragment
JS chunk
CSS chunk
WASM chunk
server query cache
```

---

### 24.17 scroll / focus 管理

router は遷移時に scroll と focus を管理する。

`main` landmark が存在しない場合は a11y warning を出す。

実装では focus の capture / restore は行うが、scroll restoration と `main` landmark 警告は [未実装]。

---

### 24.18 ルーター診断

```txt
LUME7001: duplicate route pattern
LUME7002: route parameter is not passed to page
LUME7003: page requires parameter missing from route
LUME7004: invalid route parameter type
LUME7005: catch-all segment must be last
LUME7006: redirect target does not match any known route
LUME7007: layout uses Outlet but no child route exists
LUME7008: nested route path must be relative
LUME7009: route guard returns non-Never value after redirect
LUME7010: route chunk exceeds budget
```

---
