// SPDX-License-Identifier: AGPL-3.0-or-later
// Model-owned identity types.

/// Token handle to a layer's pixel resource in the store registry. The engine
/// mints it from its own counter (`DocumentCore::next_resource`). Version 3
/// `.ptz` files carry no token, so the persisted field stays optional.
pub type ResourceId = u32;
