//! Destruição (PRD §21): `hp <= 0` → `ShipDestroyed`. A resolução de loot
//! (MF-013) vem depois; aqui morre só o que o PRD MF-010 pede.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageOutcome {
    Survived { remaining_hp: u32 },
    Destroyed,
}

/// Aplica dano ao casco e decide se afundou. Puro e determinístico.
pub fn apply_damage(current_hp: u32, damage: u32) -> DamageOutcome {
    if damage >= current_hp {
        DamageOutcome::Destroyed
    } else {
        DamageOutcome::Survived {
            remaining_hp: current_hp - damage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_reduces_hp() {
        assert_eq!(
            apply_damage(100, 30),
            DamageOutcome::Survived { remaining_hp: 70 }
        );
    }

    #[test]
    fn damage_equal_to_hp_destroys() {
        assert_eq!(apply_damage(100, 100), DamageOutcome::Destroyed);
    }

    #[test]
    fn damage_above_hp_destroys() {
        assert_eq!(apply_damage(50, 120), DamageOutcome::Destroyed);
    }

    #[test]
    fn zero_damage_survives() {
        assert_eq!(
            apply_damage(50, 0),
            DamageOutcome::Survived { remaining_hp: 50 }
        );
    }
}
