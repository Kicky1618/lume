## 44. セキュリティ

### 40.1 HTML injection

文字列は既定でエスケープされる。

```lume
Text(userInput)
```

### 40.2 unsafeHTML

```lume
UnsafeHTML(content=trustedHtml)
```

`UnsafeHTML` は警告対象とする。

### 40.3 URL 検証

`Link` / `Image` / `Script` などの URL は危険な scheme を警告する。

```lume
Link(to="javascript:alert(1)") // error
```

---

## 45. 国際化

### 41.1 t 関数

```lume
Text(t("home.title"))
```

### 41.2 辞書

```lume
i18n ja {
  "home.title" = "ホーム"
}

i18n en {
  "home.title" = "Home"
}
```

### 41.3 補間

```lume
Text(t("user.greeting", { name: user.name }))
```

---

## 46. アニメーション

### 42.1 transition

```lume
Box {
  transition {
    property: opacity
    duration: 150ms
    easing: easeOut
  }
}
```

### 42.2 animate

```lume
Box {
  animate enter {
    from { opacity: 0; y: 8 }
    to { opacity: 1; y: 0 }
  }
}
```

アニメーションは CSS animation / Web Animations API / Lume runtime animation 命令へ lowering できる。

---

## 47. テスト

### 43.1 test id

```lume
Button("Save", testId="save-button")
```

### 43.2 snapshot 安定性

コンパイラは SSR 非決定的式を検出し、snapshot の不安定化を警告する。

---

## 48. パッケージ仕様

Lume コンパイラ本体は Rust workspace として構成する。

アプリケーション側は `lume.toml` と `.lume` ソースを持つ。JavaScript package manager は必須ではない。

### 44.1 Lume アプリケーション構成

```txt
my-app/
  lume.toml
  src/
    app.lume
    routes.lume
    theme.lume
    components/
      Counter.lume
      UserCard.lume
    actions/
      post.lume
    ffi/
      image.lume
  native/
    image.c
    image.h
  public/
    favicon.svg
  dist/
```

### 44.2 Rust workspace 構成

```txt
lume/
  Cargo.toml
  crates/
    lume_cli/
    lume_driver/
    lume_session/
    lume_span/
    lume_diagnostics/
    lume_lexer/
    lume_parser/
    lume_ast/
    lume_hir/
    lume_resolver/
    lume_typeck/
    lume_ir/
    lume_opt/
    lume_codegen_html/
    lume_codegen_css/
    lume_codegen_js/
    lume_codegen_llvm/
    lume_codegen_wasm/
    lume_backend_jit/
    lume_backend_native/
    lume_runtime_dom/
    lume_runtime_server/
    lume_ffi/
    lume_lsp/
    lume_formatter/
  runtime/
    js/
      dom.js
      router.js
      actions.js
      wasm-bridge.js
    native/
      lume_runtime.c
      lume_runtime.h
  examples/
    counter/
    ranking/
```

### 44.3 Cargo.toml 例

```toml
[workspace]
resolver = "2"
members = [
  "crates/lume_cli",
  "crates/lume_driver",
  "crates/lume_session",
  "crates/lume_span",
  "crates/lume_diagnostics",
  "crates/lume_lexer",
  "crates/lume_parser",
  "crates/lume_ast",
  "crates/lume_hir",
  "crates/lume_resolver",
  "crates/lume_typeck",
  "crates/lume_ir",
  "crates/lume_opt",
  "crates/lume_codegen_html",
  "crates/lume_codegen_css",
  "crates/lume_codegen_js",
  "crates/lume_codegen_llvm",
  "crates/lume_codegen_wasm",
  "crates/lume_backend_jit",
  "crates/lume_backend_native",
  "crates/lume_runtime_dom",
  "crates/lume_runtime_server",
  "crates/lume_ffi",
  "crates/lume_lsp",
  "crates/lume_formatter"
]

[workspace.package]
edition = "2021"
license = "MIT OR Apache-2.0"
rust-version = "1.80"

[workspace.dependencies]
anyhow = "1"
thiserror = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
inkwell = { version = "0.8", features = ["llvm21-1"] }
```

LLVM のバージョンはビルド環境に依存するため、`inkwell` の feature はプロジェクトで固定する。

---

