//! Mundo procedural (MV-065). A seed decide a forma do triângulo econômico
//! (rumo, distância entre as capitais, onde fica a Ilha do Coral Negro e as
//! Águas Negras), espelha costas e espalha rochedos, ilhotas e ilhas
//! ocultas. A estrutura local (costa atrás de cada capital, corpo da ilha,
//! nós, pontos de NPC e de evento) vem de moldes medidos no mapa clássico e
//! reposicionados no referencial de cada lugar — jogabilidade testada, mapa
//! novo.
//!
//! Como no Albion, o mapa-base é fixo depois de gerado (a seed é
//! configuração do servidor); o que muda sempre é a camada viva por cima
//! (portais, eventos). Determinístico: mesma seed, mesmo mundo, servidor e
//! client.
//!
//! Garantia construtiva: tudo que é espalhado (rochedos, ilhotas, ilhas
//! ocultas) é rejeitado perto de rotas, portos, nós e pontos de conteúdo —
//! nenhuma rota de NPC atravessa terra e nenhum ponto nasce em terra. Os
//! testes varrem centenas de seeds conferindo isso.

use std::f32::consts::TAU;
use std::ops::{Add, Mul, Sub};

use crate::features::{instance_nodes, maelstrom_label, Features, NodeSpot, Sector, ISLAND};
use crate::land::{push_out_of_land, LandMass};
use crate::map::{
    instance_and_open_sea_zones, instance_land, region, zone, WorldMap, BLACK_WATERS, PIRATE_PORT,
};
use crate::region::Port;
use crate::risk::RiskTier;
use crate::treasure::HiddenIsland;

const SERRA: &str = "Porto da Serra";
const MINA: &str = "Porto da Mina";
/// Raio das águas protegidas de cada capital.
const PROTECTED_RADIUS: f32 = 200.0;
const BLACK_WATERS_RADIUS: f32 = 560.0;

/// Costa atrás de cada capital: (para fora, de lado, raio) — o molde da
/// costa da Serra no mapa clássico.
const COAST: [(f32, f32, f32); 6] = [
    (220.0, 0.0, 190.0),
    (200.0, -220.0, 150.0),
    (190.0, 230.0, 150.0),
    (350.0, 420.0, 250.0),
    (350.0, -440.0, 250.0),
    (550.0, 0.0, 260.0),
];
/// Nós da capital no mesmo referencial; o último é o "do Caminho".
const CAPITAL_NODES: [(f32, f32); 5] = [
    (20.0, 175.0),
    (-100.0, 130.0),
    (10.0, -165.0),
    (-130.0, -70.0),
    (-170.0, 20.0),
];
/// Corpo da Ilha do Coral Negro: (de lado, para fora, raio).
const ISLAND_BODY: [(f32, f32, f32); 4] = [
    (0.0, 0.0, 65.0),
    (-50.0, -30.0, 40.0),
    (50.0, 20.0, 42.0),
    (10.0, 55.0, 45.0),
];
/// Recifes das Águas Negras em volta do centro.
const REEFS: [(f32, f32, f32); 6] = [
    (-220.0, -250.0, 30.0),
    (180.0, -170.0, 36.0),
    (-60.0, 40.0, 26.0),
    (260.0, 150.0, 32.0),
    (-300.0, 210.0, 38.0),
    (60.0, 360.0, 28.0),
];
const HIDDEN_NAMES: [&str; 4] = [
    "Ilhota da Caveira",
    "Recife dos Afogados",
    "Baixio do Enforcado",
    "Atol Sem Nome",
];

#[derive(Debug, Clone, Copy, PartialEq)]
struct V(f32, f32);

impl Add for V {
    type Output = V;
    fn add(self, o: V) -> V {
        V(self.0 + o.0, self.1 + o.1)
    }
}
impl Sub for V {
    type Output = V;
    fn sub(self, o: V) -> V {
        V(self.0 - o.0, self.1 - o.1)
    }
}
impl Mul<f32> for V {
    type Output = V;
    fn mul(self, k: f32) -> V {
        V(self.0 * k, self.1 * k)
    }
}
impl V {
    fn len(self) -> f32 {
        self.0.hypot(self.1)
    }
    fn norm(self) -> V {
        self * (1.0 / self.len().max(f32::EPSILON))
    }
    fn dot(self, o: V) -> f32 {
        self.0 * o.0 + self.1 * o.1
    }
    fn polar(angle: f32, r: f32) -> V {
        V(angle.cos() * r, angle.sin() * r)
    }
    fn t(self) -> (f32, f32) {
        (self.0, self.1)
    }
}

/// splitmix64 — o mesmo gerador barato do resto do domínio.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * ((self.next() >> 40) as f32 / (1u64 << 24) as f32)
    }
    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

/// Distância de `p` ao segmento `a`-`b`.
fn segment_distance(p: V, a: V, b: V) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.dot(ab).max(f32::EPSILON)).clamp(0.0, 1.0);
    (p - (a + ab * t)).len()
}

/// Cadeia de círculos de `from` a `to` (exclusive), no máximo `step` entre
/// centros.
fn chain(from: V, to: V, step: f32) -> Vec<V> {
    let count = ((to - from).len() / step).ceil().max(2.0) as usize;
    (1..count)
        .map(|k| from + (to - from) * (k as f32 / count as f32))
        .collect()
}

/// O que o espalhamento não pode tocar: rotas (segmentos) e pontos de
/// conteúdo, cada um com sua folga.
struct KeepClear {
    lanes: Vec<(V, V)>,
    points: Vec<(V, f32)>,
}

impl KeepClear {
    fn allows(&self, at: V, radius: f32, lane_gap: f32) -> bool {
        self.lanes
            .iter()
            .all(|&(a, b)| segment_distance(at, a, b) > radius + lane_gap)
            && self
                .points
                .iter()
                .all(|&(p, gap)| (at - p).len() > radius + gap)
    }
}

fn clear_of_land(land: &[LandMass], at: V, radius: f32, gap: f32) -> bool {
    land.iter()
        .all(|m| (at - V(m.x, m.y)).len() > radius + m.radius + gap)
}

/// Ponto de conteúdo na água: sai da terra com folga se o molde caiu na
/// costa.
fn settle(land: &[LandMass], at: V, clearance: f32) -> V {
    push_out_of_land(land, at.0, at.1, clearance)
        .map(|(x, y)| V(x, y))
        .unwrap_or(at)
}

fn sector(center: V, half: f32) -> Sector {
    (
        center.0 - half,
        center.0 + half,
        center.1 - half,
        center.1 + half,
    )
}

pub(crate) fn generate(seed: u64) -> WorldMap {
    let mut rng = Rng(seed);

    // 1. Esqueleto: as duas capitais da coroa, a ilha pirata do lado de
    // dentro e as Águas Negras além dela.
    let axis = rng.range(0.0, TAU);
    let e = V(axis.cos(), axis.sin());
    let n = V(-e.1, e.0);
    let half = rng.range(560.0, 760.0);
    let serra = e * -half + n * rng.range(-60.0, 60.0);
    let mina = e * half + n * rng.range(-60.0, 60.0);
    let mid = (serra + mina) * 0.5;
    let island = mid + n * rng.range(850.0, 1100.0) + e * rng.range(-220.0, 220.0);
    let o = (island - mid).norm();
    let p = V(o.1, -o.0);
    let isl = |x: f32, y: f32| island + p * x + o * y;
    let black_waters = island + o * rng.range(700.0, 850.0);
    let bw = |x: f32, y: f32| black_waters + p * x + o * y;
    let pirate_port = isl(10.0, 133.0);
    let lane = (mina - serra).norm();
    let serra_dock = serra + lane * 40.0;
    let mina_dock = mina - lane * 40.0;

    // 2. Terra ancorada: costa atrás de cada capital (espelhada ou não),
    // corpo da ilha, recifes das Águas Negras.
    let mut land = Vec::new();
    let mut nodes = Vec::new();
    let capitals = [
        (
            SERRA,
            serra,
            serra - mina,
            "Bosque da Serra",
            "Bosque do Caminho",
        ),
        (MINA, mina, mina - serra, "Mina Profunda", "Mina do Caminho"),
    ];
    for (region_name, port, outward, deposit, road) in capitals {
        let out = outward.norm();
        let side = if rng.coin() { n } else { n * -1.0 };
        let frame = |a: f32, b: f32| port + out * a + side * b;
        for (a, b, r) in COAST {
            let at = frame(a + rng.range(0.0, 30.0), b + rng.range(-25.0, 25.0));
            land.push(disc(at, r * rng.range(0.9, 1.0)));
        }
        for (k, (a, b)) in CAPITAL_NODES.into_iter().enumerate() {
            let at = frame(a, b);
            let name = if k == CAPITAL_NODES.len() - 1 {
                road
            } else {
                deposit
            };
            nodes.push(node(name, region_name, at, 60));
        }
    }
    for (x, y, r) in ISLAND_BODY {
        land.push(disc(isl(x, y), r));
    }
    for (x, y, r) in REEFS {
        let at = bw(x + rng.range(-20.0, 20.0), y + rng.range(-20.0, 20.0));
        land.push(disc(at, r));
    }
    for (x, y) in [(0.0, -90.0), (-125.0, 5.0), (135.0, 55.0)] {
        nodes.push(node("Recife do Coral", ISLAND, isl(x, y), 30));
    }
    for (x, y) in [(-140.0, -130.0), (120.0, 10.0), (-110.0, 250.0)] {
        nodes.push(node("Recife Abissal", ISLAND, bw(x, y), 12));
    }

    // 3. Rotas e pontos de conteúdo (antes do espalhamento, que os evita).
    // O comboio do tesouro passa ao largo das costas, do lado oposto à ilha.
    let reach_behind = land
        .iter()
        .take(COAST.len() * 2)
        .map(|m| -(V(m.x, m.y) - mid).dot(n) + m.radius)
        .fold(0.0_f32, f32::max);
    let fleet_offset = reach_behind.max(PROTECTED_RADIUS) + 110.0;
    let fleet_route: Vec<V> = [-1.0, -0.5, 0.0, 0.5, 1.0]
        .into_iter()
        .map(|t| mid - n * (fleet_offset + rng.range(-15.0, 25.0)) + e * (t * (half + 700.0)))
        .collect();
    let caravan_route: Vec<V> = [0.0, 0.23, 0.5, 0.77, 1.0]
        .into_iter()
        .map(|t| serra_dock + (mina_dock - serra_dock) * t)
        .collect();
    let settle60 = |at: V| settle(&land, at, 60.0);
    let tempest_sites = vec![
        settle60(mid + (island - mid) * 0.53),
        settle60(serra + (serra - mina).norm() * 100.0 + n * 700.0),
        settle60(mina + (mina - serra).norm() * 100.0 + n * 700.0),
    ];
    let kraken_sites = vec![
        settle60(bw(0.0, -50.0)),
        settle60(isl(-900.0, 155.0)),
        settle60(isl(900.0, 155.0)),
    ];
    let tide_sites = vec![
        settle60(isl(0.0, 355.0)),
        settle60(isl(-500.0, 155.0)),
        settle60(isl(500.0, 155.0)),
    ];
    let pirate_spawns: Vec<V> = [(0.0, -145.0), (-190.0, -85.0), (190.0, 5.0)]
        .into_iter()
        .map(|(x, y)| settle(&land, isl(x, y), 30.0))
        .collect();
    let raider_spawns = vec![
        mid + lane * (-0.4 * half) + n * 60.0,
        mid + lane * (0.3 * half) - n * 60.0,
    ];
    let navy_spawns = vec![
        serra + lane * 170.0 + n * 60.0,
        mina - lane * 170.0 - n * 60.0,
    ];

    let mut keep = KeepClear {
        lanes: vec![
            (serra, mina),
            (serra, island),
            (mina, island),
            (island, black_waters),
        ],
        points: vec![
            (serra, PROTECTED_RADIUS + 60.0),
            (mina, PROTECTED_RADIUS + 60.0),
            (island, 330.0),
            (black_waters, BLACK_WATERS_RADIUS + 40.0),
        ],
    };
    keep.lanes
        .extend(fleet_route.windows(2).map(|leg| (leg[0], leg[1])));
    let content_points = tempest_sites
        .iter()
        .chain(&kraken_sites)
        .chain(&tide_sites)
        .map(|&at| (at, 90.0))
        .chain(
            pirate_spawns
                .iter()
                .chain(&raider_spawns)
                .chain(&navy_spawns)
                .map(|&at| (at, 60.0)),
        )
        .chain(nodes.iter().map(|spot| (V(spot.x, spot.y), 70.0)))
        .collect::<Vec<_>>();
    keep.points.extend(content_points);

    // 4. Espalhamento: rochedos (cobertura), ilhotas (horizonte) e as ilhas
    // ocultas (só o servidor as tem como terra até alguém avistá-las).
    let mut rocks = 0;
    for _ in 0..800 {
        if rocks == 16 {
            break;
        }
        let at = mid + V::polar(rng.range(0.0, TAU), rng.range(300.0, 2000.0));
        let r = rng.range(18.0, 38.0);
        if keep.allows(at, r, 70.0) && clear_of_land(&land, at, r, 60.0) {
            land.push(disc(at, r));
            rocks += 1;
        }
    }
    // Ilhotas: um corpo e 1-3 lóbulos — do atol pequeno ao arquipélago.
    let mut islets = 0;
    for _ in 0..800 {
        if islets == 7 {
            break;
        }
        let at = mid + V::polar(rng.range(0.0, TAU), rng.range(800.0, 2000.0));
        let r = rng.range(50.0, 120.0);
        let lobes: Vec<(V, f32)> = (0..1 + rng.next() % 3)
            .map(|_| {
                let lobe_r = r * rng.range(0.4, 0.75);
                (
                    at + V::polar(rng.range(0.0, TAU), r * rng.range(0.6, 0.95)),
                    lobe_r,
                )
            })
            .collect();
        // O raio de rejeição cobre o corpo e os lóbulos.
        let span = lobes
            .iter()
            .map(|&(lobe, lobe_r)| (lobe - at).len() + lobe_r)
            .fold(r, f32::max);
        if keep.allows(at, span, 140.0) && clear_of_land(&land, at, span, 140.0) {
            land.push(disc(at, r));
            land.extend(lobes.into_iter().map(|(lobe, lobe_r)| disc(lobe, lobe_r)));
            islets += 1;
        }
    }
    let mut hidden_islands: Vec<HiddenIsland> = Vec::new();
    for _ in 0..2000 {
        if hidden_islands.len() == HIDDEN_NAMES.len() {
            break;
        }
        let at = mid + V::polar(rng.range(0.0, TAU), rng.range(700.0, 2000.0));
        let r = rng.range(45.0, 60.0);
        let spaced = hidden_islands
            .iter()
            .all(|h| (at - V(h.x, h.y)).len() > 400.0);
        if !(spaced && keep.allows(at, r, 120.0) && clear_of_land(&land, at, r, 160.0)) {
            continue;
        }
        let dig = at + (mid - at).norm() * (r + 15.0);
        hidden_islands.push(HiddenIsland {
            id: hidden_islands.len() as u32 + 1,
            name: HIDDEN_NAMES[hidden_islands.len()],
            x: at.0,
            y: at.1,
            radius: r,
            dig_x: dig.0,
            dig_y: dig.1,
        });
    }

    // 5. Zonas, em ordem de prioridade: águas das capitais, rotas de
    // fronteira, águas da ilha, Águas Negras, instâncias e o alto-mar.
    let mut zones = vec![
        zone(
            "Águas do Porto da Serra",
            RiskTier::Protected,
            serra.0,
            serra.1,
            PROTECTED_RADIUS,
        ),
        zone(
            "Águas do Porto da Mina",
            RiskTier::Protected,
            mina.0,
            mina.1,
            PROTECTED_RADIUS,
        ),
    ];
    let coast_route = (mina - serra).len() - 600.0;
    let stops = ((coast_route / 300.0).round() as usize).max(1);
    for k in 0..=stops {
        let at = serra + lane * (300.0 + coast_route * k as f32 / stops as f32);
        zones.push(zone("Rota da Costa", RiskTier::Frontier, at.0, at.1, 120.0));
    }
    for (name, from) in [
        ("Corredor do Amanhecer", serra),
        ("Corredor do Poente", mina),
    ] {
        // Termina antes das águas da ilha: ali quem manda é o sem lei.
        for at in chain(from, island, 225.0)
            .into_iter()
            .filter(|at| (*at - island).len() > 260.0)
        {
            zones.push(zone(name, RiskTier::Frontier, at.0, at.1, 140.0));
        }
    }
    zones.push(zone(
        "Águas da Ilha do Coral Negro",
        RiskTier::Lawless,
        island.0,
        island.1,
        350.0,
    ));
    zones.push(zone(
        BLACK_WATERS,
        RiskTier::Lawless,
        black_waters.0,
        black_waters.1,
        BLACK_WATERS_RADIUS,
    ));
    zones.extend(instance_and_open_sea_zones());

    let port = |name, at: V| Port {
        name,
        x: at.0,
        y: at.1,
        service_radius: 60.0,
    };
    let regions = vec![
        region(SERRA, port(SERRA, serra)),
        region(MINA, port(MINA, mina)),
        region(ISLAND, port(PIRATE_PORT, pirate_port)),
    ];

    // 6. Caixa do mar principal, antes de juntar a terra das instâncias.
    let mut bounds = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    let hidden_land: Vec<LandMass> = hidden_islands.iter().map(HiddenIsland::land).collect();
    for m in land
        .iter()
        .chain(&hidden_land)
        .chain(&[disc(black_waters, BLACK_WATERS_RADIUS)])
    {
        bounds.0 = bounds.0.min(m.x - m.radius);
        bounds.1 = bounds.1.max(m.x + m.radius);
        bounds.2 = bounds.2.min(m.y - m.radius);
        bounds.3 = bounds.3.max(m.y + m.radius);
    }
    land.extend(instance_land());
    nodes.extend(instance_nodes());

    let features = Features {
        seed,
        spawn: serra_dock.t(),
        bounds,
        hidden_islands,
        tempest_sites: tempest_sites.into_iter().map(V::t).collect(),
        kraken_sites: kraken_sites.into_iter().map(V::t).collect(),
        tide_sites: tide_sites.into_iter().map(V::t).collect(),
        fleet_route: fleet_route.into_iter().map(V::t).collect(),
        nodes,
        pirate_spawns: pirate_spawns.into_iter().map(V::t).collect(),
        raider_spawns: raider_spawns.into_iter().map(V::t).collect(),
        navy_spawns: navy_spawns.into_iter().map(V::t).collect(),
        caravan_route: caravan_route.into_iter().map(V::t).collect(),
        whirlpool_sectors: [
            sector(serra + n * 150.0, 450.0),
            sector(mina + n * 150.0, 450.0),
            sector(black_waters, 450.0),
        ],
        labels: vec![
            ("Ilha do Coral Negro", isl(0.0, -45.0).0, isl(0.0, -45.0).1),
            ("ÁGUAS NEGRAS", isl(0.0, 455.0).0, isl(0.0, 455.0).1),
            maelstrom_label(),
        ],
    };
    WorldMap::assemble(zones, regions, land, features)
}

fn disc(at: V, radius: f32) -> LandMass {
    LandMass {
        x: at.0,
        y: at.1,
        radius,
    }
}

fn node(name: &'static str, region: &'static str, at: V, max_stock: u32) -> NodeSpot {
    NodeSpot {
        name,
        region,
        x: at.0,
        y: at.1,
        max_stock,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SeaEventKind;

    const SEEDS: std::ops::Range<u64> = 1..400;

    fn tier(map: &WorldMap, (x, y): (f32, f32)) -> RiskTier {
        map.zone_at(x, y).expect("todo o mar é declarado").tier
    }

    #[test]
    fn same_seed_same_world_and_seeds_differ() {
        // Zonas ganham ids aleatórios; o mundo é a terra, as regiões e o
        // conteúdo.
        let same = |a: &WorldMap, b: &WorldMap| {
            a.land() == b.land() && a.regions() == b.regions() && a.features() == b.features()
        };
        assert!(same(&generate(7), &generate(7)));
        assert_ne!(generate(7).land(), generate(8).land());
        assert!(same(&WorldMap::from_seed(0), &WorldMap::vertical_slice()));
    }

    #[test]
    fn capitals_are_protected_open_water_and_spawn_docks_there() {
        for seed in SEEDS {
            let map = generate(seed);
            for region in map.regions() {
                let port = region.port.as_ref().unwrap();
                assert!(
                    map.push_out_of_land(port.x, port.y, 20.0).is_none(),
                    "seed {seed}: {} sem água no cais",
                    port.name
                );
            }
            let spawn = map.features().spawn;
            assert_eq!(tier(&map, spawn), RiskTier::Protected, "seed {seed}");
            assert!(map.push_out_of_land(spawn.0, spawn.1, 20.0).is_none());
            let serra = map.region_by_name(SERRA).unwrap().port.as_ref().unwrap();
            assert!(serra.contains(spawn.0, spawn.1), "seed {seed}");
            let pirate = map.region_by_name(ISLAND).unwrap().port.as_ref().unwrap();
            assert_eq!(pirate.name, PIRATE_PORT);
            assert_eq!(tier(&map, (pirate.x, pirate.y)), RiskTier::Lawless);
        }
    }

    #[test]
    fn routes_never_scrape_land_even_with_hidden_islands() {
        for seed in SEEDS {
            let map = generate(seed).with_hidden_islands();
            let f = map.features();
            for (route, clearance) in [(&f.fleet_route, 40.0), (&f.caravan_route, 20.0)] {
                for leg in route.windows(2) {
                    let ((ax, ay), (bx, by)) = (leg[0], leg[1]);
                    for step in 0..=20 {
                        let t = step as f32 / 20.0;
                        let (x, y) = (ax + (bx - ax) * t, ay + (by - ay) * t);
                        assert!(
                            map.push_out_of_land(x, y, clearance).is_none(),
                            "seed {seed}: rota raspa terra em {x},{y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn frontier_chains_are_water_and_reach_the_island() {
        for seed in SEEDS {
            let map = generate(seed).with_hidden_islands();
            for zone in map.zones().iter().filter(|z| z.tier == RiskTier::Frontier) {
                let crate::zone::ZoneShape::Circle { x, y, .. } = zone.shape;
                assert!(
                    map.push_out_of_land(x, y, 20.0).is_none(),
                    "seed {seed}: {} em terra",
                    zone.name
                );
            }
            // Porto -> ilha pelo corredor: nunca cai fora das zonas nem vira
            // alto-mar no meio do caminho.
            let island = map.region_by_name(ISLAND).unwrap().port.as_ref().unwrap();
            for name in [SERRA, MINA] {
                let port = map.region_by_name(name).unwrap().port.as_ref().unwrap();
                for step in 0..=40 {
                    let t = step as f32 / 40.0;
                    let x = port.x + (island.x - port.x) * t;
                    let y = port.y + (island.y - port.y) * t;
                    assert_ne!(
                        map.zone_at(x, y).unwrap().name,
                        "Mar Sem Lei",
                        "seed {seed}: corredor de {name} tem brecha em {x},{y}"
                    );
                }
            }
        }
    }

    #[test]
    fn content_points_are_open_water_in_the_right_waters() {
        for seed in SEEDS {
            let map = generate(seed).with_hidden_islands();
            let f = map.features();
            for kind in SeaEventKind::ALL {
                for &at in f.event_sites(kind) {
                    assert_ne!(tier(&map, at), RiskTier::Protected, "seed {seed} {kind:?}");
                    assert!(!map.is_land(at.0, at.1), "seed {seed} {kind:?} em terra");
                }
            }
            for &at in &f.pirate_spawns {
                assert_eq!(tier(&map, at), RiskTier::Lawless, "seed {seed}");
                assert!(!map.is_land(at.0, at.1), "seed {seed}");
            }
            for &at in f.raider_spawns.iter().chain(&f.navy_spawns) {
                assert!(
                    map.push_out_of_land(at.0, at.1, 20.0).is_none(),
                    "seed {seed}"
                );
            }
            for spot in &f.nodes {
                assert!(
                    map.push_out_of_land(spot.x, spot.y, 10.0).is_none(),
                    "seed {seed}: nó {} em terra",
                    spot.name
                );
                assert!(map.region_by_name(spot.region).is_ok());
            }
        }
    }

    #[test]
    fn hidden_islands_are_risky_reachable_and_off_the_charts() {
        for seed in SEEDS {
            let map = generate(seed);
            let f = map.features();
            assert_eq!(f.hidden_islands.len(), 4, "seed {seed}");
            for island in &f.hidden_islands {
                assert_ne!(tier(&map, (island.x, island.y)), RiskTier::Protected);
                assert!(
                    !map.is_land(island.x, island.y),
                    "carta mostra {}",
                    island.name
                );
                assert!(!map.is_land(island.dig_x, island.dig_y));
                assert!(!island.land().contains(island.dig_x, island.dig_y, 0.0));
                assert!(island.at_dig_spot(island.dig_x, island.dig_y));
            }
        }
    }

    #[test]
    fn main_sea_stays_clear_of_instances_and_the_sea_quad() {
        // O client trata terra com |x| > 3000 como parede de instância e o
        // quadro do oceano cobre y de -2900 a 3500.
        for seed in SEEDS {
            let map = generate(seed).with_hidden_islands();
            let (x0, x1, y0, y1) = map.features().bounds;
            assert!(x0 > -3000.0 && x1 < 3000.0, "seed {seed}: {x0}..{x1}");
            assert!(y0 > -2900.0 && y1 < 3500.0, "seed {seed}: {y0}..{y1}");
            let land_main = map.land().iter().filter(|m| m.x.abs() < 3000.0).count();
            assert!(land_main >= 30, "seed {seed}: mundo pelado ({land_main})");
        }
    }
}
