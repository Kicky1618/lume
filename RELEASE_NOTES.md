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
