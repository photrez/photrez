# photrez-format

The `.ptz` file contract. The package is workspace-internal (`publish = false`)
and depends only on `photrez-document`.

## Owns

- `file`: the `document.json` envelope, its writer and its reader — the format
  header, the session fields the loader restores, and the per-layer bitmap keys
  that must stay null.
- `validate`: the native seed-payload parse.
- `export`: the image payload codecs. They work on byte buffers and dimensions, so
  they name no document or pixel type.
- `fixtures`: the shared real `.ptz` documents, behind the `ptz-test-fixtures`
  feature so the shipped library does not carry fixture text.

## Does not own

- The content model it serializes. `photrez-document` owns that.
- Active document state, history, the loader workflow, or compositing. Those stay
  in `photrez-core` and the application.

Version 3 is the production writer. A format change must be additive and ship a
migrator; the writer's bytes are guarded by a tracked golden under
`tests/fixtures/`. See [`docs/ptz-migration.md`](../../docs/ptz-migration.md).

## Test

```text
cargo test -p photrez-format
```
