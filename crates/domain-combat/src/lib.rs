//! domain-combat: primitivas puras de combate naval (PRD §18-§26). Sem
//! persistência, sem ECS, sem Bevy — o servidor conecta as peças.

pub mod ammo;
pub mod destruction;
pub mod loot;
pub mod naval;
pub mod projectile;
pub mod weapon;

pub use ammo::{sail_points, Ammo};
pub use destruction::{apply_damage, DamageOutcome};
pub use loot::{
    can_loot, is_expired, resolve_ship_destruction, DestructionOutcome, LootPolicy, SurvivorItem,
    WreckChest, WreckPolicy,
};
pub use naval::{
    arc_aim, boarding_chance, hit_zone, resolve_boarding, rudder_points, BoardingOutcome, HitZone,
    FIRING_ARC,
};
pub use projectile::{Projectile, WeaponParams};
pub use weapon::{BroadsideBattery, BroadsideSide};
