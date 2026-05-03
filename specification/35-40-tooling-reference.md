## 35. 標準モジュール

Lume 標準機能は `lume/std/...` 名前空間で提供する。

標準モジュールは Lume コンパイラに同梱され、Rust 側で解決される。JavaScript package として配布しない。

実装では `Text`、`Button`、`Input`、`Image`、`Form`、`Link`、`NavLink`、`Outlet`、`Box`、`Row`、`Column`、`Grid`、`Stack`、`Canvas`、`NativeCanvas` が主要な内蔵要素として扱われる。`GpuCanvas`、`TextArea`、`Modal`、`Dialog`、`Tabs`、`Table`、`Spacer`、`Field`、`VisuallyHidden`、`FocusTrap`、`Landmark`、`query`、`invalidate`、`t`、`locale` などは [未実装]。

### 35.1 `lume/std/ui`

基本 UI コンポーネント。

```lume
import { Text, Button, Input, Image, Form, Anchor, Canvas, NativeCanvas } from "lume/std/ui"
```

### 35.2 `lume/std/layout`

レイアウトコンポーネント。

```lume
import { Box, Row, Column, Grid, Stack } from "lume/std/layout"
```

### 35.3 `lume/std/router`

ルーター機能。

```lume
import {
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

`Field` と `validate` は仕様上の目標であり、現行実装ではフォーム要素と Server Action 直結のみをサポートする。

### 35.5 `lume/std/action`

Server Action client stub と action result 型。

```lume
import { ActionResult, ActionError } from "lume/std/action"
```

`ActionResult` / `ActionError` は現在の runtime の JSON 形状に対応するための概念で、専用モジュールとしては [未実装]。

### 35.6 `lume/std/query`

client query と cache。

```lume
import { query, invalidate } from "lume/std/query"
```

`query` の cache 層は実装済みだが、`invalidate` の汎用 API はまだない。

### 35.7 `lume/std/ffi`

FFI 用型と補助定義。

```lume
import { Owned, Borrowed, View, Handle, Ptr, StatusCode, CanvasSurface } from "lume/std/ffi"
```

`CanvasSurface` は `NativeCanvas` renderer の第一引数として使う opaque borrowed handle である。保存、コピー、renderer 呼び出し外への escape は禁止する。

### 35.8 `lume/std/gpu`

GPU resource、shader、GPU graph、GpuCanvas。

```lume
import { GpuCanvas, gpu } from "lume/std/gpu"
```

`lume/std/gpu` は WebGPU を最初の実装 target とするが、標準モジュール自体は WebGPU API の薄い移植ではなく Lume GPU IR の公開 API とする。

現行実装では [未実装]。

### 35.9 `lume/std/a11y`

アクセシビリティ補助。

```lume
import { VisuallyHidden, FocusTrap, Landmark } from "lume/std/a11y"
```

`VisuallyHidden`、`FocusTrap`、`Landmark` のコンポーネント群は [未実装]。

### 35.10 `lume/std/asset`

asset 参照。

```lume
import logo from "asset:./logo.svg"
```

asset import は Lume compiler が処理する。JavaScript bundler の import ではない。

### 35.11 `lume/std/i18n`

国際化。

```lume
import { t, locale } from "lume/std/i18n"
```

### 35.12 標準モジュール診断

```txt
LUME6101: unknown standard module
LUME6102: standard module item is not exported
LUME6103: standard module requires WASM target
LUME6104: standard module requires native backend
LUME6105: standard module cannot be used in client context
LUME6106: asset import cannot be resolved
```

---

## 36. CLI

### 32.1 build

```bash
lume build
```

### 32.2 dev

```bash
lume dev
```

### 32.3 check

```bash
lume check
```

### 32.4 format

```bash
lume fmt
```

### 32.5 init

```bash
lume init
```

`lume init` は `lume.toml` と `src/app.lume` の雛形を作成する。

---

## 37. 設定ファイル

Lume の標準設定ファイルは `lume.toml` とする。

Rust コンパイラ本体から直接読み込めるよう、TOML を標準形式にする。`lume.config.ts` は標準仕様から除外する。

```toml
[project]
name = "scratchjp-ui"
entry = "src/app.lume"
out_dir = "dist"

[build]
target = "html-js-css-wasm"
profile = "fast-build"
source_map = "full"
minify = false
incremental = true

[frontend]
routing = "spa"
activation = "hydrate"
hydration = "partial"

[backend]
mode = "native"
target_triple = "x86_64-unknown-linux-gnu"
llvm = true

[backend.release]
opt_level = 3
lto = "thin"
strip = "symbols"
cpu = "generic"

[wasm]
enabled = true
target = "wasm32-unknown-unknown"
opt_level = "O1"

[diagnostics]
a11y = "error"
ssr = "warn"
optimization = "warn"
resumability = "warn"
```

`max-runtime` 用。

```toml
[build]
target = "html-js-css-wasm"
profile = "max-runtime"
source_map = "hidden"
minify = true
incremental = false

[wasm]
enabled = true
target = "wasm32-unknown-unknown"
opt_level = "O3"
wasm_opt = "-O3"

[backend]
mode = "native"
target_triple = "x86_64-unknown-linux-gnu"

[backend.release]
opt_level = 3
lto = "full"
strip = "all"
cpu = "generic"
```

resumable 起動を使う場合。

```toml
[frontend]
routing = "spa"
activation = "resume"
hydration = "partial"

[diagnostics]
resumability = "error"
```

環境ごとの差分は `lume.dev.toml` / `lume.release.toml` として分離できる。

```bash
lume build --config lume.release.toml
```

---

## 38. Rust API / 埋め込み

Lume コンパイラは Rust crate としても利用できる。

```rust
use lume_compiler::{BuildOptions, Compiler};

fn main() -> anyhow::Result<()> {
    let options = BuildOptions::from_toml_file("lume.toml")?;
    let mut compiler = Compiler::new(options)?;
    let result = compiler.build()?;

    for diagnostic in result.diagnostics {
        eprintln!("{diagnostic}");
    }

    Ok(())
}
```

開発サーバーも Rust 実装の `lume dev` が提供する。

```bash
lume dev
```

Vite plugin は標準仕様から除外する。必要な場合は外部 adapter として実装する。

---

## 39. エラー診断

### 39.1 形式

```txt
error[LUME1001]: Image requires alt text
  --> src/App.lume:12:5
   |
12 |     Image(src="/cat.png")
   |     ^^^^^^^^^^^^^^^^^^^^^ alt is missing
   |
help: add alt="..." or decorative
```

### 39.2 レベル

```txt
error
warning
info
hint
```

### 39.3 代表的エラーコード

```txt
LUME1001: Missing required attribute
LUME1002: Invalid prop type
LUME1003: Unknown component
LUME1004: Invalid state mutation
LUME1005: Missing key in loop
LUME1006: Invalid slot usage
LUME1007: SSR unsafe expression
LUME1008: Accessibility violation
LUME1009: Invalid route pattern
LUME1010: Unknown theme token
LUME1020: Resumable boundary cannot be inferred
LUME1021: Captured value is not serializable
LUME1022: Event handler captures server-only value
LUME1023: Top-level side effect prevents resumability
LUME1024: Resume marker mismatch
LUME1025: Symbol chunk is missing from manifest
LUME1026: Boundary fell back to hydration
```

---

## 40. フォーマット規則

標準 formatter は以下の形式を出力する。

```lume
component Example(title: String) {
  state count: Int = 0

  view {
    Column gap=12 {
      Text(title)

      Button("Increment") {
        on click {
          count += 1
        }
      }
    }
  }
}
```

規則。

1. インデントは 2 spaces
2. ブロック前に空白を入れる
3. component 内の `state`, `derived`, `action`, `view` は空行で区切る
4. 長い属性は複数行化する

---
