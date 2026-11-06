// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests for the `.ptz` file contract: the envelope round trip, the reader, the
//! writer and its tracked golden, the seed-payload validation, and the save-path
//! cost probe. Each one pins bytes, so they live with the writer they check.

mod file;
mod reader;
mod save_perf;
mod validate;
mod writer;
mod writer_golden;
