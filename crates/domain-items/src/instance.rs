use mareforge_shared::ids::{ItemDefinitionId, ItemInstanceId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemInstance {
    pub id: ItemInstanceId,
    pub definition: ItemDefinitionId,
    pub quantity: u32,
    pub durability: Option<u16>, // presente apenas para Equipment
}

impl ItemInstance {
    pub fn new_resource(id: ItemInstanceId, def: ItemDefinitionId, quantity: u32) -> Self {
        Self {
            id,
            definition: def,
            quantity,
            durability: None,
        }
    }

    pub fn new_equipment(id: ItemInstanceId, def: ItemDefinitionId, durability: u16) -> Self {
        Self {
            id,
            definition: def,
            quantity: 1,
            durability: Some(durability),
        }
    }
}
