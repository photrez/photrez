# Bounded raster fixture

`bounded-raster.ptz` is assembled independently by `build_bounded_raster.py`
with Python's standard-library `struct`, `zlib`, and `zipfile`; it is not
produced by `SaveStream`. The script writes the expected RGBA bytes directly
from the literal pixel values `0c 22 38 ff c8 96 64 ff`; it does not decode the
PNG or derive expectations from production code. Nothing edits its output by
hand: when the document's own fields change, they change in the script and the
files are regenerated with it.

The archive contains `document.json` followed by `layers/raster-1.png`, both
Deflate level 6 with timestamp 1980-01-01 00:00:00. It has two Zip32 entries,
no ZIP64 records, no archive comment, and a conventional EOCD. The JSON
describes one visible, fully opaque, normal-blend raster with identity
transform and matching 2x1 document/raster dimensions. This fixture does not
establish support for other compositions, transforms, or blend modes.

The sidecars are the exact uncompressed entry bytes and expected canonical
RGBA. SHA-256:

| File | Bytes | SHA-256 |
|---|---:|---|
| `bounded-raster.document.json` | 362 | `391d54389c10365f774a53e5be7f054e2e2a8c4b3a1961694459376ab5029a2b` |
| `bounded-raster.layer.png` | 74 | `25aef9d34b1deb088f8a34eb8f56ad00de14e2c628866e952f7c2005d9dae36b` |
| `bounded-raster.rgba` | 8 | `8ba85dcaf96c47f964e5b6f6996fc1e2e1bc87d79c4318a757cae4ba5515a955` |
| `bounded-raster.ptz` | 526 | `4c313007acfc559d2806657789c7b4d2e21c36daf15b56b94c027f14f08e4f35` |

## Production-writer golden

`bounded-raster.save-stream.golden.ptz` is a pinned 533-byte output from the
production `SaveStream`; tests only read it and never regenerate it. Its
SHA-256 is
`e1f0d47d5ea3772af33969ac3f7fcc6f778924129e15d84c45d9ec7aca369386`.

It was re-recorded on 2026-11-05, when the fixture document's own id and display
name changed. The new bytes came from the production writer itself: a temporary
`#[ignore]`d test called the same `SaveStream` path the assertion uses and wrote
the bytes out, the dump was produced twice and compared byte-for-byte (identical,
and identical again after the sidecars were regenerated), and the temporary test
was then deleted. Only `document.json` inside the archive differs from the
previous golden: it grew from 345 to 362 bytes with those two strings, and
`layers/raster-1.png` is unchanged.

The first recording was made with a scratch Rust program outside the test path. The
program read the two sidecars above, created a `LocalFileIo` save target, and
called `SaveStream::begin`, `write_layer`, and `end`. It linked the production
library built from this checkout using Rust 1.95.0 and the lockfile that then
covered the crate (the pre-consolidation `crates/photrez/Cargo.lock`,
SHA-256 `f3b7a55c8d66e5ac6e48cf834df9d694a486008f015b6796b7e04384c1a9071d`;
that file no longer exists: the workspace consolidation replaced it with the
single root `Cargo.lock`). The locked feature tree was inspected with:

```text
cargo tree --locked -i zip@9.0.4 -e features
```

The manifest pins `zip = "=9.0.4"`, disables default features, and enables
`deflate`; the locked tree resolves `deflate-flate2-zlib-rs` and
`deflate-zopfli`. Production `file_options()` uses Deflate level 6, the DOS
system, external attributes `0o600 << 16`, and the ZIP epoch timestamp.

The exact one-off build, run, and copy commands were (the first command is
restated in today's form; the recorded run used the pre-consolidation
`--manifest-path crates/photrez/Cargo.toml` variant). Machine-local prefixes are
written as `<...>`: the recorded paths were under this machine's user profile and
temp directory, and an `rlib` filename suffix is a build hash that changes with
any rebuild, so neither carries evidence a reader can use.

```text
cargo test --locked --test services_save_stream --no-run -vv
& "<toolchain-bin>/rustc.exe" --edition=2021 --crate-name generate_save_stream_golden \
  "<scratch-dir>/src/main.rs" -o "<scratch-dir>/generate-save-stream-golden.exe" \
  -L "dependency=<repo>/target/debug/deps" \
  --extern "photrez=<repo>/target/debug/deps/libphotrez-<build-hash>.rlib" \
  -L "native=<repo>/target/debug/build/gpui-pre-<build-hash>/out" \
  -L "native=<registry-src>/windows_x86_64_gnu-0.52.6/lib" \
  -L "native=<registry-src>/windows_x86_64_gnu-0.53.1/lib"
& "<scratch-dir>/generate-save-stream-golden.exe"
Copy-Item -LiteralPath '<scratch-dir>/same-writer.golden.ptz' -Destination 'crates\photrez\tests\fixtures\bounded-raster.save-stream.golden.ptz'
```

The scratch program SHA-256 is
`0142cc5a9185769b04fc1443464a9d6d83a7a2187df082cae1385a2829f5c0eb`. The
independent Python archive and production output are compared by entry
names/order, fixed metadata (Deflate, DOS epoch, DOS system, external
attributes, no encryption, empty entry comments/extras), and exact uncompressed
payload bytes. Their compressed streams, compressed sizes, and offsets are not
compared because the backends differ. These fixture checks do not establish
production caller wiring or mounted file-dialog behavior.
