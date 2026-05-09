# Framework benchmark harness

This tool compares Lume's build output against comparable Vite builds for React,
Vue, Svelte, and Solid across a set of UI feature cases.

```bash
cargo run -q -p framework_bench -- --runs 5
```

Useful options:

```bash
cargo run -q -p framework_bench -- --framework lume,react,vue --case counter,list
cargo run -q -p framework_bench -- --runtime bun --framework react,solid
cargo run -q -p framework_bench -- --runtime deno --case routing,gallery,theme
cargo run -q -p framework_bench -- --browser --framework lume --case counter --runs 3
cargo run -q -p framework_bench -- --runs 10 --warmups 2 --json --out benchmarks/framework-results.json
cargo run -q -p framework_bench -- --runs 10 --html --out benchmarks/framework-results.html
cargo run -q -p framework_bench -- --keep
```

Available cases:

- `counter`: state updates and event handlers.
- `list`: keyed list rendering and derived filtering.
- `form`: controlled inputs and validation-like derived state.
- `input`: multiple bound inputs and text interpolation.
- `conditional`: state-driven conditional branches and toggles.
- `card`: imported component composition, props, slots, and shared styles.
- `gallery`: grid layout with repeated image cards.
- `scoreboard`: paired counters with conditional status output.
- `theme`: theme tokens, style declarations, and CSS variable output.
- `routing`: multi-page route tree and navigation shell.
- `workbench`: larger composed UI with state, loops, conditionals, and actions.

The harness measures:

- Lume: local `lume build` elapsed time after building the CLI once.
- React/Vue/Svelte/Solid: generated Vite projects built with `npm`, `deno`, or
  `bun`, selected by `--runtime`.
- Bundle size: recursive byte size of emitted files where available.
- Browser runtime metrics with `--browser`: headless Chrome load time, repeated
  button/input interaction time, and DOM node count.

Node/npm, Deno, or Bun are only required for the JavaScript framework
comparisons when selected. If the selected runtime is not installed, the Lume
cases still run and the other frameworks are reported as skipped.
