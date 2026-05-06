## 36. CLI と設定

この章は、Lume の CLI、設定、Rust API、診断、整形をまとめた参照章である。標準モジュールは別章に分離した。

```txt
CLI
  -> config
  -> resolver
  -> formatter
  -> diagnostics
```

実装詳細よりも「どう使うか」に寄せて読めるよう、機能ごとに小さく分かれている。

---

### 36.1 build

```bash
lume build
```

### 36.2 dev

```bash
lume dev
```

### 36.3 check

```bash
lume check
```

### 36.4 format

```bash
lume fmt
```

### 36.5 init

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
