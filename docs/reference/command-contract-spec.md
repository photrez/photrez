# 15 - Command Contract Specification (Tauri Shell Runtime)

This file defines the authoritative IPC contract currently exposed by the Tauri shell runtime.

Historical editor commands are implemented in the TypeScript MVP editor hot path, not registered as Tauri commands in the current runtime.

## 1) Contract Metadata

- Contract name: `photrez-command-contract`
- Current version: `2.0.0`
- Transport: Tauri command invoke (request/response)
- Encoding: JSON

## 2) Canonical Response Envelope

All command responses MUST follow exactly one of these envelopes.

Success:

```json
{
  "ok": true,
  "contract_version": "2.0.0",
  "data": {}
}
```

Error:

```json
{
  "ok": false,
  "contract_version": "2.0.0",
  "error": {
    "code": "E_VALIDATION",
    "message": "Human-readable error message",
    "details": {}
  }
}
```

Rules:

- `ok` is required.
- `contract_version` is required in both success and error.
- `error.details` is optional and must be JSON-serializable.
- No mixed envelope (`ok: true` with `error`, or `ok: false` with `data`).

## 3) Standard Error Codes

| Code | Meaning | Typical HTTP Analogy |
| --- | --- | --- |
| `E_VALIDATION` | Payload shape/value invalid | 400 |
| `E_NOT_FOUND` | Target resource/document/layer not found | 404 |
| `E_CONFLICT` | State conflict (version mismatch/invalid order) | 409 |
| `E_UNSUPPORTED` | Feature/operation not supported in MVP | 422 |
| `E_RESOURCE_LIMIT` | Memory/dimension/size guardrail exceeded | 413 |
| `E_IO` | File read/write failure | 500 |
| `E_INTERNAL` | Unexpected internal failure | 500 |
| `E_TIMEOUT` | Operation timed out | 504 |

Notes:

- Keep `message` concise and user-safe.
- Internal diagnostic detail should go into logs, not sensitive response text.

## 4) Versioning Rules

- Patch (`2.0.x`): non-breaking changes (new optional fields, better messages).
- Minor (`2.x.0`): additive backward-compatible command/data expansion.
- Major (`x.0.0`): breaking schema/envelope semantics.
- Any breaking change requires ADR update and migration note.

## 5) Request Model Baseline

For command payloads, use deterministic field names and explicit IDs.

Conventions:

- IDs are string-based (`doc_id`, `layer_id`).
- Coordinates are numeric pixels (`x`, `y`, `width`, `height`).
- Angles in degrees (`rotate_deg`).
- Opacity in range `[0..1]`.
- Booleans are explicit (`flip_x`, `flip_y`, `preserve_aspect`).

## 6) Runtime Commands

### 6.1 `ping`

Purpose: bridge health check.

Request:

```json
{}
```

Success `data`:

```json
{
  "status": "ok",
  "service": "native"
}
```

### 6.2 `get_contract_info`

Purpose: expose contract metadata to shell/tests.

Request:

```json
{}
```

Success `data`:

```json
{
  "name": "photrez-command-contract",
  "version": "2.0.0",
  "supported_commands": ["ping", "get_contract_info", "read_file_bytes", "write_file_bytes"]
}
```

### 6.3 `read_file_bytes`

Purpose: read a dialog/drop-provided local file path and return base64-encoded bytes to the frontend.

Path policy:

- Supported import extensions: `.png`, `.jpg`, `.jpeg`, `.webp`, `.gif`, `.bmp`, `.tif`, `.tiff`.
- Extension checks are case-insensitive and run before filesystem metadata/read.

Request:

```json
{ "path": "C:\\Users\\Example\\Pictures\\image.png" }
```

Success `data`:

```json
{
  "path": "C:\\Users\\Example\\Pictures\\image.png",
  "size": 1234,
  "data": "base64-encoded-bytes"
}
```

Failure:

- `E_VALIDATION` when the path extension is not supported for import.
- `E_IO` when metadata/read fails.
- `E_RESOURCE_LIMIT` when file size exceeds 256 MB.

### 6.4 `write_file_bytes`

Purpose: write base64-encoded bytes to a local path selected by the native save dialog.

Path policy:

- Supported export extensions: `.png`, `.jpg`, `.jpeg`, `.webp`.
- Extension checks are case-insensitive and run before base64 decode/write.

Request:

```json
{
  "path": "C:\\Users\\Example\\Pictures\\export.png",
  "data": "base64-encoded-bytes"
}
```

Success `data`:

```json
{
  "path": "C:\\Users\\Example\\Pictures\\export.png",
  "size": 1234
}
```

Failure:

- `E_VALIDATION` when the path extension is not supported for export.
- `E_VALIDATION` when `data` is not valid base64.
- `E_RESOURCE_LIMIT` when decoded bytes exceed 256 MB.
- `E_IO` when write fails.

## 7) Implemented & Registered IPC Commands

The following commands are registered in `apps/desktop/src-tauri/src/main.rs` (`tauri::generate_handler!`).

### 7.1 Public contract commands (frontend bridge)

These are the commands exposed via `get_contract_info.supported_commands` (see `PUBLIC_CONTRACT_COMMANDS` in `src/file_io.rs`). The frontend shell treats this set as the stable bridge contract:

- `ping` (Bridge health check)
- `get_contract_info` (Version and command metadata)
- `read_file_bytes` (Base64 file import bridge, max 256 MB)
- `write_file_bytes` (Base64 file export bridge, max 256 MB)
- `save_project` (ZIP project save)
- `load_project` (ZIP project load)
- `print_image` (Print dispatch)
- `get_system_printers` (Printer enumeration)
- `open_printer_properties` (Printer properties dialog)

### 7.2 Internal shell commands (not part of the public contract)

Registered and implemented, but NOT listed in `supported_commands` — they are internal to the desktop shell and may change without a contract bump:

- `get_pending_open_path`, `set_trusted_paths`, `save_project_binary`
- `save_project_streaming_begin`, `save_project_streaming_write_layer`, `save_project_streaming_end`, `save_project_streaming_cancel`
- `print_image_raw`, `get_printer_paper_sizes`
- `get_print_settings`, `set_paper`, `toggle_orientation`, `set_orientation`, `set_margin`, `set_per_side_margins`, `set_scale_to_fit`, `set_scale_percent`, `set_center_image`, `set_top_offset_mm`, `set_left_offset_mm`, `set_copies`, `set_unit`, `set_show_paper_white`, `set_color_handling`, `set_rendering_intent`, `set_black_point_compensation`, `set_printer`, `open_printer_properties_and_apply`, `convert_mm_to_current_unit`, `convert_current_unit_to_mm`
- `set_native_cursor`, `delete_file`, `delete_autosave_file`, `close_app`

Any addition or modification of the PUBLIC contract (7.1) must update this registry AND `PUBLIC_CONTRACT_COMMANDS` in `src/file_io.rs`. Internal commands (7.2) only need the registry above.

## 8) Example Error Cases

### Invalid base64 payload

```json
{
  "ok": false,
  "contract_version": "2.0.0",
  "error": {
    "code": "E_VALIDATION",
    "message": "Invalid base64: Invalid byte 45, offset 3.",
    "details": null
  }
}
```

### File too large for IPC transfer

```json
{
  "ok": false,
  "contract_version": "2.0.0",
  "error": {
    "code": "E_RESOURCE_LIMIT",
    "message": "File is too large for IPC transfer; max supported size is 256 MB",
    "details": null
  }
}
```

## 9) Contract Test Minimum

At minimum, contract tests must verify:

1. Envelope shape for success and error.
2. `contract_version` presence in all responses.
3. Deterministic `E_VALIDATION` on malformed payload.
4. Runtime `get_contract_info.supported_commands` exactly matches the public contract registry in §7.1 (enforced by `test_get_contract_info_includes_write_command` in `src/file_io.rs`).
5. No panic/uncaught failure leaks to shell.

## 10) Ownership and Change Control

- Primary owner: Core + Shell maintainers.
- Any change to envelope, versioning semantics, or error code set must update:
1. `docs/spec/trd.md`
2. Contract tests and evidence in milestone report.

## 11) Contract Version Bump Rules

- `contract_version` is **independent** of the app version — a SemVer pre-release suffix (`-alpha.N`, `-beta.N`, `-rc.N`) never affects it.
- Bump MAJOR for: envelope shape changes, command removal/rename, error-code set changes, breaking payload semantics.
- Bump MINOR for: additive commands, optional payload fields, new error codes that do not remap existing ones.
- **Frozen during each beta cycle**: once a release enters beta, the contract is locked for that release; subsequent `-beta.N` / `-rc.N` app versions keep the same contract version.
- Alpha stages may bump freely. Every bump must update `docs/spec/trd.md` (envelope section) and this spec, and §9.4 (`supported_commands` == registered commands) must stay green.
