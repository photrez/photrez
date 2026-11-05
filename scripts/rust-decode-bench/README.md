# rust-decode-bench

A standalone benchmark, deliberately outside the repository workspace: its `Cargo.toml` declares an empty
`[workspace]` so the root profiles never apply to it.

Run it from this directory:

```
cargo run --release
```

Set `RUSTFLAGS=-C target-cpu=native` for machine-specific figures. Fixtures are generated in memory, written
once to a temporary file, then decoded repeatedly by each contender and reported as a median. The contenders
are `image`'s `load_from_memory` against the SIMD `zune-png` and `zune-jpeg` decoders, over the same bytes.

Nothing in the build or in CI calls it. It stays because a decoder comparison is otherwise impossible to
re-run here, and the numbers it produces belong to a report, not to the shipped code.
