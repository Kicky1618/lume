# Lume example projects

Each directory is a tiny standalone Lume project. From the repository root, build one with:

```bash
cargo run -q -- build --entry examples/counter/src/app.lume --out-dir examples/counter/dist
```

Examples:

- `counter`: minimal state and click event.
- `input-preview`: input binding and focus-preserving dynamic render.
- `conditional-panel`: `if` / `else` with state toggles.
- `list-picker`: `for` loop with loop item captured by a click event.
- `theme-card`: `theme`, `style`, token lowering, and `style=card`.
- `gallery-grid`: `Grid`, `for`, and `Image`.
- `scoreboard`: multiple counters and conditional text.
- `form-state`: small form-like state preview.
- `composed-card`: local `.lume` import with exported component and style.
- `routing-tree`: nested routes, `index`, `layout`, `guard`, `:id`, and `*path` in one route manifest.
- `server-actions`: interpreted Server Actions, typed arguments, arrays, comparisons, and generated client stubs.
- `action-guardrails`: `mutation`, `validation`, `auth role=`, and `auth can=` on one page.
- `server-actions-jit`: numeric `runtime "jit"` Server Action compiled through the LLVM backend when available.
- `server-actions-metadata`: action modifiers such as `auth`, `csrf`, `invalidates`, `maxBodySize`, `rateLimit`, and `transaction` in the manifest.
- `ffi-lab`: FFI modules, structs, enums, opaques, callbacks, ownership, `free`, `throws`, and native bridge planning in one operational console.
- `ffi-platform`: `language "cpp"`, `namespace`, `abi`, and platform-specific native library targets, wired to an `ImageCanvas` hash preview backed by native RGBA bytes.
- `mega-workbench`: a large stress example combining theme/style tokens, composed components, many states, actions, loops, conditionals, Server Actions, WASM-enabled numeric updates, and resumable output.
