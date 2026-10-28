// SPDX-License-Identifier: AGPL-3.0-or-later
// Wire types that cross the host/engine boundary: the command envelope, the
// render projection/query DTOs, and the error shape. Crate-private; `protocol`
// re-exports the items the host is allowed to name.

pub(crate) mod command;
pub(crate) mod history;
pub(crate) mod projection;

#[cfg(test)]
mod tests;
