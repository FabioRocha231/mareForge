use serde::{Deserialize, Serialize};

use crate::components::EquippedComponents;
use crate::definition::ShipDefinition;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipStats {
    pub speed: f32,
    pub turn_rate: f32,
    pub max_hp: u32,
    pub cargo_capacity: u32,
    pub weapon_damage: u32,
    pub weapon_range: f32,
}

pub fn compute_ship_stats(def: &ShipDefinition, equipped: &EquippedComponents) -> ShipStats {
    let mut damage: i32 = def.base_weapon_damage as i32;
    let mut speed_offset: f32 = 0.0;
    let mut cargo: i32 = def.cargo_capacity as i32;
    let mut hp: i32 = def.base_hp as i32;
    let mut range: f32 = def.base_weapon_range;

    for comp in equipped
        .hull
        .iter()
        .chain(equipped.sail.iter())
        .chain(equipped.weapon.iter())
        .chain(equipped.aux.iter())
    {
        damage += comp.damage_modifier;
        speed_offset += comp.speed_modifier as f32 * 0.01;
        cargo += comp.cargo_modifier;
        hp += comp.hp_modifier;
        range += comp.range_modifier as f32 * 0.01;
    }

    ShipStats {
        speed: (def.base_speed + speed_offset).max(0.0),
        turn_rate: def.base_turn_rate.max(0.0),
        max_hp: hp.max(0) as u32,
        cargo_capacity: cargo.max(0) as u32,
        weapon_damage: damage.max(0) as u32,
        weapon_range: range.max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use mareforge_shared::ids::{ItemDefinitionId, ShipDefinitionId};

    use super::*;
    use crate::components::EquippedComponent;
    use crate::definition::{ShipKind, SlotKind};

    fn def() -> ShipDefinition {
        ShipDefinition {
            id: ShipDefinitionId::new(),
            kind: ShipKind::SmallMerchant,
            display_name: String::new(),
            slots: Vec::new(),
            cargo_capacity: 100,
            base_speed: 5.0,
            base_turn_rate: 1.0,
            base_hp: 100,
            base_weapon_damage: 20,
            base_weapon_range: 50.0,
        }
    }

    fn component(damage_modifier: i32) -> EquippedComponent {
        EquippedComponent {
            slot_kind: SlotKind::Weapon,
            item_definition: ItemDefinitionId::new(),
            damage_modifier,
            speed_modifier: 0,
            cargo_modifier: 0,
            hp_modifier: 0,
            range_modifier: 0,
        }
    }

    #[test]
    fn no_components_returns_base_stats() {
        let stats = compute_ship_stats(&def(), &EquippedComponents::default());
        assert_eq!(stats.speed, 5.0);
        assert_eq!(stats.turn_rate, 1.0);
        assert_eq!(stats.max_hp, 100);
        assert_eq!(stats.cargo_capacity, 100);
        assert_eq!(stats.weapon_damage, 20);
        assert_eq!(stats.weapon_range, 50.0);
    }

    #[test]
    fn damage_modifier_adds_to_weapon_damage() {
        let equipped = EquippedComponents {
            weapon: vec![component(10)],
            ..EquippedComponents::default()
        };
        let stats = compute_ship_stats(&def(), &equipped);
        assert_eq!(stats.weapon_damage, 30);
    }

    #[test]
    fn damage_modifier_clamps_weapon_damage_at_zero() {
        let equipped = EquippedComponents {
            weapon: vec![component(-100)],
            ..EquippedComponents::default()
        };
        let stats = compute_ship_stats(&def(), &equipped);
        assert_eq!(stats.weapon_damage, 0);
    }

    #[test]
    fn speed_and_range_modifiers_apply_percent_offsets() {
        let equipped = EquippedComponents {
            sail: vec![EquippedComponent {
                slot_kind: SlotKind::Sail,
                item_definition: ItemDefinitionId::new(),
                damage_modifier: 0,
                speed_modifier: 100,
                cargo_modifier: 0,
                hp_modifier: 0,
                range_modifier: 100,
            }],
            ..EquippedComponents::default()
        };
        let stats = compute_ship_stats(&def(), &equipped);
        assert_eq!(stats.speed, 6.0);
        assert_eq!(stats.weapon_range, 51.0);
    }
}
