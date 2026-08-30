//! domain-ships: tipos puros de navio, stats derivados e movimento naval.

pub mod components;
pub mod definition;
pub mod motion;
pub mod stats;

pub use components::{EquippedComponent, EquippedComponents};
pub use definition::{ShipDefinition, ShipKind, SlotKind, SlotSpec};
pub use motion::{step_motion, MotionInput, MotionTuning, ShipMotion};
pub use stats::{compute_ship_stats, ShipStats, StatsError};
