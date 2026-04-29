## 49. コンパイラ構成

### 45.1 Rust コンパイラパイプライン

```txt
source files
  -> lume_lexer
  -> lume_parser
  -> AST
  -> HIR lowering
  -> name resolver
  -> type checker
  -> accessibility checker
  -> SSR checker
  -> Lume IR
  -> optimization passes
  -> HTML/CSS/JS codegen
  -> LLVM codegen
  -> WASM object / native object
  -> linker
  -> dist output
```

役割。

```txt
AST: 構文木。ソース形状を保持する
HIR: 名前解決しやすい中間表現
Lume IR: UI / state / action / style を正規化した最適化用 IR
Backend IR: LLVM lowering 向け低レベル IR
LLVM IR: Inkwell で生成される LLVM module
```

### 45.2 Rust crate 構成

```txt
crates/
  lume_cli/
    src/main.rs
  lume_driver/
    src/lib.rs
    src/build.rs
    src/watch.rs
  lume_session/
    src/lib.rs
    src/config.rs
    src/files.rs
  lume_span/
    src/lib.rs
    src/span.rs
    src/source_map.rs
  lume_diagnostics/
    src/lib.rs
    src/error.rs
    src/emitter.rs
  lume_lexer/
    src/lib.rs
    src/token.rs
  lume_parser/
    src/lib.rs
    src/parser.rs
  lume_ast/
    src/lib.rs
    src/node.rs
  lume_hir/
    src/lib.rs
    src/lower.rs
  lume_resolver/
    src/lib.rs
  lume_typeck/
    src/lib.rs
    src/types.rs
    src/layout.rs
  lume_ir/
    src/lib.rs
    src/component.rs
    src/view.rs
    src/state.rs
  lume_opt/
    src/lib.rs
    src/passes/
  lume_codegen_html/
    src/lib.rs
  lume_codegen_css/
    src/lib.rs
  lume_codegen_js/
    src/lib.rs
  lume_codegen_llvm/
    src/lib.rs
    src/context.rs
    src/types.rs
    src/lower.rs
    src/passes.rs
  lume_codegen_wasm/
    src/lib.rs
  lume_backend_jit/
    src/lib.rs
  lume_backend_native/
    src/lib.rs
  lume_runtime_dom/
    src/lib.rs
  lume_runtime_server/
    src/lib.rs
  lume_ffi/
    src/lib.rs
  lume_lsp/
    src/lib.rs
  lume_formatter/
    src/lib.rs
```

---

## 50. 最小実装 MVP

v0.1 MVP は、Rust 製コンパイラで `.lume` を読み込み、React などを経由せずに HTML + CSS + JS を出力することを目標にする。

WASM / LLVM / Native backend は MVP では skeleton まで用意し、最初の実行可能成果物は HTML + CSS + JS とする。

### 46.1 MVP 実装対象

1. Rust CLI `lume`
2. `.lume` ファイル読み込み
3. lexer
4. parser
5. AST
6. HIR lowering
7. 最小 type checker
8. component
9. state
10. action
11. view
12. if
13. for
14. Text / Button / Column / Row / Box
15. HTML 出力
16. CSS 出力
17. JS runtime glue 出力
18. manifest 出力
19. 基本診断
20. `lume dev` の簡易 watch build
21. LLVM / Inkwell codegen crate の skeleton
22. WASM backend の skeleton

MVP では Server Actions、FFI、Native backend、JIT backend は構文予約と IR skeleton までに留める。

### 46.2 MVP 入力

`src/app.lume`

```lume
component App {
  state count: i32 = 0

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

### 46.3 MVP 出力ディレクトリ

```txt
dist/
  index.html
  assets/
    app.js
    style.css
    lume.manifest.json
```

WASM skeleton を有効にした場合。

```txt
dist/
  index.html
  assets/
    app.js
    style.css
    app.wasm
    lume.manifest.json
```

### 46.4 MVP HTML 出力

`dist/index.html`

```html
<!doctype html>
<html lang="ja">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Lume App</title>
    <link rel="stylesheet" href="/assets/style.css">
    <script type="module" src="/assets/app.js"></script>
  </head>
  <body>
    <div id="lume-root" data-lume-component="App" data-lume-id="c0">
      <div class="l-col l-s0" data-lume-id="n1">
        <span data-lume-id="n2" data-lume-bind="text:count">Count: 0</span>
        <button data-lume-id="n3" data-lume-event="click:0">増やす</button>
      </div>
    </div>
  </body>
</html>
```

### 46.5 MVP CSS 出力

`dist/assets/style.css`

```css
:root {
  --lume-space-12: 12px;
  --lume-space-16: 16px;
}

.l-col {
  display: flex;
  flex-direction: column;
}

.l-s0 {
  gap: var(--lume-space-12);
  padding: var(--lume-space-16);
}
```

### 46.6 MVP JS 出力

`dist/assets/app.js`

```js
const state = {
  count: 0
};

const root = document.getElementById("lume-root");
const nodes = {
  n2: root.querySelector('[data-lume-id="n2"]'),
  n3: root.querySelector('[data-lume-id="n3"]')
};

function render_count() {
  nodes.n2.textContent = `Count: ${state.count}`;
}

const actions = {
  0() {
    state.count += 1;
    render_count();
  }
};

root.addEventListener("click", event => {
  const target = event.target.closest("[data-lume-event]");
  if (!target) return;

  const eventSpec = target.getAttribute("data-lume-event");
  if (eventSpec === "click:0") {
    actions[0]();
  }
});
```

MVP では可読性を優先する。`max-runtime` では event id の数値化、DOM 参照の圧縮、minify を行う。

### 46.7 MVP Manifest 出力

`dist/assets/lume.manifest.json`

```json
{
  "version": "0.1",
  "entry": "src/app.lume",
  "target": "html-js-css",
  "components": [
    {
      "id": "c0",
      "name": "App",
      "state": [
        {
          "name": "count",
          "type": "i32",
          "initial": 0
        }
      ],
      "bindings": [
        {
          "node": "n2",
          "kind": "text",
          "dependsOn": ["count"]
        }
      ],
      "events": [
        {
          "id": 0,
          "node": "n3",
          "event": "click",
          "action": "increment_count"
        }
      ]
    }
  ],
  "assets": {
    "js": ["assets/app.js"],
    "css": ["assets/style.css"],
    "wasm": []
  }
}
```

### 46.8 MVP Rust CLI

```bash
cargo run -p lume_cli -- build examples/counter
```

通常利用。

```bash
lume build
lume dev
lume check
lume fmt
```

### 46.9 MVP Rust driver 概念

```rust
use lume_driver::{BuildOptions, Driver};

fn main() -> anyhow::Result<()> {
    let options = BuildOptions::from_file("lume.toml")?;
    let mut driver = Driver::new(options)?;
    let output = driver.build()?;

    for diagnostic in output.diagnostics() {
        eprintln!("{diagnostic}");
    }

    Ok(())
}
```

### 46.10 MVP LLVM / WASM skeleton

MVP 時点では `lume_codegen_llvm` と `lume_codegen_wasm` は以下を満たす。

```txt
Inkwell Context 初期化
target triple 設定
空 module 生成
runtime symbol 宣言
簡単な i32 add 関数の LLVM IR 生成テスト
wasm32 object 生成テスト
```

例。

```llvm
define i32 @lume_test_add(i32 %a, i32 %b) {
entry:
  %sum = add i32 %a, %b
  ret i32 %sum
}
```

この段階では UI state の WASM lowering は必須ではない。

---

## 51. 実例: ランキングページ

```lume
page RankingPage {
  state type: "xp" | "messages" | "voice" = "xp"

  query ranking key=["ranking", type] = api.get("/ranking?type={type}")

  view {
    Page title="ランキング" {
      SegmentedControl(value=type) {
        Option(value="xp", label="XP")
        Option(value="messages", label="メッセージ")
        Option(value="voice", label="通話")

        on change(value) {
          type = value
        }
      }

      if ranking.loading {
        SkeletonList(count=10)
      } else if ranking.error {
        ErrorView(error=ranking.error)
      } else {
        Table {
          column "順位"
          column "ユーザー"
          column "スコア"

          for user, index in ranking.data key=user.id {
            row {
              cell "#{index + 1}"
              cell {
                Row gap=8 align=center {
                  Avatar(src=user.image, alt=user.name)
                  Text(user.name)
                }
              }
              cell user.score
            }
          }
        }
      }
    }
  }
}
```

---

## 52. バージョニング

Lume は semver を採用する。

```txt
0.x: 実験段階。破壊的変更あり
1.x: 基本構文安定
2.x: 複数ターゲット安定
```

`.lume` ファイルでは任意で言語バージョンを指定できる。

```lume
language "0.1"
```

---

## 53. 将来拡張

候補。

1. Visual editor 用メタデータ
2. Native UI 出力
3. Partial hydration
4. Islands architecture
5. Design token import/export
6. Figma token 連携
7. Storybook 自動生成
8. Playwright テスト自動生成
9. WASM コンパイラ

---

## 54. まとめ

Lume は UI を以下の単位に分解する。

```txt
component: UI の再利用単位
state: 変更可能な値
derived: 計算値
action: イベント処理
effect: 副作用
view: UI 構造
style: 見た目
theme: デザイントークン
route: URL と page の対応
query: データ取得
```

最初に実装すべきは Rust 製コンパイラによる HTML + CSS + JS 出力の MVP である。

完成形を最初から追うと、たぶん GitHub に星が 4 つ付いて放置される。MVP は小さく、仕様は広く、実装は Rust workspace として段階的に進める。これが比較的まともな順序である。
