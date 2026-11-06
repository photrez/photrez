// SPDX-License-Identifier: AGPL-3.0-or-later
// Tests for the runtime engine: the `DocumentCore` command families exercised end
// to end, plus the state and accessor half that belongs to the engine's own state
// file. They need the whole engine surface, so the shared import block lives in the
// parent `engine` module.

use super::*;

mod canvas;
mod commands;
mod contract;
mod contract_structured;
mod external;
mod external_size;
mod flatten_layers;
mod fuzz;
mod history_stream;
mod projection;
mod reorder;
mod reorder_history;
mod replica;
mod seed;
mod selection_commands;
mod snapshot;
mod structural;
mod version;
