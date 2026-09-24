//! Exploração (MV-061): ilhas ocultas e mapas do tesouro. A ilha existe no
//! mundo do servidor (é terra: bala morre nela, navio encalha), mas o
//! client só a desenha quando o navio chega perto — conhecimento do mundo
//! é vantagem, não dado de HUD.
//!
//! O tesouro é recurso bruto em porão: quem cava ainda precisa voltar vivo
//! ao porto com ele (Pilar 2) e fabricar algo com ele (Pilar 1).

use crate::land::LandMass;

/// Ilha fora das cartas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HiddenIsland {
    pub id: u32,
    pub name: &'static str,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    /// Ponto de escavação: na água rasa junto à praia, fora da terra.
    pub dig_x: f32,
    pub dig_y: f32,
}

/// As ilhas ocultas do slice — todas em águas de fronteira ou sem lei.
pub const HIDDEN_ISLANDS: [HiddenIsland; 4] = [
    HiddenIsland {
        id: 1,
        name: "Ilhota da Caveira",
        x: -600.0,
        y: 1250.0,
        radius: 55.0,
        dig_x: -600.0,
        dig_y: 1180.0,
    },
    HiddenIsland {
        id: 2,
        name: "Recife dos Afogados",
        x: 650.0,
        y: 1300.0,
        radius: 50.0,
        dig_x: 650.0,
        dig_y: 1235.0,
    },
    HiddenIsland {
        id: 3,
        name: "Baixio do Enforcado",
        x: -300.0,
        y: -950.0,
        radius: 45.0,
        dig_x: -300.0,
        dig_y: -885.0,
    },
    HiddenIsland {
        id: 4,
        name: "Atol Sem Nome",
        x: 820.0,
        y: 1450.0,
        radius: 60.0,
        dig_x: 820.0,
        dig_y: 1375.0,
    },
];

/// Distância (m) em que a vigia avista uma ilha oculta.
pub const SIGHT_RADIUS: f32 = 320.0;
/// Distância máxima (m) do ponto de escavação.
pub const DIG_RADIUS: f32 = 40.0;
/// Segundos parado cavando.
pub const DIG_SECS: f32 = 8.0;
/// Velocidade máxima (m/s) para seguir cavando.
pub const DIG_MAX_SPEED: f32 = 1.5;

impl HiddenIsland {
    pub fn land(&self) -> LandMass {
        LandMass {
            x: self.x,
            y: self.y,
            radius: self.radius,
        }
    }

    pub fn in_sight(&self, x: f32, y: f32) -> bool {
        within(self.x, self.y, x, y, SIGHT_RADIUS + self.radius)
    }

    pub fn at_dig_spot(&self, x: f32, y: f32) -> bool {
        within(self.dig_x, self.dig_y, x, y, DIG_RADIUS)
    }
}

/// Ilha para onde um mapa aponta. Derivada do id da instância do mapa —
/// cada mapa é um tesouro fixo, sem estado extra para persistir.
pub fn island_for_map(map_seed: u128) -> &'static HiddenIsland {
    let index = (map_seed % HIDDEN_ISLANDS.len() as u128) as usize;
    &HIDDEN_ISLANDS[index]
}

/// Chance (%) de um lance de coleta trazer um mapa do tesouro junto.
pub const MAP_FIND_PERCENT: u128 = 6;

/// Um lance de coleta achou mapa? `roll` é um número aleatório do servidor.
pub fn finds_map(roll: u128) -> bool {
    roll % 100 < MAP_FIND_PERCENT
}

fn within(ax: f32, ay: f32, bx: f32, by: f32, radius: f32) -> bool {
    let (dx, dy) = (ax - bx, ay - by);
    dx * dx + dy * dy <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::WorldMap;
    use crate::risk::RiskTier;

    #[test]
    fn islands_sit_in_risky_water_with_dig_spot_off_the_beach() {
        let map = WorldMap::vertical_slice();
        for island in &HIDDEN_ISLANDS {
            let tier = map.zone_at(island.x, island.y).unwrap().tier;
            assert_ne!(tier, RiskTier::Protected, "{}", island.name);
            assert!(!island.land().contains(island.dig_x, island.dig_y, 0.0));
            assert!(
                !map.is_land(island.dig_x, island.dig_y),
                "{} cava em terra do mapa",
                island.name
            );
            assert!(island.in_sight(island.dig_x, island.dig_y));
            assert!(island.at_dig_spot(island.dig_x + 10.0, island.dig_y));
        }
    }

    #[test]
    fn map_points_to_a_fixed_island() {
        assert_eq!(island_for_map(7).id, island_for_map(7).id);
        let ids: std::collections::HashSet<u32> =
            (0..8u128).map(|seed| island_for_map(seed).id).collect();
        assert_eq!(ids.len(), HIDDEN_ISLANDS.len());
    }

    #[test]
    fn map_find_rate_is_rare() {
        let found = (0..1000u128).filter(|roll| finds_map(*roll)).count();
        assert_eq!(found, 60);
    }
}
