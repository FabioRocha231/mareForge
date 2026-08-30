//! Networking autoritativo do servidor (PRD MF-006/009/010, ADR-0002/0003).
//!
//! O servidor é a única fonte de verdade: aplica `ShipInput` e `FireBroadside`
//! dos clients no modelo puro de `domain-ships`/`domain-combat` a cada tick de
//! 30 Hz e transmite `WorldSnapshot`. Handshake de versão segue o ADR-0011.
//!
//! Nota: canais e registros de mensagens devem ser um espelho exato do
//! `client/src/net.rs` (consolidação futura quando os dois lados compartilharem
//! um crate de net comum).

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_combat::{
    apply_damage, BroadsideBattery, DamageOutcome, Projectile, WeaponParams,
};
use mareforge_domain_items::ItemCatalog;
use mareforge_domain_ships::{
    compute_ship_stats, step_motion, EquippedComponents, MotionInput, MotionTuning, ShipMotion,
    ShipStats,
};
use mareforge_protocol::{
    AssignShip, ClientHello, FireBroadside, ProjectileState, ServerWelcome, ShipDestroyed,
    ShipInput, ShipState, WorldSnapshot, PROTOCOL_VERSION,
};
use tracing::{info, warn};

pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5000);
const SIM_HZ: f64 = 30.0;

/// Canal confiável (handshake, atribuição de navio, comandos de tiro, morte).
#[derive(Channel)]
pub struct ReliableChannel;

/// Canal não-confiável (inputs e snapshots: o próximo já substitui o anterior).
#[derive(Channel)]
pub struct UnreliableChannel;

fn shared_config() -> SharedConfig {
    SharedConfig {
        server_replication_send_interval: Duration::from_secs_f64(1.0 / SIM_HZ),
        client_replication_send_interval: Duration::from_secs_f64(1.0 / SIM_HZ),
        tick: TickConfig {
            tick_duration: Duration::from_secs_f64(1.0 / SIM_HZ),
        },
    }
}

/// Tuning de combate (PRD §23: valores de balanceamento vivem em configuração).
#[derive(Resource, Debug, Clone, Copy)]
pub struct CombatTuning {
    /// Recarga por bordo, em segundos.
    pub cooldown_secs: f32,
    pub projectile_speed: f32,
    pub muzzle_offset: f32,
    /// Raio do círculo de colisão de um navio (aprox. meia eslora), em metros.
    pub hit_radius: f32,
}

impl Default for CombatTuning {
    fn default() -> Self {
        Self {
            cooldown_secs: 4.0,
            projectile_speed: 40.0,
            muzzle_offset: 5.0,
            hit_radius: 10.0,
        }
    }
}

pub struct ServerNetPlugin;

impl Plugin for ServerNetPlugin {
    fn build(&self, app: &mut App) {
        let net_config = NetConfig::Netcode {
            io: IoConfig {
                transport: ServerTransport::UdpSocket(SERVER_ADDR),
                ..default()
            },
            config: NetcodeConfig::default(),
        };
        app.add_plugins(ServerPlugins::new(ServerConfig {
            shared: shared_config(),
            net: vec![net_config],
            ..default()
        }));
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_resource::<CombatTuning>();
        app.init_resource::<ShipIdCounter>();
        app.init_resource::<ProjectileIdCounter>();
        app.add_channel::<ReliableChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        });
        app.add_channel::<UnreliableChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
        });
        app.register_message::<ClientHello>(ChannelDirection::ClientToServer);
        app.register_message::<ShipInput>(ChannelDirection::ClientToServer);
        app.register_message::<FireBroadside>(ChannelDirection::ClientToServer);
        app.register_message::<ServerWelcome>(ChannelDirection::ServerToClient);
        app.register_message::<AssignShip>(ChannelDirection::ServerToClient);
        app.register_message::<WorldSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<ShipDestroyed>(ChannelDirection::ServerToClient);
        app.add_systems(Startup, start_server);
        app.add_systems(
            FixedUpdate,
            (
                handle_connections,
                handle_hello,
                handle_input,
                handle_fire,
                simulate_and_snapshot,
            ),
        );
    }
}

fn start_server(mut commands: Commands) {
    commands.start_server();
    info!(addr = %SERVER_ADDR, "mareforge server listening");
}

/// Navio autoritativo: a única cópia do estado que vale (Pilar 4).
#[derive(Component)]
pub struct ServerShip {
    pub ship_id: u32,
    pub client_id: ClientId,
    pub input: ShipInput,
    pub hp: u32,
    pub battery: BroadsideBattery,
    pub stats: ShipStats,
    pub motion: ShipMotion,
    pub tuning: MotionTuning,
}

#[derive(Component)]
pub struct ServerProjectile(pub Projectile);

#[derive(Resource, Default)]
pub struct ShipIdCounter(pub u32);

#[derive(Resource, Default)]
pub struct ProjectileIdCounter(pub u32);

fn spawn_ship_for(
    commands: &mut Commands,
    counter: &mut ShipIdCounter,
    client_id: ClientId,
) -> u32 {
    let ship_id = counter.0;
    counter.0 += 1;
    let definition = mareforge_domain_ships::ShipDefinition::small_merchant_placeholder();
    let stats = compute_ship_stats(
        &definition,
        &EquippedComponents::default(),
        &ItemCatalog::default(),
    )
    .expect("stats de navio sem equipamento não podem falhar");

    commands.spawn((ServerShip {
        ship_id,
        client_id,
        input: ShipInput {
            throttle: 0.0,
            turn: 0.0,
        },
        hp: stats.max_hp,
        battery: BroadsideBattery::default(),
        stats,
        motion: ShipMotion::default(),
        tuning: MotionTuning::default(),
    },));
    ship_id
}

fn handle_connections(
    mut commands: Commands,
    mut connect: EventReader<ConnectEvent>,
    mut disconnect: EventReader<DisconnectEvent>,
    ships: Query<(Entity, &ServerShip)>,
    _counter: ResMut<ShipIdCounter>,
) {
    for connection in connect.read() {
        info!(client = ?connection.client_id, "client conectado; aguardando ClientHello");
    }
    for event in disconnect.read() {
        info!(client = ?event.client_id, "client desconectado");
        for (entity, ship) in &ships {
            if ship.client_id == event.client_id {
                commands.entity(entity).despawn();
                info!(ship_id = ship.ship_id, "navio do client removido do mundo");
            }
        }
    }
}

/// Handshake (ADR-0011): só spawna navio para hello com versão atual.
fn handle_hello(
    mut commands: Commands,
    mut hello_events: EventReader<ServerReceiveMessage<ClientHello>>,
    mut connection_manager: ResMut<ConnectionManager>,
    ships: Query<&ServerShip>,
    mut counter: ResMut<ShipIdCounter>,
) {
    for event in hello_events.read() {
        let client_id = event.from();
        let hello = event.message();

        if hello.protocol_version != PROTOCOL_VERSION {
            warn!(
                client = ?client_id,
                got = hello.protocol_version,
                expected = PROTOCOL_VERSION,
                "rejeitando client com protocolo incompatível"
            );
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &ServerWelcome {
                    protocol_version: PROTOCOL_VERSION,
                    accepted: false,
                },
            );
            continue;
        }

        let already_has_ship = ships.iter().any(|ship| ship.client_id == client_id);
        if already_has_ship {
            continue;
        }

        let ship_id = spawn_ship_for(&mut commands, &mut counter, client_id);
        let _ = connection_manager.send_message::<ReliableChannel, _>(
            client_id,
            &ServerWelcome {
                protocol_version: PROTOCOL_VERSION,
                accepted: true,
            },
        );
        let _ = connection_manager
            .send_message::<ReliableChannel, _>(client_id, &AssignShip { ship_id });
        info!(client = ?client_id, ship_id, "navio autoritativo criado");
    }
}

/// Último input vence; validação/clamp acontece dentro do modelo puro.
fn handle_input(
    mut input_events: EventReader<ServerReceiveMessage<ShipInput>>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in input_events.read() {
        let client_id = event.from();
        for mut ship in &mut ships {
            if ship.client_id == client_id {
                ship.input = *event.message();
                break;
            }
        }
    }
}

/// Disparo de bordo (PRD MF-009): cooldown decide; o projétil nasce
/// server-authoritative a partir do estado real do navio.
fn handle_fire(
    mut commands: Commands,
    mut fire_events: EventReader<ServerReceiveMessage<FireBroadside>>,
    tuning: Res<CombatTuning>,
    mut projectile_ids: ResMut<ProjectileIdCounter>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in fire_events.read() {
        let client_id = event.from();
        let side = event.message().side;
        let Some(mut ship) = ships.iter_mut().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        if !ship.battery.try_fire(side, tuning.cooldown_secs) {
            continue; // recarregando: clique ignorado, sem spam de projétil
        }
        let projectile_id = projectile_ids.0;
        projectile_ids.0 += 1;
        let weapon = WeaponParams {
            damage: ship.stats.weapon_damage,
            speed: tuning.projectile_speed,
            range: ship.stats.weapon_range,
            muzzle_offset: tuning.muzzle_offset,
        };
        let projectile = Projectile::from_broadside(
            projectile_id,
            ship.ship_id,
            side,
            ship.motion.x,
            ship.motion.y,
            ship.motion.heading,
            weapon,
        );
        info!(
            ship_id = ship.ship_id,
            ?side,
            projectile_id,
            "broadside disparada"
        );
        commands.spawn((ServerProjectile(projectile),));
    }
}

/// O tick autoritativo: move navios e projéteis, resolve impactos e
/// destruições, e transmite o snapshot do mundo.
// System Bevy: quantidade de params é a injeção de dependências, não assinatura.
#[allow(clippy::too_many_arguments)]
fn simulate_and_snapshot(
    mut commands: Commands,
    mut connection_manager: ResMut<ConnectionManager>,
    mut counter: ResMut<crate::plugin::TickCounter>,
    mut ship_ids: ResMut<ShipIdCounter>,
    tuning: Res<CombatTuning>,
    time: Res<Time>,
    mut ships: Query<(Entity, &mut ServerShip)>,
    mut projectiles: Query<(Entity, &mut ServerProjectile)>,
) {
    let dt = time.delta_secs();
    let mut snapshot = WorldSnapshot {
        tick: u64::from(counter.0),
        ships: Vec::with_capacity(ships.iter().count()),
        projectiles: Vec::with_capacity(projectiles.iter().count()),
    };

    // 1. Física dos navios.
    for (_, mut ship) in &mut ships {
        let ServerShip {
            ship_id,
            input,
            stats,
            motion,
            tuning: motion_tuning,
            battery,
            ..
        } = ship.as_mut();
        step_motion(
            motion,
            stats,
            MotionInput {
                throttle: input.throttle,
                turn: input.turn,
            },
            motion_tuning,
            dt,
        );
        battery.advance(dt);
        snapshot.ships.push(ShipState {
            ship_id: *ship_id,
            x: motion.x,
            y: motion.y,
            heading: motion.heading,
            speed: motion.speed,
        });
    }

    // 2. Projéteis: avançam, expiram, colidem (decisão imutável primeiro).
    let ship_positions: HashMap<u32, (Entity, f32, f32)> = ships
        .iter()
        .map(|(entity, ship)| (ship.ship_id, (entity, ship.motion.x, ship.motion.y)))
        .collect();

    let mut expired: Vec<Entity> = Vec::new();
    let mut impacts: Vec<(Entity, u32, u32)> = Vec::new(); // (projétil, alvo ship_id, dano)
    for (projectile_entity, mut projectile) in &mut projectiles {
        projectile.0.advance(dt);
        if projectile.0.expired() {
            expired.push(projectile_entity);
            continue;
        }
        snapshot.projectiles.push(ProjectileState {
            projectile_id: projectile.0.projectile_id,
            x: projectile.0.x,
            y: projectile.0.y,
            heading: projectile.0.heading,
        });

        for (ship_id, (_ship_entity, x, y)) in &ship_positions {
            if *ship_id == projectile.0.owner_ship_id {
                continue;
            }
            if projectile.0.hit_ship(*x, *y, tuning.hit_radius) {
                impacts.push((projectile_entity, *ship_id, projectile.0.damage));
                break; // um projétil atinge um navio só
            }
        }
    }

    // 3. Impactos e destruição (MF-010: sem loot ainda — MF-013 resolve).
    for (projectile_entity, ship_id, damage) in impacts {
        commands.entity(projectile_entity).despawn();
        let Some((entity, mut ship)) = ships.iter_mut().find(|(_, ship)| ship.ship_id == ship_id)
        else {
            continue;
        };
        match apply_damage(ship.hp, damage) {
            DamageOutcome::Survived { remaining_hp } => {
                ship.hp = remaining_hp;
                info!(ship_id, damage, hp = remaining_hp, "impacto no casco");
            }
            DamageOutcome::Destroyed => {
                info!(ship_id, damage, "SHIP DESTROYED");
                let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
                    &ShipDestroyed { ship_id },
                    NetworkTarget::All,
                );
                commands.entity(entity).despawn();
                // Dev respawn (PRD §39): conveniência de teste — o loop do
                // vertical slice não pode matar a sessão do jogador. A regra
                // definitiva de reconstrução é o Dock (PRD §38, Phase 7).
                let new_ship_id = spawn_ship_for(&mut commands, &mut ship_ids, ship.client_id);
                let _ = connection_manager.send_message::<ReliableChannel, _>(
                    ship.client_id,
                    &AssignShip {
                        ship_id: new_ship_id,
                    },
                );
                info!(client = ?ship.client_id, new_ship_id, "dev respawn (PRD §39)");
            }
        }
    }

    for entity in expired {
        commands.entity(entity).despawn();
    }

    if !snapshot.ships.is_empty() || !snapshot.projectiles.is_empty() {
        let _ = connection_manager
            .send_message_to_target::<UnreliableChannel, _>(&snapshot, NetworkTarget::All);
    }

    counter.0 += 1;
}
