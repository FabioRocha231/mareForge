//! Munição (MF-059): bala redonda afunda, corrente aleija. A escolha é do
//! jogador; o servidor guarda a munição carregada por navio.

use serde::{Deserialize, Serialize};

use crate::projectile::WeaponParams;

/// Bala redonda: fração do dano bruto que vai ao casco / ao pano.
pub const HULL_SHARE: f32 = 0.8;
pub const SAIL_SHARE: f32 = 0.2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ammo {
    /// Bala redonda: casco.
    #[default]
    Round,
    /// Bala de corrente: rasga velas, pouco casco, curta e lenta.
    Chain,
}

impl Ammo {
    /// Tecla C: próxima munição.
    pub fn next(self) -> Self {
        match self {
            Ammo::Round => Ammo::Chain,
            Ammo::Chain => Ammo::Round,
        }
    }

    pub fn hull_factor(self) -> f32 {
        match self {
            Ammo::Round => 1.0,
            Ammo::Chain => 0.3,
        }
    }

    pub fn sail_factor(self) -> f32 {
        match self {
            Ammo::Round => 1.0,
            Ammo::Chain => 4.0,
        }
    }

    pub fn range_factor(self) -> f32 {
        match self {
            Ammo::Round => 1.0,
            Ammo::Chain => 0.6,
        }
    }

    pub fn speed_factor(self) -> f32 {
        match self {
            Ammo::Round => 1.0,
            Ammo::Chain => 0.8,
        }
    }

    /// A arma carregada com esta munição (alcance e velocidade da bala).
    pub fn load(self, weapon: WeaponParams) -> WeaponParams {
        WeaponParams {
            speed: weapon.speed * self.speed_factor(),
            range: weapon.range * self.range_factor(),
            ..weapon
        }
    }
}

/// Dano bruto ao pano → pontos de vela (0..100) proporcionais ao casco do
/// alvo: navio grande tem mais pano para rasgar.
pub fn sail_points(sail_damage: f32, target_max_hp: u32) -> f32 {
    sail_damage / target_max_hp.max(1) as f32 * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectile::Projectile;
    use crate::weapon::BroadsideSide;

    fn weapon() -> WeaponParams {
        WeaponParams {
            damage: 20,
            speed: 150.0,
            range: 210.0,
            muzzle_offset: 10.0,
        }
    }

    fn salvo(ammo: Ammo) -> Vec<Projectile> {
        Projectile::broadside_salvo(
            1,
            9,
            BroadsideSide::Port,
            0.0,
            0.0,
            0.0,
            0.0,
            ammo.load(weapon()),
            3,
            8.0,
        )
        .into_iter()
        .map(|p| p.with_ammo(ammo))
        .collect()
    }

    fn totals(balls: &[Projectile]) -> (u32, f32) {
        (
            balls.iter().map(|p| p.damage).sum(),
            balls.iter().map(|p| p.sail_damage).sum(),
        )
    }

    #[test]
    fn round_shot_splits_80_hull_20_sail() {
        let (hull, sail) = totals(&salvo(Ammo::Round));
        assert_eq!(hull, 16);
        assert!((sail - 4.0).abs() < 1e-4);
        assert!((sail_points(sail, 100) - 4.0).abs() < 1e-4);
    }

    #[test]
    fn chain_shot_shreds_sails_and_spares_the_hull() {
        let (round_hull, round_sail) = totals(&salvo(Ammo::Round));
        let (chain_hull, chain_sail) = totals(&salvo(Ammo::Chain));
        assert!((chain_sail - round_sail * 4.0).abs() < 1e-4);
        // 5/6/5 × 0.3 = 1.5/1.8/1.5 → 2/2/2.
        assert_eq!(chain_hull, 6);
        assert!(chain_hull * 2 < round_hull);
    }

    #[test]
    fn chain_shot_is_short_and_slow() {
        let chain = Ammo::Chain.load(weapon());
        assert!((chain.range - 126.0).abs() < 1e-3);
        assert!((chain.speed - 120.0).abs() < 1e-3);
        assert_eq!(Ammo::Round.load(weapon()), weapon());
    }

    #[test]
    fn sail_points_scale_with_target_hull() {
        assert!((sail_points(4.0, 200) - 2.0).abs() < 1e-5);
        assert!(sail_points(4.0, 0).is_finite());
    }

    #[test]
    fn c_cycles_ammo() {
        assert_eq!(Ammo::default(), Ammo::Round);
        assert_eq!(Ammo::Round.next(), Ammo::Chain);
        assert_eq!(Ammo::Chain.next(), Ammo::Round);
    }
}
