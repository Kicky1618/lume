# Lume

Lume is a UI language that compiles declarative UI into HTML, JavaScript, CSS, and optionally WebAssembly.

## Documentation

- Web docs: `docs/`
- Specification overview: `specification.md`
- Example gallery: `examples/README.md`

## Local preview

If you have `mdbook` installed, you can preview the documentation site with:

```bash
mdbook serve docs
```

The generated site is configured to build into `dist/docs`.

## Framework benchmarks

The repository includes a build benchmark harness that compares Lume with
generated Vite apps for React, Vue, Svelte, and Solid across counter, list,
form, input, conditional, card, gallery, scoreboard, theme, routing, and larger
workbench cases:

```bash
cargo run -q -p framework_bench -- --runs 5
cargo run -q -p framework_bench -- --browser --framework lume --case counter --runs 3
```

See `tools/framework_bench/README.md` for filters, browser runtime metrics, and
JSON output.
