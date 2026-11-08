# Native Rust and GPUI Conventions

## State ownership

- crates/core owns canonical document state, pixels, and history.
- GPUI may own transient interface state; views and GPU buffers are derived projections.
- Do not introduce a second authoritative document store or history cursor.

## Rendering resources

Rendered images and GPU buffers are disposable display resources, not document state. Keep document mutations in photrez-core; let GPUI manage its rendering resources.

## Regression evidence

For a behavior change, add a focused regression check that fails against the previous behavior and passes with the change. Prefer the smallest runnable check that exercises the affected contract.

## Code comment hygiene

When adding or editing code comments, use the antislop-code skill as the detailed checklist.

- Keep comments that add non-obvious information: why, constraints, contracts, security, performance, accessibility, edge cases, legal facts, or measured provenance.
- Remove decorative separators, signature echoes, obvious restatements, workflow narration, empty labels, vague TODOs, and end markers.
- Prefer one short comment per meaningful block. Comment density is not a target by itself.
- Comment-only cleanup must not alter executable code, identifiers, imports, formatting, control flow, or logic. For non-trivial cleanup, prove the code token stream is unchanged with a string-aware comparison.
- Apply the rule to the owned scope; a wider audit needs explicit scope and its own evidence.

Preserve geometry tables, reference measurements, API contracts, known gaps, and other comments whose facts are not recoverable from the code alone.

## Public documentation

- Public documents use the set listed in `docs/README.md`. Do not add per-crate
  context files; use a crate README or the architecture document.
- A crate `README.md` is optional and short: purpose, owned types, non-goals,
  and the test command. It does not list the file tree.
- Do not hand-maintain module maps. Use rustdoc, `cargo metadata`, or
  `cargo tree`.
- When a public document moves or retires, update every reference in the same
  change.
- Only project documentation belongs in the tracked documentation set.

## File and test layout

- Name files for their domain. Avoid `utils`, `helpers`, `common`, `misc`,
  `manager`, and unqualified `engine`.
- Keep unit tests with their owner. Put integration tests in
  `crates/<package>/tests/` and test data under `tests/`.
- Prefer `foo.rs` plus `foo/` over `foo/mod.rs`.
- Do not use `#[path]` test indirection or root-level `*_tests.rs` files. Gate
  test-only modules with `#[cfg(test)]`.
- Existing violations are migration debt. Fix them when the file is touched or
  in a dedicated rename; do not spread them.
