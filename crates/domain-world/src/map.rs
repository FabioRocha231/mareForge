//! Mapa do mundo (PRD §6): o triângulo econômico. Porto da Serra e Porto da
//! Mina nas águas protegidas, rotas de fronteira ligando os portos entre si e
//! à Ilha do Coral Negro no mar sem lei. Geografia é a regra de risco —
//! distância e oportunidade, nunca proteção secreta (§9).
//!
//! Resolução de sobreposição: a **primeira zona declarada que contém o ponto
//! vence** (ordem de declaração = prioridade). Águas protegidas vêm primeiro
//! para que a beira dos portos nunca seja engolida pelas rotas.

use mareforge_shared::ids::{RegionId, ZoneId};
use thiserror::Error;

use crate::land::{push_out_of_land, LandMass};
use crate::region::{Port, Region};
use crate::risk::RiskTier;
use crate::zone::{Zone, ZoneShape};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WorldError {
    /// Nenhuma zona declarada contém a posição (§69: nenhum default mágico —
    /// o mar além do mapa não existe legalmente).
    #[error("posição fora de todas as zonas declaradas (UnknownZone)")]
    UnknownZone,
    #[error("região desconhecida (UnknownRegion)")]
    UnknownRegion,
}

/// Nome da zona mais perigosa (Águas Negras) e seu centro.
pub const BLACK_WATERS: &str = "Águas Negras";
pub const BLACK_WATERS_CENTER: (f32, f32) = (0.0, 1750.0);
/// Porto pirata na ilha — fora da coroa, aceita procurados.
pub const PIRATE_PORT: &str = "Porto do Coral Negro";
/// Instâncias de Cerração: arenas cercadas de terra, longe do mapa principal.
pub const FOG_ZONE: &str = "Cerração";
pub const FOG_SLOTS: [(f32, f32); 3] = [(4200.0, -1400.0), (4200.0, 0.0), (4200.0, 1400.0)];
/// Raio navegável de cada cerração (a parede começa aqui).
pub const FOG_RADIUS: f32 = 560.0;
/// Passagem do Sorvedouro: corredor norte-sul cercado de terra.
pub const MAELSTROM_ZONE: &str = "Passagem do Sorvedouro";
pub const MAELSTROM_X: f32 = -4200.0;
pub const MAELSTROM_HALF_LENGTH: f32 = 1400.0;
/// Meia largura navegável do corredor.
pub const MAELSTROM_HALF_WIDTH: f32 = 250.0;
/// Pontos da passagem ligados a cada redemoinho do mundo (sul, meio, norte).
pub const MAELSTROM_POINTS: [(f32, f32); 3] = [
    (MAELSTROM_X, -1000.0),
    (MAELSTROM_X, 0.0),
    (MAELSTROM_X, 1000.0),
];

const WALL_RADIUS: f32 = 170.0;

/// Terra das instâncias: anel de parede em cada cerração, corredor de Sorvedouro
/// e rochedos de dentro. Determinístico (mesma geometria no servidor e no
/// client).
fn instance_land() -> Vec<LandMass> {
    let mut land = Vec::new();
    for (slot, (cx, cy)) in FOG_SLOTS.into_iter().enumerate() {
        let ring = FOG_RADIUS + WALL_RADIUS;
        let count = (std::f32::consts::TAU * ring / 200.0).ceil() as usize;
        for k in 0..count {
            let a = k as f32 / count as f32 * std::f32::consts::TAU;
            land.push(LandMass {
                x: cx + a.cos() * ring,
                y: cy + a.sin() * ring,
                radius: WALL_RADIUS,
            });
        }
        for k in 0..5 {
            let a = k as f32 * 1.3 + slot as f32;
            let r = 190.0 + (k % 3) as f32 * 90.0;
            land.push(LandMass {
                x: cx + a.cos() * r,
                y: cy + a.sin() * r,
                radius: 22.0 + (k % 3) as f32 * 8.0,
            });
        }
    }
    let wall_x = MAELSTROM_HALF_WIDTH + WALL_RADIUS;
    let mut y = -MAELSTROM_HALF_LENGTH - WALL_RADIUS;
    while y <= MAELSTROM_HALF_LENGTH + WALL_RADIUS {
        for side in [-1.0, 1.0] {
            land.push(LandMass {
                x: MAELSTROM_X + side * wall_x,
                y,
                radius: WALL_RADIUS,
            });
        }
        y += 170.0;
    }
    for end in [-1.0, 1.0] {
        let mut x = -wall_x;
        while x <= wall_x {
            land.push(LandMass {
                x: MAELSTROM_X + x,
                y: end * (MAELSTROM_HALF_LENGTH + WALL_RADIUS),
                radius: WALL_RADIUS,
            });
            x += 170.0;
        }
    }
    // Slalom de rochedos no corredor, longe dos pontos de chegada.
    for (i, y) in [-1300.0, -500.0, 500.0, 1300.0].into_iter().enumerate() {
        let side = if i % 2 == 0 { -1.0 } else { 1.0 };
        land.push(LandMass {
            x: MAELSTROM_X + side * 110.0,
            y,
            radius: 34.0,
        });
    }
    land
}

/// O mundo: zonas de risco e regiões econômicas.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldMap {
    zones: Vec<Zone>,
    regions: Vec<Region>,
    land: Vec<LandMass>,
}

impl WorldMap {
    /// Zona na posição. Primeira zona declarada que contém o ponto vence;
    /// nenhuma contendo é `UnknownZone` (fail-closed, §69).
    pub fn zone_at(&self, x: f32, y: f32) -> Result<&Zone, WorldError> {
        self.zones
            .iter()
            .find(|zone| zone.contains(x, y))
            .ok_or(WorldError::UnknownZone)
    }

    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    pub fn land(&self) -> &[LandMass] {
        &self.land
    }

    /// Posição corrigida para fora da terra (ver [`push_out_of_land`]).
    pub fn push_out_of_land(&self, x: f32, y: f32, clearance: f32) -> Option<(f32, f32)> {
        push_out_of_land(&self.land, x, y, clearance)
    }

    pub fn is_land(&self, x: f32, y: f32) -> bool {
        self.land.iter().any(|mass| mass.contains(x, y, 0.0))
    }

    pub fn region_by_name(&self, name: &str) -> Result<&Region, WorldError> {
        self.regions
            .iter()
            .find(|region| region.name == name)
            .ok_or(WorldError::UnknownRegion)
    }

    /// O mundo do Vertical Slice (PRD §6): o triângulo econômico.
    ///
    /// Geografia (x cresce para leste, y para norte):
    ///
    /// ```text
    ///              Ilha do Coral Negro (0, 900) — Lawless
    ///                corredores de fronteira subindo dos dois portos
    ///   Porto da Serra (-600, 0) ──── Rota da Costa ──── Porto da Mina (600, 0)
    ///     (Protected)   (Frontier: caravanas podem ser caçadas)   (Protected)
    /// ```
    ///
    /// O "Mar Sem Lei" externo é uma zona declarada como as outras (conteúdo
    /// explícito do mapa, não um default de código): tudo dentro do mar
    /// jogável que não for porto nem rota é lawless. Fora dele, `UnknownZone`.
    pub fn vertical_slice() -> Self {
        let mut zones = Vec::new();
        let mut zone = |name: &'static str, tier: RiskTier, x: f32, y: f32, radius: f32| {
            zones.push(Zone {
                id: ZoneId::new(),
                name,
                tier,
                shape: ZoneShape::Circle { x, y, radius },
            });
        };

        // 1. Águas protegidas dos portos (prioridade máxima nas sobreposições).
        zone(
            "Águas do Porto da Serra",
            RiskTier::Protected,
            -600.0,
            0.0,
            200.0,
        );
        zone(
            "Águas do Porto da Mina",
            RiskTier::Protected,
            600.0,
            0.0,
            200.0,
        );

        // 2. Rotas de fronteira — cadeias de círculos encadeados. A Rota da
        // Costa liga os portos por dentro (caravana cobiçada, §7); os
        // corredores sobem para a ilha.
        for x in [-300.0, 0.0, 300.0] {
            zone("Rota da Costa", RiskTier::Frontier, x, 0.0, 120.0);
        }
        for (x, y) in [
            (-480.0, 180.0),
            (-360.0, 360.0),
            (-240.0, 540.0),
            (-120.0, 720.0),
        ] {
            zone("Corredor do Amanhecer", RiskTier::Frontier, x, y, 140.0);
        }
        for (x, y) in [
            (480.0, 180.0),
            (360.0, 360.0),
            (240.0, 540.0),
            (120.0, 720.0),
        ] {
            zone("Corredor do Poente", RiskTier::Frontier, x, y, 140.0);
        }

        // 3. Mar sem lei: as águas da ilha (nome próprio para a UI) e o
        // alto-mar que cobre todo o resto do mundo declarado.
        zone(
            "Águas da Ilha do Coral Negro",
            RiskTier::Lawless,
            0.0,
            900.0,
            350.0,
        );
        // 4. MF-059 — zonas de alto risco, todas sem lei (full loot):
        // Águas Negras ao norte da ilha; instâncias de Cerração
        // e a Passagem do Sorvedouro, isoladas por paredes de terra longe do
        // mapa principal e alcançáveis só por portal.
        zone(
            BLACK_WATERS,
            RiskTier::Lawless,
            BLACK_WATERS_CENTER.0,
            BLACK_WATERS_CENTER.1,
            560.0,
        );
        for (x, y) in FOG_SLOTS {
            zone(FOG_ZONE, RiskTier::Lawless, x, y, FOG_RADIUS + 40.0);
        }
        let mut y = -MAELSTROM_HALF_LENGTH;
        while y <= MAELSTROM_HALF_LENGTH {
            zone(MAELSTROM_ZONE, RiskTier::Lawless, MAELSTROM_X, y, 300.0);
            y += 280.0;
        }

        zone("Mar Sem Lei", RiskTier::Lawless, 0.0, 0.0, 8000.0);

        let regions = vec![
            Region {
                id: RegionId::new(),
                name: "Porto da Serra",
                port: Some(Port {
                    name: "Porto da Serra",
                    x: -600.0,
                    y: 0.0,
                    service_radius: 60.0,
                }),
            },
            Region {
                id: RegionId::new(),
                name: "Porto da Mina",
                port: Some(Port {
                    name: "Porto da Mina",
                    x: 600.0,
                    y: 0.0,
                    service_radius: 60.0,
                }),
            },
            Region {
                id: RegionId::new(),
                name: "Ilha do Coral Negro",
                // Porto pirata: atraca qualquer um, até
                // procurado, mas a porta dele é água sem lei.
                port: Some(Port {
                    name: PIRATE_PORT,
                    x: 10.0,
                    y: 1078.0,
                    service_radius: 60.0,
                }),
            },
        ];

        // 4. Terra (MF-058): cada porto encosta numa costa a barlavento
        // (Serra a oeste, Mina a leste), a ilha tem corpo e o mar aberto
        // ganha rochedos — cobertura para quem caça e para quem foge.
        let mut land = Vec::new();
        for side in [-1.0_f32, 1.0] {
            for (x, y, radius) in [
                (820.0, 0.0, 190.0),
                (800.0, -220.0, 150.0),
                (790.0, 230.0, 150.0),
                (950.0, 420.0, 250.0),
                (950.0, -440.0, 250.0),
                (1150.0, 0.0, 260.0),
            ] {
                land.push(LandMass {
                    x: side * x,
                    y,
                    radius,
                });
            }
        }
        for (x, y, radius) in [
            // Ilha do Coral Negro: corpo irregular a norte do recife.
            (0.0, 945.0, 65.0),
            (-50.0, 915.0, 40.0),
            (50.0, 965.0, 42.0),
            (10.0, 1000.0, 45.0),
            // Rochedos do mar aberto.
            (-250.0, -190.0, 34.0),
            (262.0, -205.0, 28.0),
            (-60.0, 520.0, 28.0),
            (92.0, 600.0, 22.0),
            (0.0, 330.0, 18.0),
            // Recifes das Águas Negras.
            (-220.0, 1500.0, 30.0),
            (180.0, 1580.0, 36.0),
            (-60.0, 1790.0, 26.0),
            (260.0, 1900.0, 32.0),
            (-300.0, 1960.0, 38.0),
            (60.0, 2110.0, 28.0),
        ] {
            land.push(LandMass { x, y, radius });
        }
        land.extend(instance_land());

        Self {
            zones,
            regions,
            land,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier_at(map: &WorldMap, x: f32, y: f32) -> Result<(&'static str, RiskTier), WorldError> {
        map.zone_at(x, y).map(|z| (z.name, z.tier))
    }

    #[test]
    fn port_waters_are_protected() {
        let map = WorldMap::vertical_slice();
        let (name, tier) = tier_at(&map, -560.0, 0.0).unwrap();
        assert_eq!(tier, RiskTier::Protected);
        assert_eq!(name, "Águas do Porto da Serra");
        assert_eq!(tier_at(&map, 600.0, 0.0).unwrap().1, RiskTier::Protected);
    }

    #[test]
    fn trade_route_and_corridors_are_frontier() {
        let map = WorldMap::vertical_slice();
        let (name, tier) = tier_at(&map, 0.0, 0.0).unwrap();
        assert_eq!(tier, RiskTier::Frontier);
        assert_eq!(name, "Rota da Costa");
        assert_eq!(
            tier_at(&map, -360.0, 360.0).unwrap(),
            ("Corredor do Amanhecer", RiskTier::Frontier)
        );
        assert_eq!(
            tier_at(&map, 240.0, 540.0).unwrap(),
            ("Corredor do Poente", RiskTier::Frontier)
        );
    }

    #[test]
    fn island_and_open_sea_are_lawless() {
        let map = WorldMap::vertical_slice();
        assert_eq!(
            tier_at(&map, 0.0, 900.0).unwrap(),
            ("Águas da Ilha do Coral Negro", RiskTier::Lawless)
        );
        // mar aberto fora de qualquer rota: cai no alto-mar declarado.
        assert_eq!(
            tier_at(&map, 5000.0, -3000.0).unwrap(),
            ("Mar Sem Lei", RiskTier::Lawless)
        );
    }

    #[test]
    fn outside_every_declared_zone_fails_closed() {
        let map = WorldMap::vertical_slice();
        assert_eq!(tier_at(&map, 20_000.0, 0.0), Err(WorldError::UnknownZone));
    }

    #[test]
    fn protected_beats_frontier_where_zones_overlap() {
        let map = WorldMap::vertical_slice();
        // (-410, 0): dentro das águas do Porto da Serra (raio 200) e da Rota
        // da Costa (círculo em -300, raio 120). A declaração protegida vem
        // primeiro — a beira do porto nunca vira rota.
        assert_eq!(tier_at(&map, -410.0, 0.0).unwrap().1, RiskTier::Protected);
    }

    #[test]
    fn regions_expose_ports_and_fail_closed_on_unknown() {
        let map = WorldMap::vertical_slice();
        let serra = map.region_by_name("Porto da Serra").unwrap();
        let port = serra.port.as_ref().expect("Porto da Serra tem porto");
        assert!(port.contains(-600.0, 0.0));
        assert!(!port.contains(0.0, 0.0));

        // MF-059: a ilha ganhou o porto pirata.
        let ilha = map.region_by_name("Ilha do Coral Negro").unwrap();
        assert_eq!(ilha.port.as_ref().map(|p| p.name), Some(PIRATE_PORT));

        assert!(matches!(
            map.region_by_name("Atlântida"),
            Err(WorldError::UnknownRegion)
        ));
    }

    #[test]
    fn corridor_circles_form_a_contiguous_chain_to_the_island() {
        // Navegar do porto à ilha pelos waypoints dos corredores (e pelos
        // pontos médios entre eles) nunca encontra UnknownZone — os círculos
        // se encadeiam de verdade.
        let map = WorldMap::vertical_slice();
        let waypoints = [
            (-560.0, 0.0),
            (-480.0, 180.0),
            (-360.0, 360.0),
            (-240.0, 540.0),
            (-120.0, 720.0),
            (0.0, 900.0),
        ];
        for pair in waypoints.windows(2) {
            let (ax, ay) = (pair[0].0, pair[0].1);
            let (bx, by) = (pair[1].0, pair[1].1);
            let mid = ((ax + bx) / 2.0, (ay + by) / 2.0);
            for (x, y) in [(ax, ay), mid] {
                assert!(
                    map.zone_at(x, y).is_ok(),
                    "trecho ({x}, {y}) do corredor cai em UnknownZone"
                );
            }
        }
    }

    #[test]
    fn ports_and_trade_lanes_are_open_water() {
        let map = WorldMap::vertical_slice();
        let clearance = 20.0;
        for region in map.regions() {
            if let Some(port) = &region.port {
                assert!(
                    map.push_out_of_land(port.x, port.y, clearance).is_none(),
                    "{}: o cais precisa de água para um casco atracar",
                    port.name
                );
            }
        }
        // Rota da Costa e corredores: o eixo de cada círculo é navegável.
        for zone in map.zones().iter().filter(|z| z.tier == RiskTier::Frontier) {
            let ZoneShape::Circle { x, y, .. } = zone.shape;
            assert!(
                map.push_out_of_land(x, y, clearance).is_none(),
                "{}",
                zone.name
            );
        }
    }

    #[test]
    fn island_has_land_and_ships_cannot_enter_it() {
        let map = WorldMap::vertical_slice();
        assert!(map.is_land(0.0, 950.0));
        let (x, y) = map.push_out_of_land(0.0, 950.0, 15.0).unwrap();
        assert!(!map.is_land(x, y));
    }

    #[test]
    fn instances_are_enclosed_and_their_portal_points_are_open_water() {
        let map = WorldMap::vertical_slice();
        for (x, y) in FOG_SLOTS {
            assert_eq!(map.zone_at(x, y).unwrap().name, FOG_ZONE);
            assert!(map.push_out_of_land(x, y - 250.0, 20.0).is_none());
            // Qualquer direção, logo depois do raio navegável, é parede.
            for k in 0..16 {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                let r = FOG_RADIUS + 60.0;
                assert!(map.is_land(x + a.cos() * r, y + a.sin() * r), "brecha {k}");
            }
        }
        for (x, y) in MAELSTROM_POINTS {
            assert_eq!(map.zone_at(x, y).unwrap().name, MAELSTROM_ZONE);
            assert!(map.push_out_of_land(x, y, 20.0).is_none());
            assert!(map.is_land(x + MAELSTROM_HALF_WIDTH + 60.0, y));
            assert!(map.is_land(x - MAELSTROM_HALF_WIDTH - 60.0, y));
        }
    }

    #[test]
    fn black_waters_and_pirate_port() {
        let map = WorldMap::vertical_slice();
        let (x, y) = BLACK_WATERS_CENTER;
        let zone = map.zone_at(x, y).unwrap();
        assert_eq!((zone.name, zone.tier), (BLACK_WATERS, RiskTier::Lawless));
        let island = map.region_by_name("Ilha do Coral Negro").unwrap();
        let port = island.port.as_ref().expect("porto pirata");
        assert_eq!(port.name, PIRATE_PORT);
        assert_eq!(map.zone_at(port.x, port.y).unwrap().tier, RiskTier::Lawless);
    }
}
