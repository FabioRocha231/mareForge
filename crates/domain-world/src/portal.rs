//! Portais (MF-059): Cerração e Sorvedouros. Regra pura e determinística
//! (semente): o
//! servidor chama [`PortalDirector::tick`] no loop e [`PortalDirector::transit`]
//! para cada navio; o client só desenha o que recebe.
//!
//! - **Cerração**: um banco de névoa surge no mar arriscado por tempo limitado
//!   e com lotação (dupla). Leva a uma arena isolada; quando a arena fecha,
//!   quem ficou dentro é cuspido de volta onde a cerração nasceu.
//! - **Sorvedouro**: três redemoinhos no mapa ligam aos três
//!   pontos da Passagem; a rede se reembaralha periodicamente. Entrar por um
//!   e sair por outro é atalho — por dentro de águas sem lei.

use std::collections::HashMap;

use crate::map::{WorldMap, FOG_RADIUS, FOG_SLOTS, MAELSTROM_POINTS};
use crate::risk::RiskTier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalKind {
    /// Entrada de cerração no mapa principal.
    FogGate,
    /// Saída de dentro da cerração, de volta à origem.
    FogExit,
    /// Sorvedouro (dos dois lados).
    Whirlpool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Portal {
    pub id: u32,
    pub kind: PortalKind,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    /// Onde o navio reaparece.
    pub dest: (f32, f32),
    /// Tempo de simulação (s) em que o portal some.
    pub expires_at: f64,
    /// Travessias restantes; `None` = ilimitado.
    pub uses_left: Option<u32>,
}

impl Portal {
    pub fn catches(&self, x: f32, y: f32) -> bool {
        let (dx, dy) = (x - self.x, y - self.y);
        dx * dx + dy * dy <= self.radius * self.radius
    }
}

/// Tuning das cerrações e de Sorvedouro (segundos, metros).
#[derive(Debug, Clone, Copy)]
pub struct PortalTuning {
    pub fog_check_every: f64,
    pub max_fog_gates: usize,
    pub fog_gate_lifetime: f64,
    pub fog_gate_uses: u32,
    pub fog_arena_lifetime: f64,
    pub maelstrom_reroll_every: f64,
    pub whirlpool_uses: u32,
    pub transit_cooldown: f64,
}

impl Default for PortalTuning {
    fn default() -> Self {
        Self {
            fog_check_every: 45.0,
            max_fog_gates: 2,
            fog_gate_lifetime: 240.0,
            fog_gate_uses: 2,
            fog_arena_lifetime: 600.0,
            maelstrom_reroll_every: 600.0,
            whirlpool_uses: 10,
            transit_cooldown: 5.0,
        }
    }
}

/// Arena de cerração ocupada: onde devolve quem sai (ao lado do portão) e
/// quando fecha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogArena {
    pub origin: (f32, f32),
    pub closes_at: f64,
}

/// Arena que fechou neste tick: o servidor devolve quem estiver dentro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClosedArena {
    pub center: (f32, f32),
    pub origin: (f32, f32),
}

/// xorshift64*: suficiente para sortear posições, reproduzível em teste.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let v = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (v >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }
}

#[derive(Debug, Clone)]
pub struct PortalDirector {
    pub tuning: PortalTuning,
    portals: Vec<Portal>,
    arenas: [Option<FogArena>; FOG_SLOTS.len()],
    next_id: u32,
    next_fog_check: f64,
    next_reroll: f64,
    cooldowns: HashMap<u32, f64>,
    rng: Rng,
}

impl PortalDirector {
    pub fn new(seed: u64, tuning: PortalTuning) -> Self {
        Self {
            tuning,
            portals: Vec::new(),
            arenas: [None; FOG_SLOTS.len()],
            next_id: 1,
            next_fog_check: 0.0,
            next_reroll: 0.0,
            cooldowns: HashMap::new(),
            rng: Rng::new(seed),
        }
    }

    pub fn portals(&self) -> &[Portal] {
        &self.portals
    }

    pub fn arenas(&self) -> &[Option<FogArena>] {
        &self.arenas
    }

    /// Avança o relógio: expira portais, fecha arenas, abre cerrações novas e
    /// reembaralha Sorvedouro. Devolve as arenas que fecharam agora.
    pub fn tick(&mut self, now: f64, map: &WorldMap) -> Vec<ClosedArena> {
        self.portals.retain(|p| p.expires_at > now);
        self.cooldowns.retain(|_, until| *until > now);
        let mut closed = Vec::new();
        for (slot, arena) in self.arenas.iter_mut().enumerate() {
            if let Some(open) = *arena {
                if open.closes_at <= now {
                    *arena = None;
                    closed.push(ClosedArena {
                        center: FOG_SLOTS[slot],
                        origin: open.origin,
                    });
                }
            }
        }
        if now >= self.next_reroll {
            self.next_reroll = now + self.tuning.maelstrom_reroll_every;
            self.reroll_maelstrom(now, map);
        }
        if now >= self.next_fog_check {
            self.next_fog_check = now + self.tuning.fog_check_every;
            self.try_open_fog(now, map);
        }
        closed
    }

    /// O navio `ship_id` em (x, y) atravessa algum portal? Devolve o destino
    /// e consome uma travessia. Recém-chegado tem carência para não quicar.
    pub fn transit(&mut self, ship_id: u32, x: f32, y: f32, now: f64) -> Option<(f32, f32)> {
        if self
            .cooldowns
            .get(&ship_id)
            .is_some_and(|until| *until > now)
        {
            return None;
        }
        let index = self.portals.iter().position(|p| p.catches(x, y))?;
        let portal = &mut self.portals[index];
        let dest = portal.dest;
        portal.uses_left = portal.uses_left.map(|uses| uses.saturating_sub(1));
        if portal.uses_left == Some(0) {
            self.portals.remove(index);
        }
        self.cooldowns
            .insert(ship_id, now + self.tuning.transit_cooldown);
        Some(dest)
    }

    /// (x, y) está numa arena de cerração que não está aberta? Acontece com
    /// quem desconectou lá dentro e voltou depois do colapso (ou após restart).
    pub fn stranded(&self, x: f32, y: f32) -> bool {
        FOG_SLOTS.iter().zip(&self.arenas).any(|(center, arena)| {
            arena.is_none()
                && (x - center.0).powi(2) + (y - center.1).powi(2) <= (FOG_RADIUS + 80.0).powi(2)
        })
    }

    /// Carência sem travessia — para quem foi devolvido pela arena fechando
    /// não cair direto num portão novo.
    pub fn hold(&mut self, ship_id: u32, now: f64) {
        self.cooldowns
            .insert(ship_id, now + self.tuning.transit_cooldown);
    }

    fn id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn try_open_fog(&mut self, now: f64, map: &WorldMap) {
        let gates = self
            .portals
            .iter()
            .filter(|p| p.kind == PortalKind::FogGate)
            .count();
        if gates >= self.tuning.max_fog_gates {
            return;
        }
        let Some(slot) = self.arenas.iter().position(Option::is_none) else {
            return;
        };
        let sectors = &map.features().sea_sectors;
        let pick = (self.rng.next_f32() * sectors.len() as f32) as usize;
        let Some(&sector) = sectors.get(pick.min(sectors.len().saturating_sub(1))) else {
            return;
        };
        let Some(origin) = self.sample_open_sea(map, sector) else {
            return;
        };
        let center = FOG_SLOTS[slot];
        let closes_at = now + self.tuning.fog_arena_lifetime;
        // Volta ao lado do portão, não em cima dele.
        let landing = landing_near(map, origin, 90.0);
        self.arenas[slot] = Some(FogArena {
            origin: landing,
            closes_at,
        });
        let gate = Portal {
            id: self.id(),
            kind: PortalKind::FogGate,
            x: origin.0,
            y: origin.1,
            radius: 35.0,
            dest: (center.0, center.1 - 250.0),
            // Portão nunca sobrevive à arena para onde leva.
            expires_at: (now + self.tuning.fog_gate_lifetime).min(closes_at),
            uses_left: Some(self.tuning.fog_gate_uses),
        };
        let exit = Portal {
            id: self.id(),
            kind: PortalKind::FogExit,
            x: center.0,
            y: center.1,
            radius: 35.0,
            dest: landing,
            expires_at: closes_at,
            uses_left: None,
        };
        self.portals.push(gate);
        self.portals.push(exit);
    }

    fn reroll_maelstrom(&mut self, now: f64, map: &WorldMap) {
        self.portals.retain(|p| p.kind != PortalKind::Whirlpool);
        let expires_at = now + self.tuning.maelstrom_reroll_every;
        for (i, sector) in map.features().whirlpool_sectors.into_iter().enumerate() {
            let Some(world) = self.sample_open_sea(map, sector) else {
                continue;
            };
            let inside = MAELSTROM_POINTS[i];
            let world_side = Portal {
                id: self.id(),
                kind: PortalKind::Whirlpool,
                x: world.0,
                y: world.1,
                radius: 40.0,
                dest: (inside.0 + 110.0, inside.1),
                expires_at,
                uses_left: Some(self.tuning.whirlpool_uses),
            };
            let passage_side = Portal {
                id: self.id(),
                kind: PortalKind::Whirlpool,
                x: inside.0,
                y: inside.1,
                radius: 40.0,
                dest: landing_near(map, world, 100.0),
                expires_at,
                uses_left: None,
            };
            self.portals.push(world_side);
            self.portals.push(passage_side);
        }
    }

    /// Ponto de mar aberto arriscado (fronteira/sem lei), longe de terra e de
    /// outros portais. `None` se o sorteio não achar lugar (tenta de novo no
    /// próximo ciclo).
    fn sample_open_sea(
        &mut self,
        map: &WorldMap,
        (x0, x1, y0, y1): (f32, f32, f32, f32),
    ) -> Option<(f32, f32)> {
        for _ in 0..40 {
            let (x, y) = (self.rng.range(x0, x1), self.rng.range(y0, y1));
            let risky = map
                .zone_at(x, y)
                .is_ok_and(|zone| zone.tier != RiskTier::Protected);
            let dry = map.push_out_of_land(x, y, 90.0).is_none();
            let spaced = self.portals.iter().all(|p| {
                let (dx, dy) = (p.x - x, p.y - y);
                dx * dx + dy * dy > 250.0 * 250.0
            });
            if risky && dry && spaced {
                return Some((x, y));
            }
        }
        None
    }
}

/// Um ponto de água a `distance` de `at` (primeira direção livre de terra).
fn landing_near(map: &WorldMap, at: (f32, f32), distance: f32) -> (f32, f32) {
    (0..8)
        .map(|k| {
            let a = k as f32 * std::f32::consts::FRAC_PI_4 - std::f32::consts::FRAC_PI_2;
            (at.0 + a.cos() * distance, at.1 + a.sin() * distance)
        })
        .find(|(x, y)| map.push_out_of_land(*x, *y, 25.0).is_none())
        .unwrap_or(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{FOG_ZONE, MAELSTROM_ZONE};

    fn director() -> (PortalDirector, WorldMap) {
        (
            PortalDirector::new(7, PortalTuning::default()),
            WorldMap::vertical_slice(),
        )
    }

    #[test]
    fn first_tick_opens_a_fog_bank_and_the_maelstrom_network() {
        let (mut d, map) = director();
        d.tick(0.0, &map);
        let count = |kind| d.portals().iter().filter(|p| p.kind == kind).count();
        assert_eq!(count(PortalKind::FogGate), 1);
        assert_eq!(count(PortalKind::FogExit), 1);
        assert_eq!(count(PortalKind::Whirlpool), 6);
        // Cada redemoinho do mundo leva para dentro da Passagem.
        for p in d.portals() {
            let dest_zone = map.zone_at(p.dest.0, p.dest.1).unwrap().name;
            let here_zone = map.zone_at(p.x, p.y).unwrap().name;
            match p.kind {
                PortalKind::FogGate => assert_eq!(dest_zone, FOG_ZONE),
                PortalKind::FogExit => assert_eq!(here_zone, FOG_ZONE),
                PortalKind::Whirlpool if here_zone == MAELSTROM_ZONE => {
                    assert_ne!(dest_zone, MAELSTROM_ZONE)
                }
                PortalKind::Whirlpool => assert_eq!(dest_zone, MAELSTROM_ZONE),
            }
        }
    }

    #[test]
    fn portals_never_spawn_on_land_or_in_protected_waters() {
        for seed in 1..40 {
            let mut d = PortalDirector::new(seed, PortalTuning::default());
            let map = WorldMap::vertical_slice();
            d.tick(0.0, &map);
            for p in d.portals() {
                assert!(!map.is_land(p.x, p.y), "seed {seed}: {p:?}");
                assert_ne!(map.zone_at(p.x, p.y).unwrap().tier, RiskTier::Protected);
                assert!(map.push_out_of_land(p.dest.0, p.dest.1, 20.0).is_none());
            }
        }
    }

    #[test]
    fn fog_gate_is_a_duo_and_transit_has_a_cooldown() {
        let (mut d, map) = director();
        d.tick(0.0, &map);
        let gate = d
            .portals()
            .iter()
            .find(|p| p.kind == PortalKind::FogGate)
            .unwrap()
            .clone();
        assert_eq!(d.transit(1, gate.x, gate.y, 1.0), Some(gate.dest));
        // Recém-chegado não atravessa de novo na carência.
        assert_eq!(d.transit(1, gate.x, gate.y, 2.0), None);
        assert_eq!(d.transit(2, gate.x, gate.y, 2.0), Some(gate.dest));
        // Lotação esgotada: a cerração se fecha para o terceiro.
        assert_eq!(d.transit(3, gate.x, gate.y, 2.0), None);
        assert!(d.portals().iter().all(|p| p.id != gate.id));
        // Arena aberta não prende ninguém; carência expirada é esquecida.
        assert!(!d.stranded(gate.dest.0, gate.dest.1));
        d.hold(9, 2.0);
        d.tick(100.0, &map);
        assert!(d.cooldowns.is_empty());
    }

    #[test]
    fn arena_closes_and_reports_where_to_eject() {
        let (mut d, map) = director();
        d.tick(0.0, &map);
        let open = d.arenas()[0].expect("arena aberta");
        let closed = d.tick(open.closes_at + 1.0, &map);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].origin, open.origin);
        assert_eq!(closed[0].center, FOG_SLOTS[0]);
        // Quem está numa arena que não está aberta ficou encalhado.
        let free = d
            .arenas()
            .iter()
            .position(Option::is_none)
            .expect("slot livre");
        assert!(d.stranded(FOG_SLOTS[free].0, FOG_SLOTS[free].1 - 250.0));
        assert!(!d.stranded(0.0, 0.0));
        assert!(d
            .portals()
            .iter()
            .all(|p| p.kind != PortalKind::FogExit || p.expires_at > open.closes_at + 1.0));
    }

    #[test]
    fn maelstrom_rerolls_to_new_places() {
        let (mut d, map) = director();
        d.tick(0.0, &map);
        let before: Vec<(f32, f32)> = d
            .portals()
            .iter()
            .filter(|p| p.kind == PortalKind::Whirlpool)
            .map(|p| (p.x, p.y))
            .collect();
        d.tick(PortalTuning::default().maelstrom_reroll_every + 0.1, &map);
        let after: Vec<(f32, f32)> = d
            .portals()
            .iter()
            .filter(|p| p.kind == PortalKind::Whirlpool)
            .map(|p| (p.x, p.y))
            .collect();
        assert_eq!(after.len(), 6);
        assert_ne!(before, after);
    }
}
