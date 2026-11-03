// SPDX-License-Identifier: AGPL-3.0-or-later
// Document-runtime tests. These exercise `DocumentCore` end to end across
// command families, so they need the whole engine surface; the shared import
// block lives in the parent `document` module.

use super::*;

mod canvas;
mod commands;
mod contract;
mod contract_structured;
mod external;
mod external_size;
mod fuzz;
mod history_stream;
mod reorder;
mod reorder_history;
mod seed;
mod selection_commands;
mod snapshot;
mod structural;
