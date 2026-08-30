//! domain-crafting: receitas e validação pura.

pub mod apply;
pub mod construct;
pub mod recipe;
pub mod validate;

pub use apply::craft;
pub use construct::{can_construct, ShipConstructionJob};
pub use recipe::{Ingredient, Recipe, StationKind};
pub use validate::{can_craft, CraftError, InventoryView};
