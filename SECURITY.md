# Security Policy

Photrez is a native Rust desktop application using GPUI. Its security boundary is the native window and event loop, local file I/O, and platform integrations. The product has no network listener or embedded browser.

## Supported Versions

| Version | Fixes |
| --- | --- |
| Latest release | Yes |
| `main` | Yes, best effort |
| Older releases | No, upgrade first |

A report against an older release still counts if the problem is present in
the latest release or on `main`.

## Supported Scope

Security reports are accepted for:

- The native window, event loop, dialogs, and OS integrations.
- File I/O and path handling for open, save, and export operations.
- Parsing untrusted `.ptz` project files.
- Decoding untrusted image files.
- Dependency and supply-chain concerns.
- Denial-of-service issues caused by crafted image files or project inputs.
- Violations of the native Rust `unsafe` policy. `unsafe_code = "deny"` is set for `crates/photrez`.

## Reporting a Vulnerability

Please report vulnerabilities privately before public disclosure. Use GitHub's
private vulnerability reporting: "Security" in this repository, then "Report a
vulnerability". Do not open a public issue with exploit details before a fix
or a mitigation is available.

Include:

- Reproduction steps.
- Affected commit, branch, or release.
- Impact assessment if known.
- Minimal sample files when safe to share privately.

Response targets:

- Initial acknowledgment: within 72 hours.
- Triage and severity classification: as soon as reproducible.
- Fix timeline: based on severity and release risk.

## Handling Guidelines

- Treat imported project and image files as untrusted input.
- Prefer fail-closed behavior for parser, decoder, and filesystem errors.
- Do not include exploit details in public issues before a fix or mitigation is available.
- Keep reports scoped and reproducible.
