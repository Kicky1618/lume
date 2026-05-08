## 35. 標準モジュール

この章は、Lume の標準ライブラリをまとめた参照章である。日常的に触る `lume/std/...` 名前空間を一箇所で確認できるようにする。

```txt
standard module
  -> ui
  -> layout
  -> router
  -> form
  -> action
  -> query
  -> storage
  -> ffi
  -> gpu
  -> a11y
  -> asset
  -> i18n
```

実装詳細よりも「どう使うか」に寄せて読めるよう、機能ごとに小さく分かれている。

Lume 標準機能は `lume/std/...` 名前空間で提供する。

標準モジュールは Lume コンパイラに同梱され、Rust 側で解決される。JavaScript package として配布しない。

実装では `Text`、`Button`、`Input`、`TextArea`、`Image`、`Script`、`Form`、`Anchor`、`Canvas`、`ImageCanvas`、`NativeCanvas`、`Modal`、`Dialog`、`Tabs`、`Table`、`Box`、`Row`、`Column`、`Grid`、`Stack`、`Container`、`Spacer`、`Router`、`Route`、`Link`、`NavLink`、`Outlet`、`GpuCanvas`、`Field`、`VisuallyHidden`、`FocusTrap`、`Landmark`、`bytes`、`query`、`storage`、`invalidate`、`t`、`locale` などが主要な内蔵要素として扱われる。

### 35.1 `lume/std/ui`

基本 UI コンポーネント。

```lume
import { Text, Button, Input, TextArea, Image, Script, Form, Anchor, Canvas, ImageCanvas, NativeCanvas, Modal, Dialog, Tabs, Table, Spacer } from "lume/std/ui"
```

`Script` は外部 script 参照のみを扱う。

```lume
Script(src="/assets/widget.js", defer)
```

inline JavaScript や `raw js` ブロックは許可しない。

### 35.2 `lume/std/layout`

レイアウトコンポーネント。

```lume
import { Box, Row, Column, Grid, Stack, Container, Spacer } from "lume/std/layout"
```

### 35.3 `lume/std/router`

ルーター機能。

```lume
import {
  Router,
  Route,
  Link,
  NavLink,
  Outlet,
  navigate,
  redirect,
  notFound,
  prefetchRoute
} from "lume/std/router"
```

### 35.4 `lume/std/form`

フォーム、validation、FormData binding。

```lume
import { Form, Field, FormData, validate } from "lume/std/form"
```

`Field` は標準フォーム要素として扱う。`validate` は仕様上の目標であり、現行実装では専用 validation runtime はまだ持たない。

### 35.5 `lume/std/action`

Client Action の実行制御、Server Action client stub、action result 型。

```lume
import {
  ActionController,
  ActionResult,
  ActionError,
  ActionStatus,
  callAction,
  useAction
} from "lume/std/action"
```

Client Action では `async action ... concurrency=enqueue|drop|restart` を扱う。既定は `enqueue` で、生成 JS runtime は action ごとの逐次 queue、実行中 drop、restart の latest-wins 管理を行う。

Server Action は通常の関数呼び出しに加えて controller としても使える。

```lume
Button("保存") {
  on click {
    await savePost.mutate(input)
  }
}
```

生成 runtime は `ActionController`、`ActionResult`、`ActionError`、`ActionStatus`、`callAction`、`useAction` を client 側に用意し、server 側は同じ JSON wire format に `value`、`runtime`、`revalidate`、構造化 error を載せる。

### 35.6 `lume/std/query`

client query と cache。

```lume
import { query, invalidate } from "lume/std/query"
```

`query` の cache 層は実装済みだが、`invalidate` の汎用 API はまだない。

### 35.7 `lume/std/storage`

永続ストレージの共通抽象。

```txt
Web target
  -> IndexedDB

Server target
  -> NoSQL database
```

`lume/std/storage` は、Web では IndexedDB を永続層として使い、サーバーでは NoSQL データベースを永続層として使う標準モジュールである。アプリケーション側は同じ永続アクセスの形を使い、target ごとの差分は compiler/runtime が吸収する。

`query` が fetch/cache の一時的な結果を扱うのに対して、`storage` はオフライン保存やサーバー永続化のような長期保存を対象とする。SQL 固有の join を前提にせず、key-value / document / collection 系のデータモデルを想定する。

### 35.8 `lume/std/bytes`

`Bytes` と `String` の境界を扱う標準補助。

```lume
import { bytes } from "lume/std/bytes"

let digest = bytes.hex(await nativehash.hash(input))
let text = bytes.utf8(payload)
```

`bytes.hex(value)` は `Bytes` / `Uint8Array` 相当の値を小文字 hex 文字列へ変換する。`bytes.utf8(value)` は UTF-8 として decode する。FFI の `Owned<Bytes>` を直接 UI に出すのではなく、digest、payload preview、protocol message などの用途に応じて明示的に `String` へ変換する。

### 35.9 `lume/std/ffi`

FFI 用型と補助定義。

```lume
import { Owned, Borrowed, View, Handle, Ptr, StatusCode, CanvasSurface } from "lume/std/ffi"
```

`CanvasSurface` は `NativeCanvas` renderer の第一引数として使う opaque borrowed handle である。保存、コピー、renderer 呼び出し外への escape は禁止する。

### 35.10 `lume/std/image`

FFI や server action から返された RGBA image buffer を canvas surface に表示する補助。

```lume
import { ImageCanvas, RgbaImage } from "lume/std/image"
```

`ImageCanvas` の `renderer` は `Owned<Bytes>` として `width * height * 4` bytes の RGBA8 を返す関数を受け取る。`NativeCanvas` が native surface へ直接描画する renderer を表すのに対して、`ImageCanvas` は byte image を UI canvas に転送する renderer を明示する。

### 35.11 `lume/std/gpu`

GPU resource、shader、GPU graph、GpuCanvas。

```lume
import { GpuCanvas, gpu } from "lume/std/gpu"
```

`lume/std/gpu` は WebGPU を最初の実装 target とするが、標準モジュール自体は WebGPU API の薄い移植ではなく Lume GPU IR の公開 API とする。

現行実装では `GpuCanvas` 要素を HTML 出力に反映する。GPU graph の実行 runtime は段階的に拡張する。

### 35.12 `lume/std/a11y`

アクセシビリティ補助。

```lume
import { VisuallyHidden, FocusTrap, Landmark } from "lume/std/a11y"
```

`VisuallyHidden`、`FocusTrap`、`Landmark` は標準要素として HTML / CSS 出力に反映する。

### 35.11 `lume/std/asset`

asset 参照。

```lume
import logo from "asset:./logo.svg"
```

asset import は Lume compiler が処理する。JavaScript bundler の import ではない。

### 35.12 `lume/std/i18n`

国際化。

```lume
import { t, locale } from "lume/std/i18n"
```

### 35.13 標準モジュール診断

```txt
LUME6101: unknown standard module
LUME6102: standard module item is not exported
LUME6103: standard module requires WASM target
LUME6104: standard module requires native backend
LUME6105: standard module cannot be used in client context
LUME6106: asset import cannot be resolved
```
