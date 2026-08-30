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

/// O mundo: zonas de risco e regiões econômicas.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldMap {
    zones: Vec<Zone>,
    regions: Vec<Region>,
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
                port: None,
            },
        ];

        Self { zones, regions }
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

        let ilha = map.region_by_name("Ilha do Coral Negro").unwrap();
        assert!(ilha.port.is_none());

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
}
