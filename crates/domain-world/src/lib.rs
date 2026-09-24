//! domain-world: geografia econômica do Marvyr (PRD §6-§10, §57). Regras
//! puras de mundo — regiões, zonas de risco, portos, nós de recurso — sem
//! ECS, sem Bevy. O servidor conecta as peças; o client apenas representa.

pub mod events;
pub mod features;
mod generate;
pub mod land;
pub mod map;
pub mod node;
pub mod portal;
pub mod region;
pub mod risk;
pub mod treasure;
pub mod zone;

pub use events::{DirectorChange, SeaEvent, SeaEventDirector, SeaEventKind};
pub use features::{Features, NodeSpot};
pub use land::{push_out_of_land, LandMass};
pub use map::{WorldError, WorldMap};
pub use node::{GatheringPolicy, ResourceNode};
pub use portal::{ClosedArena, FogArena, Portal, PortalDirector, PortalKind, PortalTuning};
pub use region::{Port, Region};
pub use risk::{RiskPolicy, RiskTier};
pub use treasure::HiddenIsland;
pub use zone::{Zone, ZoneShape};
