# Render and export coverage

Composited output is pinned by the golden comparisons in `crates/render/src/compositor/tests/goldens.rs`.
Encoded bytes are pinned by the writer comparisons in `crates/format/src/tests/writer_golden.rs` and the
export behaviour tests in `crates/format/src/export/tests.rs`.

```
cargo test -p photrez-render --lib compositor::tests::goldens
cargo test -p photrez-format --lib tests::writer_golden
cargo test -p photrez-format --lib export::tests
```

Each command selects one module, so the result is small enough to read. Checked
2026-11-06: the first selects 4 tests, the second selects 1 test and 1 ignored, the third
selects 14. Run them from the workspace root.
