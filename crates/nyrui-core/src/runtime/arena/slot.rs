//! Internal slot storage representations in the generational arena.

use super::instance::InstanceNode;
use std::num::NonZeroU32;

/// Slot in the generational arena storage.
#[derive(Debug)]
pub(crate) enum ArenaSlot {
    Occupied {
        generation: NonZeroU32,
        node: InstanceNode,
    },
    Vacant {
        generation: NonZeroU32,
        next_free: Option<u32>,
    },
}
