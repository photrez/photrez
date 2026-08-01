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

The following commands are registered and implemented in `apps/desktop/src-tauri/src/main.rs`:

- `ping` (Bridge health check)
- `get_contract_info` (Version and command metadata)
- `read_file_bytes` (Base64 file import bridge, max 256 MB)
- `write_file_bytes` (Base64 file export bridge, max 256 MB)

Any addition or modification of IPC contract interfaces must update this registry.

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
4. Runtime `get_contract_info.supported_commands` exactly matches registered commands.
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
