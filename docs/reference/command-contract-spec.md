# Command contract

The command contract is owned by code, not by a separate specification:
`crates/core/src/wire/command.rs` declares every command the document accepts,
history records and replays them, and the tests beside the writer pin the
behaviour each command guarantees.

The specification document that previously collected these notes described the
desktop shell that shipped before the native runtime. It is not part of this
repository.
