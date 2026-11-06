# `.ptz` format migration note

The `.ptz` document format is pinned by the tests in `crates/format`, beside the
writer that produces it. A format change must be additive and ship a migrator,
and the writer's output is guarded by a tracked golden fixture
(`crates/format/tests/fixtures/ptzWriterGolden.json`) so an unintended on-disk
change fails a test rather than a release. Regenerate that golden deliberately
with `cargo test -p photrez-format --lib -- --ignored ptz_writer_golden`, and
stage it in the same change as the writer.

The dated migration working notes that accompanied the native rewrite were an
internal record of that rewrite; they are not part of this repository.
