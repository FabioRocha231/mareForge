//! Projétil server-authoritative (PRD §20): position, direction, speed,
//! damage, owner_ship, lifetime. Sem rigid body — movimento retilíneo com
//! tempo de vida derivado do alcance da arma.

use serde::{Deserialize, Serialize};

use crate::ammo::{Ammo, HULL_SHARE, SAIL_SHARE};
use crate::weapon::BroadsideSide;

/// Parâmetros da arma no momento do disparo (vêm de `ShipStats` + tuning do
/// servidor — o domínio não conhece navios).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeaponParams {
    pub damage: u32,
    /// m/s.
    pub speed: f32,
    /// m — alcance máximo; deriva o tempo de vida do projétil.
    pub range: f32,
    /// m — distância do centro do casco até a boca do canhão (meia boca).
    pub muzzle_offset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Projectile {
    pub projectile_id: u32,
    pub owner_ship_id: u32,
    /// Dano ao casco (já repartido entre casco e pano, MF-059).
    pub damage: u32,
    /// Dano bruto ao pano; o alvo converte para pontos de vela pelo casco
    /// dele ([`crate::ammo::sail_points`]).
    #[serde(default)]
    pub sail_damage: f32,
    pub x: f32,
    pub y: f32,
    /// Direção de voo em radianos (0 = +X, anti-horário).
    pub heading: f32,
    /// m/s.
    pub speed: f32,
    /// Vida restante em segundos.
    pub remaining_lifetime: f32,
}

impl Projectile {
    /// Cria o projétil de uma borda: nasce na lateral do casco e voa
    /// perpendicular ao heading (PRD §19). Sai carregado de bala redonda:
    /// 80% do dano no casco, 20% no pano (MF-059).
    pub fn from_broadside(
        projectile_id: u32,
        owner_ship_id: u32,
        side: BroadsideSide,
        ship_x: f32,
        ship_y: f32,
        ship_heading: f32,
        weapon: WeaponParams,
    ) -> Self {
        let direction = ship_heading + side.angle_offset();
        let (dir_x, dir_y) = (direction.cos(), direction.sin());
        let (damage, sail_damage) = split_round(weapon.damage);
        Self {
            projectile_id,
            owner_ship_id,
            damage,
            sail_damage,
            x: ship_x + dir_x * weapon.muzzle_offset,
            y: ship_y + dir_y * weapon.muzzle_offset,
            heading: normalize(direction),
            speed: weapon.speed,
            remaining_lifetime: weapon.range / weapon.speed,
        }
    }

    /// Salva de bordo (MF-058): `balls` projéteis alinhados ao casco, a
    /// `spacing` metros entre si, herdando a velocidade do navio — atirar
    /// andando não deixa a bala para trás. O dano total da arma é repartido
    /// (sobra vai para a bala central), então a salva não muda o balanço
    /// de dano, só a chance de acerto. IDs são `first_id..first_id+balls`.
    #[allow(clippy::too_many_arguments)]
    pub fn broadside_salvo(
        first_id: u32,
        owner_ship_id: u32,
        side: BroadsideSide,
        ship_x: f32,
        ship_y: f32,
        ship_heading: f32,
        ship_speed: f32,
        weapon: WeaponParams,
        balls: u32,
        spacing: f32,
    ) -> Vec<Self> {
        let balls = balls.max(1);
        let base_damage = weapon.damage / balls;
        let remainder = weapon.damage % balls;
        let (hx, hy) = (ship_heading.cos(), ship_heading.sin());
        (0..balls)
            .map(|i| {
                let along = (i as f32 - (balls - 1) as f32 / 2.0) * spacing;
                let mut p = Self::from_broadside(
                    first_id + i,
                    owner_ship_id,
                    side,
                    ship_x + hx * along,
                    ship_y + hy * along,
                    ship_heading,
                    weapon,
                );
                (p.damage, p.sail_damage) =
                    split_round(base_damage + if i == balls / 2 { remainder } else { 0 });
                let vx = p.heading.cos() * p.speed + hx * ship_speed;
                let vy = p.heading.sin() * p.speed + hy * ship_speed;
                p.heading = normalize(vy.atan2(vx));
                p.speed = (vx * vx + vy * vy).sqrt();
                p
            })
            .collect()
    }

    /// Troca a munição (MF-059): fatores relativos à bala redonda. Alcance e
    /// velocidade vêm de [`Ammo::load`] aplicado à arma antes do disparo.
    pub fn with_ammo(mut self, ammo: Ammo) -> Self {
        self.damage = (self.damage as f32 * ammo.hull_factor()).round() as u32;
        self.sail_damage *= ammo.sail_factor();
        self
    }

    /// Movimento retilíneo por um passo de simulação.
    pub fn advance(&mut self, dt: f32) {
        self.x += self.heading.cos() * self.speed * dt;
        self.y += self.heading.sin() * self.speed * dt;
        self.remaining_lifetime -= dt;
    }

    pub fn expired(&self) -> bool {
        self.remaining_lifetime <= 0.0
    }

    /// Colisão por círculo: o navio alvo é aproximado por um raio (meia
    /// eslora). Determinístico e barato — exatamente o que o §20 pede.
    pub fn hit_ship(&self, ship_x: f32, ship_y: f32, ship_radius: f32) -> bool {
        let dx = self.x - ship_x;
        let dy = self.y - ship_y;
        dx * dx + dy * dy <= ship_radius * ship_radius
    }
}

/// Bala redonda: dano bruto → (casco, pano).
fn split_round(raw: u32) -> (u32, f32) {
    let raw = raw as f32;
    ((raw * HULL_SHARE).round() as u32, raw * SAIL_SHARE)
}

fn normalize(angle: f32) -> f32 {
    angle.rem_euclid(std::f32::consts::TAU)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weapon() -> WeaponParams {
        WeaponParams {
            damage: 20,
            speed: 40.0,
            range: 50.0,
            muzzle_offset: 5.0,
        }
    }

    #[test]
    fn port_broadside_flies_counterclockwise_from_left_side() {
        // Navio apontando +X (heading 0): bordo port voa para +Y, nascendo à esquerda.
        let p = Projectile::from_broadside(1, 10, BroadsideSide::Port, 0.0, 0.0, 0.0, weapon());

        assert!((p.heading - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!((p.x - (-0.0)).abs() < 1e-5 || p.x.abs() < 1e-5);
        assert!((p.y - 5.0).abs() < 1e-5);
        assert_eq!(p.damage, 16, "80% no casco");
        assert!((p.sail_damage - 4.0).abs() < 1e-5, "20% no pano");
    }

    #[test]
    fn starboard_broadside_flies_clockwise_from_right_side() {
        let p =
            Projectile::from_broadside(1, 10, BroadsideSide::Starboard, 100.0, 50.0, 0.0, weapon());

        // -π/2 normalizado para [0, 2π) = 3π/2 (mesma direção, convenção do modelo).
        let expected = (-std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU);
        assert!((p.heading - expected).abs() < 1e-5);
        assert!((p.y - 45.0).abs() < 1e-5);
    }

    #[test]
    fn lifetime_derives_from_range_and_speed() {
        let p = Projectile::from_broadside(1, 10, BroadsideSide::Port, 0.0, 0.0, 0.0, weapon());

        assert!((p.remaining_lifetime - 50.0 / 40.0).abs() < 1e-5);
    }

    #[test]
    fn advance_moves_straight_and_expires() {
        let mut p = Projectile::from_broadside(1, 10, BroadsideSide::Port, 0.0, 0.0, 0.0, weapon());
        let start_y = p.y;

        for _ in 0..20 {
            p.advance(0.1);
        }

        assert!((p.y - (start_y + 40.0 * 2.0)).abs() < 1e-4);
        assert!((p.x).abs() < 1e-4);
        assert!(p.expired());
    }

    #[test]
    fn hit_ship_uses_radius() {
        let p = Projectile::from_broadside(1, 10, BroadsideSide::Port, 0.0, 0.0, 0.0, weapon());

        // Nasce a 5 m do centro do dono; raio 10 cobre.
        assert!(p.hit_ship(0.0, 0.0, 10.0));
        assert!(!p.hit_ship(0.0, 100.0, 10.0));
    }

    #[test]
    fn salvo_splits_damage_and_spreads_along_hull() {
        let salvo = Projectile::broadside_salvo(
            7,
            10,
            BroadsideSide::Port,
            0.0,
            0.0,
            0.0,
            0.0,
            weapon(),
            3,
            8.0,
        );
        assert_eq!(salvo.len(), 3);
        // 6/8/6 brutos → casco 5/6/5 (80% arredondado), pano 20%.
        assert_eq!(salvo.iter().map(|p| p.damage).sum::<u32>(), 16);
        assert_eq!(salvo[1].damage, 6);
        let sail: f32 = salvo.iter().map(|p| p.sail_damage).sum();
        assert!((sail - 4.0).abs() < 1e-4);
        let ids: Vec<u32> = salvo.iter().map(|p| p.projectile_id).collect();
        assert_eq!(ids, vec![7, 8, 9]);
        assert!((salvo[0].x + 8.0).abs() < 1e-4 && (salvo[2].x - 8.0).abs() < 1e-4);
    }

    #[test]
    fn salvo_inherits_ship_velocity() {
        let salvo = Projectile::broadside_salvo(
            1,
            10,
            BroadsideSide::Port,
            0.0,
            0.0,
            0.0,
            30.0,
            weapon(),
            1,
            0.0,
        );
        let p = salvo[0];
        // Bala lateral (+Y a 40) somada ao navio (+X a 30): 50 m/s na diagonal.
        assert!((p.speed - 50.0).abs() < 1e-3);
        assert!((p.heading.cos() * p.speed - 30.0).abs() < 1e-3);
    }
}
