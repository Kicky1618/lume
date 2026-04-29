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
