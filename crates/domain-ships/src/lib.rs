//! domain-ships: tipos puros de navio e derivados de stats.

pub mod components;
pub mod definition;
pub mod stats;

pub use components::{EquippedComponent, EquippedComponents};
pub use definition::{ShipDefinition, ShipKind, SlotKind, SlotSpec};
pub use stats::{compute_ship_stats, ShipStats};
