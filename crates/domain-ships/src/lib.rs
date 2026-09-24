//! domain-ships: tipos puros de navio, stats derivados e movimento naval.

pub mod components;
pub mod crew;
pub mod definition;
pub mod loadout;
pub mod motion;
pub mod presence;
pub mod sailing;
pub mod stats;
pub mod weather;

pub use components::{EquippedComponent, EquippedComponents};
pub use crew::{
    casualties, crew_capacity, reload_multiplier, repair_step, rudder_turn_multiplier, RepairStep,
    CREW_WAGE, REPAIR_COMBAT_LOCK_SECS, RUDDER_HP_MAX, SKELETON_CREW,
};
pub use definition::{ShipDefinition, ShipKind, SlotSpec};
pub use loadout::{can_equip, LoadoutError, ShipLoadout};
pub use marvyr_domain_items::EquipmentSlot;
pub use motion::{step_motion, MotionInput, MotionTuning, ShipMotion};
pub use presence::{dock, undock, DockError, DockPolicy, VesselPresence};
pub use sailing::{
    angle_off_wind, point_of_sail, polar_factor, sail_speed_multiplier, wind_speed_factor,
    PointOfSail, Wind, SAIL_HP_MAX,
};
pub use stats::{compute_ship_stats, ShipStats, StatsError};
pub use weather::{Storm, Weather, WeatherBounds};
