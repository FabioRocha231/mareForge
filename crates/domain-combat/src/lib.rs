//! domain-combat: primitivas puras de combate naval (PRD §18-§26). Sem
//! persistência, sem ECS, sem Bevy — o servidor conecta as peças.

pub mod destruction;
pub mod loot;
pub mod projectile;
pub mod weapon;

pub use destruction::{apply_damage, DamageOutcome};
pub use loot::{
    can_loot, is_expired, resolve_ship_destruction, DestructionOutcome, LootPolicy, SurvivorItem,
    WreckChest, WreckPolicy,
};
pub use projectile::{Projectile, WeaponParams};
pub use weapon::{BroadsideBattery, BroadsideSide};
