# Lume v0.2

Lume v0.2 moves the MVP from single-file demos toward small real projects.

## Highlights

- Local `.lume` imports now validate exported items and reject private imports.
- Component scope names are resolved before type checking, so same-file and imported components are treated as known components.
- Imported `component`, `style`, and `theme` declarations are pulled into IR.
- Custom component views are expanded by HTML and JS codegen.
- Custom component props and default / named slots are supported in the expansion path.
- Routing Phase 1 adds route trees, nested/index routes, dynamic `:id` segments, catch-all `*path` segments, route `layout` / `guard` metadata, and an IR matcher.
- `App` is preferred as the entry component when a file contains multiple components.
- Diagnostics now include wider span carets and an error / warning summary.
- Added `examples/composed-card` for a multi-file component, prop, slot, and style workflow.

## Known v0.2 Limits

- Custom component child state is not isolated yet.
- `Outlet` rendering, router hooks, client navigation helpers, prefetch, SSR integration, and theme modes remain future work.
- Formatter is still indentation-oriented rather than fully AST-based.
- Server Actions, real FFI linking, SSR, Native execution, and JIT execution remain reserved for later versions.

## Specification Gaps

The following spec areas are described in the written specification but are not implemented yet in the current codebase:

| Area | Missing features |
| --- | --- |
| Routing | `Outlet` rendering, router hooks, client navigation helpers, prefetch, SSR integration, and browser/runtime navigation. |
| Data and queries | `query`, `mutation`, cache keys, `server query`, and client cache/runtime integration. |
| Server Actions | `auth`, `csrf`, `validate`, `rateLimit`, `transaction`, `revalidate`, streaming actions, and a full action manifest. |
| FFI | `ffi module` declarations, structs, enums, opaque types, callbacks, ownership/lifetime tracking, platform-specific loading, and FFI diagnostics. |
| Standard modules | `lume/std/asset`, `lume/std/i18n`, `lume/std/time`, `lume/std/result`, and the non-core parts of `lume/std/form`, `lume/std/query`, `lume/std/action`, `lume/std/a11y`, and `lume/std/router`. |
| UI components | `TextArea`, `Modal`, `Dialog`, `Tabs`, `Table`, `Container`, `Spacer`, and richer `Link` / `Form` semantics. |
| Accessibility | Heading-level checks, role/element validation, modal/dialog checks, and a broader a11y policy surface. |
| Style and theme | Inline `style` blocks, theme modes, and broader CSS lowering coverage beyond the current MVP subset. |
| Runtime and backends | Real Native execution, full JIT execution, WASM output, SSR, and DOM-diff / patch-based update models. |
| Tooling | A real LSP, AST-perfect formatter, and the fuller compiler/runtime packaging story from the spec. |

# Lume v0.1

Lume v0.1 is the first runnable MVP of the language and compiler.

## Highlights

- Rust workspace with compiler pipeline crates.
- `lume init`, `lume build`, `lume check`, `lume fmt`, and `lume dev`.
- `lume dev` runs watch build and serves `dist/` over HTTP.
- Lexer, parser, AST, HIR wrapper, type checks, IR, and HTML/CSS/JS codegen.
- Components with `state`, `view`, event handlers, `if`, and `for`.
- Standard UI/layout MVP: `Text`, `Button`, `Input`, `Image`, `Box`, `Row`, `Column`, `Grid`, `Stack`.
- Dynamic root render path for `if` / `for`.
- Loop item/index capture for events.
- Input focus/selection restoration across root rerenders.
- Theme tokens and style declarations lowered to CSS.
- State style and responsive style MVP.
- `lume.manifest.json` with state, routes, styles, and themes.
- Runtime initial state restoration from manifest.
- Basic accessibility and type diagnostics.
- LLVM/WASM/Native/JIT skeleton crates.
- Example projects under `examples/`.

## Known v0.1 Limits

- Dynamic rerendering uses root replacement rather than DOM diffing.
- Name resolution is intentionally shallow.
- Router support is manifest-level only.
- Formatter is stable/idempotent but not AST-perfect.
- Server Actions, real FFI linking, SSR, Native execution, and JIT execution are reserved for later versions.
