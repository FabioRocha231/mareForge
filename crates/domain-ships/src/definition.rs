use mareforge_shared::ids::ShipDefinitionId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShipKind {
    SmallMerchant, // 3 tipos do vertical slice
    Patrol,
    Corsair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlotKind {
    Hull,
    Sail,
    Weapon,
    Aux,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotSpec {
    pub kind: SlotKind,
    pub accepts_tag: Option<String>, // tag opcional de ItemDefinition; None = aceita qualquer
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipDefinition {
    pub id: ShipDefinitionId,
    pub kind: ShipKind,
    pub display_name: String,
    pub slots: Vec<SlotSpec>,
    pub cargo_capacity: u32, // em unidades de peso
    pub base_speed: f32,     // m/s
    pub base_turn_rate: f32, // rad/s
    pub base_hp: u32,
    pub base_weapon_damage: u32,
    pub base_weapon_range: f32, // metros
}

impl ShipDefinition {
    pub fn slot_count(&self, kind: SlotKind) -> usize {
        self.slots.iter().filter(|s| s.kind == kind).count()
    }

    /// Definição provisória do SmallMerchant do vertical slice. Placeholder
    /// até o catálogo de navios existir (PRD MF-022); client e server usam
    /// esta mesma definição para que stats e movimento batam nos dois lados.
    pub fn small_merchant_placeholder() -> Self {
        Self {
            id: ShipDefinitionId::new(),
            kind: ShipKind::SmallMerchant,
            display_name: String::from("Small Merchant"),
            slots: vec![
                SlotSpec {
                    kind: SlotKind::Hull,
                    accepts_tag: None,
                },
                SlotSpec {
                    kind: SlotKind::Sail,
                    accepts_tag: None,
                },
                SlotSpec {
                    kind: SlotKind::Weapon,
                    accepts_tag: None,
                },
            ],
            cargo_capacity: 100,
            base_speed: 6.0,
            base_turn_rate: 1.0,
            base_hp: 100,
            base_weapon_damage: 20,
            base_weapon_range: 50.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> ShipDefinition {
        ShipDefinition {
            id: ShipDefinitionId::new(),
            kind: ShipKind::SmallMerchant,
            display_name: String::new(),
            slots: vec![
                SlotSpec {
                    kind: SlotKind::Hull,
                    accepts_tag: Some("hull".to_owned()),
                },
                SlotSpec {
                    kind: SlotKind::Hull,
                    accepts_tag: None,
                },
                SlotSpec {
                    kind: SlotKind::Sail,
                    accepts_tag: None,
                },
                SlotSpec {
                    kind: SlotKind::Weapon,
                    accepts_tag: None,
                },
            ],
            cargo_capacity: 100,
            base_speed: 5.0,
            base_turn_rate: 1.0,
            base_hp: 100,
            base_weapon_damage: 20,
            base_weapon_range: 50.0,
        }
    }

    #[test]
    fn slot_count_counts_each_kind() {
        let def = def();
        assert_eq!(def.slot_count(SlotKind::Hull), 2);
        assert_eq!(def.slot_count(SlotKind::Sail), 1);
        assert_eq!(def.slot_count(SlotKind::Weapon), 1);
        assert_eq!(def.slot_count(SlotKind::Aux), 0);
    }
}
