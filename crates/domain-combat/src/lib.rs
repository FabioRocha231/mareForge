//! domain-combat: primitivas puras de combate naval (PRD §18-§21). Sem
//! persistência, sem ECS, sem Bevy — o servidor conecta as peças.

pub mod destruction;
pub mod projectile;
pub mod weapon;

pub use destruction::{apply_damage, DamageOutcome};
pub use projectile::{Projectile, WeaponParams};
pub use weapon::{BroadsideBattery, BroadsideSide};
