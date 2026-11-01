# `.ptz` format migration note

The `.ptz` document format is pinned by the format tests beside the writer in
`crates/core/src/ptz/`. A format change must be additive and ship a migrator,
and the writer's output is guarded by a tracked golden fixture so an
unintended on-disk change fails a test rather than a release.

The dated migration working notes that accompanied the native rewrite were an
internal record of that rewrite; they are not part of this repository.
