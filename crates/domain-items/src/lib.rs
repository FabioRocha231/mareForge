//! domain-items: tipos puros de item. Sem persistência, sem ECS.

pub mod catalog;
pub mod definition;
pub mod instance;
pub mod stack;

pub use catalog::{CatalogError, ItemCatalog};
pub use definition::{EquipmentStats, ItemDefinition, ItemKind, Tag};
pub use instance::ItemInstance;
pub use stack::{remaining_capacity, split, try_merge, SplitError};
