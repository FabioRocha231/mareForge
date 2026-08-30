//! domain-world: geografia econômica do mareForge (PRD §6-§10, §57). Regras
//! puras de mundo — regiões, zonas de risco, portos — sem ECS, sem Bevy.
//! O servidor conecta as peças; o client apenas representa (§10).

pub mod map;
pub mod region;
pub mod risk;
pub mod zone;

pub use map::{WorldError, WorldMap};
pub use region::{Port, Region};
pub use risk::{RiskPolicy, RiskTier};
pub use zone::{Zone, ZoneShape};
