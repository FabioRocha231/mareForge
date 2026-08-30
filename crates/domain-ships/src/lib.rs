//! domain-ships: tipos puros de navio, stats derivados e movimento naval.

pub mod components;
pub mod definition;
pub mod loadout;
pub mod motion;
pub mod presence;
pub mod stats;

pub use components::{EquippedComponent, EquippedComponents};
pub use definition::{ShipDefinition, ShipKind, SlotSpec};
pub use loadout::{can_equip, LoadoutError, ShipLoadout};
pub use mareforge_domain_items::EquipmentSlot;
pub use motion::{step_motion, MotionInput, MotionTuning, ShipMotion};
pub use presence::{dock, undock, DockError, DockPolicy, VesselPresence};
pub use stats::{compute_ship_stats, ShipStats, StatsError};
