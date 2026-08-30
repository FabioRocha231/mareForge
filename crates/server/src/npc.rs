//! NPC naval mínimo (MF-044/045): navios transitórios que patrulham,
//! perseguem e atacam jogadores, sem loot de item (Pilar 1). A morte
//! concede bounty auditado em Gold, registrado como `LedgerKind::NpcBounty`.

use std::collections::HashMap;

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_combat::{
    apply_damage, BroadsideBattery, BroadsideSide, DamageOutcome, Projectile, WeaponParams,
};
use mareforge_domain_economy::{LedgerKind, Money};
use mareforge_domain_items::{CargoHold, ItemCatalog};
use mareforge_domain_ships::{
    compute_ship_stats, step_motion, EquippedComponents, MotionInput, MotionTuning, ShipKind,
    ShipMotion, ShipStats, VesselPresence,
};
use mareforge_domain_world::{RiskTier, WorldMap};
use mareforge_protocol::{ShipState, WalletUpdated};
use mareforge_shared::ids::{CharacterId, ShipInstanceId, ZoneId};
use tracing::info;

use crate::crafting::DevShips;
use crate::net::{
    CombatTuning, ProjectileIdCounter, ServerProjectile, ServerRiskPolicy, ServerShip,
    ServerWorldMap,
};

/// Raio padrão de patrulha ao redor do ponto de spawn.
const PATROL_RADIUS: f32 = 120.0;
/// Separador de ids: NPCs não compartilham o espaço de `ShipIdCounter` para
/// que `Projectile.owner_ship_id` não seja ambíguo.
const NPC_ID_OFFSET: u32 = 1_000_000;

#[derive(Component)]
pub struct NpcShip {
    pub ship_id: u32,
    pub kind: ShipKind,
    pub hp: u32,
    pub max_hp: u32,
    /// NPCs nunca atracam.
    pub presence: VesselPresence,
    /// NPCs nunca carregam item; mantido para shape de `ShipState`.
    pub hold: CargoHold,
    pub battery: BroadsideBattery,
    pub stats: ShipStats,
    pub motion: ShipMotion,
    pub tuning: MotionTuning,
    pub zone: Option<ZoneId>,
    pub ai: NpcAi,
    /// MF-045: personagem do jogador que deu o golpe final.
    pub last_damage_dealer: Option<CharacterId>,
    /// Alvo atual enquanto Chase/Attack (Attack não guarda o id no estado).
    pub last_target: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NpcState {
    Idle,
    Patrol { origin: (f32, f32), radius: f32 },
    Chase { target: u32 },
    Attack,
    Dead,
}

#[derive(Debug, Clone)]
pub struct NpcAi {
    pub state: NpcState,
    pub detection_radius: f32,
    pub weapon_range: f32,
    pub respawn_after_secs: f32,
    pub bounty_gold: u64,
    pub spawn_position: (f32, f32),
}

#[derive(Resource, Default)]
pub struct NpcIdCounter(pub u32);

#[derive(Resource, Debug, Clone)]
pub struct NpcSpawnConfig {
    pub count: usize,
    pub spawn_positions: Vec<(f32, f32)>,
    pub respawn_after_secs: f32,
}

impl Default for NpcSpawnConfig {
    fn default() -> Self {
        Self {
            count: 3,
            // Águas da Ilha do Coral Negro: lawless e longe dos portos.
            spawn_positions: vec![(0.0, 900.0), (-160.0, 850.0), (160.0, 950.0)],
            respawn_after_secs: 30.0,
        }
    }
}

/// NPCs mortos aguardando respawn: (tempo restante, kind, posição original).
#[derive(Resource, Default)]
pub struct NpcRespawnQueue(pub Vec<(f32, ShipKind, (f32, f32))>);

pub fn setup_npcs(
    mut commands: Commands,
    dev_ships: Res<DevShips>,
    map: Res<ServerWorldMap>,
    config: Res<NpcSpawnConfig>,
    mut ids: ResMut<NpcIdCounter>,
) {
    for position in config.spawn_positions.iter().take(config.count).copied() {
        let ship_id = spawn_npc(
            &mut commands,
            &dev_ships,
            &map.0,
            &config,
            &mut ids,
            ShipKind::Corsair,
            position,
        );
        info!(
            ship_id,
            x = position.0,
            y = position.1,
            "NPC corsário no mar"
        );
    }
}

pub fn respawn_npcs(
    mut commands: Commands,
    mut queue: ResMut<NpcRespawnQueue>,
    mut ids: ResMut<NpcIdCounter>,
    dev_ships: Res<DevShips>,
    map: Res<ServerWorldMap>,
    config: Res<NpcSpawnConfig>,
    time: Res<Time>,
) {
    if queue.0.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    for pending in &mut queue.0 {
        pending.0 -= dt;
    }
    let ready: Vec<(ShipKind, (f32, f32))> = queue
        .0
        .iter()
        .filter(|pending| pending.0 <= 0.0)
        .map(|pending| (pending.1, pending.2))
        .collect();
    queue.0.retain(|pending| pending.0 > 0.0);
    for (kind, position) in ready {
        let ship_id = spawn_npc(
            &mut commands,
            &dev_ships,
            &map.0,
            &config,
            &mut ids,
            kind,
            position,
        );
        info!(ship_id, x = position.0, y = position.1, "NPC respawnou");
    }
}

pub(crate) fn build_npc(
    dev_ships: &DevShips,
    map: &WorldMap,
    config: &NpcSpawnConfig,
    ids: &mut NpcIdCounter,
    kind: ShipKind,
    position: (f32, f32),
) -> (u32, NpcShip) {
    let ship_id = next_npc_id(ids);
    let definition = dev_ships.definition(kind).clone();
    let stats = compute_ship_stats(
        &definition,
        &EquippedComponents::default(),
        &ItemCatalog::default(),
    )
    .expect("stats de navio sem equipamento não podem falhar");
    let max_hp = stats.max_hp;
    let cargo_capacity = stats.cargo_capacity;
    let weapon_range = stats.weapon_range;
    let ship = NpcShip {
        ship_id,
        kind,
        hp: max_hp,
        max_hp,
        presence: VesselPresence::AtSea,
        hold: CargoHold::new(ShipInstanceId::new(), cargo_capacity),
        battery: BroadsideBattery::default(),
        stats,
        motion: ShipMotion {
            x: position.0,
            y: position.1,
            ..ShipMotion::default()
        },
        tuning: MotionTuning::default(),
        zone: map.zone_at(position.0, position.1).ok().map(|zone| zone.id),
        ai: NpcAi {
            state: NpcState::Patrol {
                origin: position,
                radius: PATROL_RADIUS,
            },
            detection_radius: 300.0,
            weapon_range,
            respawn_after_secs: config.respawn_after_secs,
            bounty_gold: 50,
            spawn_position: position,
        },
        last_damage_dealer: None,
        last_target: None,
    };
    (ship_id, ship)
}

fn spawn_npc(
    commands: &mut Commands,
    dev_ships: &DevShips,
    map: &WorldMap,
    config: &NpcSpawnConfig,
    ids: &mut NpcIdCounter,
    kind: ShipKind,
    position: (f32, f32),
) -> u32 {
    let (ship_id, ship) = build_npc(dev_ships, map, config, ids, kind, position);
    commands.spawn((ship,));
    ship_id
}

fn next_npc_id(ids: &mut NpcIdCounter) -> u32 {
    let ship_id = NPC_ID_OFFSET + ids.0;
    ids.0 += 1;
    ship_id
}

#[allow(clippy::too_many_arguments)]
pub fn simulate_npcs(
    mut commands: Commands,
    mut connection_manager: ResMut<ConnectionManager>,
    mut npcs: Query<(Entity, &mut NpcShip)>,
    ships: Query<&ServerShip>,
    projectiles: Query<(Entity, &ServerProjectile)>,
    map: Res<ServerWorldMap>,
    risk_policy: Res<ServerRiskPolicy>,
    tuning: Res<CombatTuning>,
    mut projectile_ids: ResMut<ProjectileIdCounter>,
    mut market: ResMut<crate::market::ServerMarket>,
    mut metrics: ResMut<crate::net::Metrics>,
    mut npc_respawns: ResMut<NpcRespawnQueue>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let players: Vec<(u32, ShipMotion)> = ships
        .iter()
        .filter(|ship| ship.client_id.is_some())
        .map(|ship| (ship.ship_id, ship.motion))
        .collect();
    let player_positions: HashMap<u32, (f32, f32)> = players
        .iter()
        .map(|(ship_id, motion)| (*ship_id, (motion.x, motion.y)))
        .collect();
    let player_owners: HashMap<u32, CharacterId> = ships
        .iter()
        .map(|ship| (ship.ship_id, ship.character))
        .collect();
    let viewers: Vec<(Option<ClientId>, CharacterId)> = ships
        .iter()
        .map(|ship| (ship.client_id, ship.character))
        .collect();

    for (entity, mut npc) in &mut npcs {
        if npc.ai.state == NpcState::Dead {
            commands.entity(entity).despawn();
            continue;
        }
        npc.battery.advance(dt);
        let protected = in_protected_area(&map.0, npc.motion.x, npc.motion.y);
        if protected {
            npc.ai.state = NpcState::Patrol {
                origin: npc.ai.spawn_position,
                radius: PATROL_RADIUS,
            };
            npc.last_target = None;
        }

        match &npc.ai.state {
            NpcState::Idle | NpcState::Patrol { .. } => {
                if !protected {
                    if let Some(target) = nearest_player(
                        &players,
                        npc.motion.x,
                        npc.motion.y,
                        npc.ai.detection_radius,
                    ) {
                        npc.last_target = Some(target);
                        npc.ai.state = NpcState::Chase { target };
                        continue;
                    }
                }
                if let NpcState::Patrol { origin, radius } = &npc.ai.state {
                    let input = patrol_input(npc.motion, *origin, *radius);
                    let NpcShip {
                        motion,
                        stats,
                        tuning,
                        ..
                    } = &mut *npc;
                    step_motion(motion, stats, input, tuning, dt);
                }
            }
            NpcState::Chase { target } => {
                if let Some((_, target_motion)) = players.iter().find(|(id, _)| id == target) {
                    let dist =
                        distance(npc.motion.x, npc.motion.y, target_motion.x, target_motion.y);
                    if dist <= npc.ai.weapon_range {
                        npc.ai.state = NpcState::Attack;
                    } else {
                        npc.last_target = Some(*target);
                        let input = steer_input(npc.motion, target_motion.x, target_motion.y);
                        let NpcShip {
                            motion,
                            stats,
                            tuning,
                            ..
                        } = &mut *npc;
                        step_motion(motion, stats, input, tuning, dt);
                    }
                } else {
                    npc.ai.state = NpcState::Patrol {
                        origin: npc.ai.spawn_position,
                        radius: PATROL_RADIUS,
                    };
                    npc.last_target = None;
                }
            }
            NpcState::Attack => {
                let target_id = npc.last_target;
                let Some((_, target_motion)) =
                    target_id.and_then(|target| players.iter().find(|(id, _)| *id == target))
                else {
                    npc.ai.state = NpcState::Patrol {
                        origin: npc.ai.spawn_position,
                        radius: PATROL_RADIUS,
                    };
                    npc.last_target = None;
                    continue;
                };
                let dist = distance(npc.motion.x, npc.motion.y, target_motion.x, target_motion.y);
                if dist > npc.ai.weapon_range {
                    if let Some(target) = target_id {
                        npc.ai.state = NpcState::Chase { target };
                    }
                } else {
                    let side = side_for_target(
                        npc.motion.heading,
                        target_motion.x - npc.motion.x,
                        target_motion.y - npc.motion.y,
                    );
                    if npc.battery.try_fire(side, tuning.cooldown_secs) {
                        spawn_projectile(&mut commands, &mut projectile_ids, &npc, side, &tuning);
                    }
                }
            }
            NpcState::Dead => {
                commands.entity(entity).despawn();
            }
        }
    }

    // Impactos em NPCs: projéteis que NÃO acertaram jogador nesta passada.
    // O dano usa o mesmo `apply_damage`; NPC morto não vira wreck (Pilar 1).
    let npc_positions: HashMap<u32, (f32, f32)> = npcs
        .iter()
        .map(|(_, npc)| (npc.ship_id, (npc.motion.x, npc.motion.y)))
        .collect();
    let mut npc_impacts: Vec<(Entity, u32, u32, u32)> = Vec::new();
    for (projectile_entity, projectile) in &projectiles {
        if projectile.0.expired() {
            continue;
        }
        let hits_player = player_positions.iter().any(|(ship_id, (x, y))| {
            *ship_id != projectile.0.owner_ship_id
                && projectile.0.hit_ship(*x, *y, tuning.hit_radius)
        });
        if hits_player {
            continue;
        }
        for (npc_id, (x, y)) in &npc_positions {
            if *npc_id == projectile.0.owner_ship_id {
                continue;
            }
            if projectile.0.hit_ship(*x, *y, tuning.hit_radius) {
                npc_impacts.push((
                    projectile_entity,
                    *npc_id,
                    projectile.0.damage,
                    projectile.0.owner_ship_id,
                ));
                break; // um projétil atinge um navio só
            }
        }
    }

    for (projectile_entity, target_npc_id, damage, killer_ship_id) in npc_impacts {
        commands.entity(projectile_entity).despawn();

        let killed = {
            let Some((entity, mut npc)) = npcs
                .iter_mut()
                .find(|(_, npc)| npc.ship_id == target_npc_id)
            else {
                continue;
            };
            if npc.ai.state == NpcState::Dead {
                continue;
            }
            let pvp_here = map
                .0
                .zone_at(npc.motion.x, npc.motion.y)
                .map(|zone| risk_policy.0.pvp_allowed(zone.tier))
                .unwrap_or(false);
            if !pvp_here {
                info!(
                    npc_id = target_npc_id,
                    "impacto em NPC ignorado: águas protegidas ou fora do mapa"
                );
                continue;
            }
            let killer = player_owners.get(&killer_ship_id).copied();
            npc.last_damage_dealer = killer;
            match apply_npc_damage(&mut npc, damage) {
                DamageOutcome::Survived { remaining_hp } => {
                    info!(
                        npc_id = target_npc_id,
                        damage,
                        hp = remaining_hp,
                        "impacto em NPC"
                    );
                    None
                }
                DamageOutcome::Destroyed => Some((
                    entity,
                    npc.ship_id,
                    npc.kind,
                    npc.motion.x,
                    npc.motion.y,
                    npc.ai.respawn_after_secs,
                    npc.ai.bounty_gold,
                    killer,
                )),
            }
        };

        let Some((
            entity,
            npc_ship_id,
            kind,
            npc_x,
            npc_y,
            respawn_after_secs,
            bounty_gold,
            killer,
        )) = killed
        else {
            continue;
        };

        info!(
            npc_id = npc_ship_id,
            ?kind,
            "NPC DESTROYED; sem wreck (Pilar 1)"
        );
        if let Some(killer) = killer {
            let wallet =
                award_npc_bounty(&mut market, &mut metrics, killer, npc_ship_id, bounty_gold);
            crate::market::send_wallet(&mut connection_manager, &market, &viewers, killer);
            info!(
                npc_id = npc_ship_id,
                killer = ?killer,
                gold = wallet.gold,
                "bounty de NPC creditado"
            );
        }
        commands.entity(entity).despawn();
        npc_respawns
            .0
            .push((respawn_after_secs, kind, (npc_x, npc_y)));
    }
}

pub(crate) fn apply_npc_damage(npc: &mut NpcShip, damage: u32) -> DamageOutcome {
    let outcome = apply_damage(npc.hp, damage);
    match outcome {
        DamageOutcome::Survived { remaining_hp } => npc.hp = remaining_hp,
        DamageOutcome::Destroyed => {
            npc.hp = 0;
            npc.ai.state = NpcState::Dead;
        }
    }
    outcome
}

pub(crate) fn award_npc_bounty(
    market: &mut crate::market::ServerMarket,
    metrics: &mut crate::net::Metrics,
    killer: CharacterId,
    npc_ship_id: u32,
    bounty_gold: u64,
) -> WalletUpdated {
    market.credit(killer, Money(bounty_gold));
    market.ledger.record(
        LedgerKind::NpcBounty,
        Money(bounty_gold),
        format!("npc bounty ship {npc_ship_id}"),
    );
    metrics.npc_bounty_gold_minted += bounty_gold;
    market.persist();
    WalletUpdated {
        gold: market.balance(killer).0,
    }
}

pub(crate) fn to_npc_ship_state(npc: &NpcShip, catalog: &ItemCatalog) -> ShipState {
    ShipState {
        ship_id: npc.ship_id,
        x: npc.motion.x,
        y: npc.motion.y,
        heading: npc.motion.heading,
        speed: npc.motion.speed,
        cargo_weight: npc
            .hold
            .used_weight(catalog)
            .expect("porão NPC só contém definições do catálogo"),
        hp: npc.hp,
        max_hp: npc.max_hp,
        max_speed: npc.stats.speed,
        weapon_damage: npc.stats.weapon_damage,
        weapon_range: npc.stats.weapon_range,
        port_cooldown_secs: npc.battery.port_cooldown,
        starboard_cooldown_secs: npc.battery.starboard_cooldown,
        is_npc: true,
    }
}

fn nearest_player(players: &[(u32, ShipMotion)], x: f32, y: f32, radius: f32) -> Option<u32> {
    players
        .iter()
        .filter(|(_, motion)| distance_sq(x, y, motion.x, motion.y) <= radius * radius)
        .min_by(|a, b| distance_sq(x, y, a.1.x, a.1.y).total_cmp(&distance_sq(x, y, b.1.x, b.1.y)))
        .map(|(id, _)| *id)
}

fn patrol_input(motion: ShipMotion, origin: (f32, f32), radius: f32) -> MotionInput {
    let target = if motion.x < origin.0 + radius * 0.5 {
        (origin.0 + radius, origin.1)
    } else {
        origin
    };
    steer_input(motion, target.0, target.1)
}

fn steer_input(motion: ShipMotion, target_x: f32, target_y: f32) -> MotionInput {
    let desired = (target_y - motion.y).atan2(target_x - motion.x);
    let delta = angle_delta(desired, motion.heading);
    MotionInput {
        throttle: 1.0,
        turn: (delta / std::f32::consts::PI).clamp(-1.0, 1.0),
    }
}

fn side_for_target(heading: f32, target_x: f32, target_y: f32) -> BroadsideSide {
    let target_angle = target_y.atan2(target_x);
    let port_delta = angle_delta(target_angle, heading + std::f32::consts::FRAC_PI_2).abs();
    let starboard_delta = angle_delta(target_angle, heading - std::f32::consts::FRAC_PI_2).abs();
    if port_delta <= starboard_delta {
        BroadsideSide::Port
    } else {
        BroadsideSide::Starboard
    }
}

fn angle_delta(target: f32, current: f32) -> f32 {
    let mut delta = (target - current).rem_euclid(std::f32::consts::TAU);
    if delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    delta
}

fn distance_sq(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx * dx + dy * dy
}

fn distance(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    distance_sq(ax, ay, bx, by).sqrt()
}

fn in_protected_area(map: &WorldMap, x: f32, y: f32) -> bool {
    if map
        .zone_at(x, y)
        .is_ok_and(|zone| zone.tier == RiskTier::Protected)
    {
        return true;
    }
    map.regions()
        .iter()
        .filter_map(|region| region.port.as_ref())
        .any(|port| port.contains(x, y))
}

fn spawn_projectile(
    commands: &mut Commands,
    projectile_ids: &mut ProjectileIdCounter,
    npc: &NpcShip,
    side: BroadsideSide,
    tuning: &CombatTuning,
) {
    let projectile_id = projectile_ids.0;
    projectile_ids.0 += 1;
    let weapon = WeaponParams {
        damage: npc.stats.weapon_damage,
        speed: tuning.projectile_speed,
        range: npc.stats.weapon_range,
        muzzle_offset: tuning.muzzle_offset,
    };
    let projectile = Projectile::from_broadside(
        projectile_id,
        npc.ship_id,
        side,
        npc.motion.x,
        npc.motion.y,
        npc.motion.heading,
        weapon,
    );
    commands.spawn((ServerProjectile(projectile),));
    info!(
        npc_id = npc.ship_id,
        projectile_id, "NPC broadside disparada"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npc_at(position: (f32, f32)) -> NpcShip {
        let mut ids = NpcIdCounter::default();
        build_npc(
            &DevShips::new(),
            &WorldMap::vertical_slice(),
            &NpcSpawnConfig::default(),
            &mut ids,
            ShipKind::Corsair,
            position,
        )
        .1
    }

    #[test]
    fn npc_spawns_at_configured_position_with_hp() {
        let mut ids = NpcIdCounter::default();
        let (ship_id, npc) = build_npc(
            &DevShips::new(),
            &WorldMap::vertical_slice(),
            &NpcSpawnConfig::default(),
            &mut ids,
            ShipKind::Corsair,
            (0.0, 900.0),
        );

        assert_eq!(ship_id, NPC_ID_OFFSET);
        assert_eq!(npc.ship_id, NPC_ID_OFFSET);
        assert!(npc.hp > 0);
        assert_eq!(npc.motion.x, 0.0);
        assert_eq!(npc.motion.y, 900.0);
        assert!(npc.hold.items().is_empty());
    }

    #[test]
    fn patrol_input_moves_npc_toward_patrol_target() {
        let mut npc = npc_at((0.0, 0.0));
        for _ in 0..30 {
            let NpcState::Patrol { origin, radius } = npc.ai.state else {
                panic!("NPC deveria estar em Patrol");
            };
            let input = patrol_input(npc.motion, origin, radius);
            step_motion(&mut npc.motion, &npc.stats, input, &npc.tuning, 1.0 / 30.0);
        }

        assert!(
            npc.motion.x > 0.0,
            "patrulha avança do spawn; x={}",
            npc.motion.x
        );
    }

    #[test]
    fn npc_detects_nearest_player_inside_radius_and_ignores_outside() {
        let players = vec![
            (
                1,
                ShipMotion {
                    x: 10.0,
                    y: 0.0,
                    ..ShipMotion::default()
                },
            ),
            (
                2,
                ShipMotion {
                    x: 1_000.0,
                    y: 0.0,
                    ..ShipMotion::default()
                },
            ),
        ];

        assert_eq!(nearest_player(&players, 0.0, 0.0, 100.0), Some(1));
        assert_eq!(nearest_player(&players, 0.0, 0.0, 5.0), None);
    }

    #[test]
    fn npc_at_weapon_range_fires_broadside() {
        let side = side_for_target(0.0, 0.0, 10.0);
        assert_eq!(side, BroadsideSide::Port);

        let mut battery = BroadsideBattery::default();
        assert!(battery.try_fire(side, 4.0));
        assert!(!battery.is_ready(side));
    }

    #[test]
    fn npc_damage_reduces_hp_and_destroy_marks_dead_without_cargo() {
        let mut npc = npc_at((0.0, 900.0));
        let start_hp = npc.hp;

        assert_eq!(
            apply_npc_damage(&mut npc, 10),
            DamageOutcome::Survived {
                remaining_hp: start_hp - 10
            }
        );
        assert_eq!(npc.hp, start_hp - 10);
        assert_ne!(npc.ai.state, NpcState::Dead);

        let lethal = npc.hp;
        apply_npc_damage(&mut npc, lethal);
        assert_eq!(npc.ai.state, NpcState::Dead);
        assert_eq!(npc.hp, 0);
        assert!(npc.hold.items().is_empty());
    }

    #[test]
    fn npc_bounty_credits_wallet_ledger_metrics_and_wallet_message() {
        let mut market = crate::market::ServerMarket::new();
        let killer = market.character("killer");
        let mut metrics = crate::net::Metrics::default();

        let wallet = award_npc_bounty(&mut market, &mut metrics, killer, 7, 50);

        assert_eq!(wallet.gold, market.balance(killer).0);
        assert!(market
            .ledger
            .entries()
            .iter()
            .any(|entry| entry.kind == LedgerKind::NpcBounty
                && entry.amount == Money(50)
                && entry.memo == "npc bounty ship 7"));
        assert_eq!(market.ledger.npc_bounties(), Money(50));
        assert_eq!(metrics.npc_bounty_gold_minted, 50);
        assert_eq!(market.balance(killer).0, 1_000 + 50);
    }

    #[test]
    fn npc_ship_state_roundtrips_snapshot_flag() {
        let npc = npc_at((0.0, 900.0));
        let state = to_npc_ship_state(&npc, &ItemCatalog::default());

        assert!(state.is_npc);
        assert_eq!(state.ship_id, npc.ship_id);
        assert_eq!(state.hp, npc.hp);
    }
}
