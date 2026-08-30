//! Networking autoritativo do servidor (PRD MF-006, ADR-0002/0003).
//!
//! O servidor é a única fonte de verdade: aplica `ShipInput` dos clients no
//! modelo puro de `domain-ships` a cada tick de 30 Hz e transmite
//! `WorldSnapshot`. O handshake de versão segue o ADR-0011.
//!
//! Nota: canais e registros de mensagens devem ser um espelho exato do
//! `client/src/net.rs` (consolidação futura quando os dois lados compartilharem
//! um crate de net comum).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_items::ItemCatalog;
use mareforge_domain_ships::{
    compute_ship_stats, step_motion, EquippedComponents, MotionInput, MotionTuning, ShipMotion,
    ShipStats,
};
use mareforge_protocol::{
    AssignShip, ClientHello, ServerWelcome, ShipInput, ShipState, WorldSnapshot, PROTOCOL_VERSION,
};
use tracing::info;

pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5000);
const SIM_HZ: f64 = 30.0;

/// Canal confiável (handshake, atribuição de navio).
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
        app.register_message::<ServerWelcome>(ChannelDirection::ServerToClient);
        app.register_message::<AssignShip>(ChannelDirection::ServerToClient);
        app.register_message::<WorldSnapshot>(ChannelDirection::ServerToClient);
        app.init_resource::<ShipIdCounter>();
        app.add_systems(
            FixedUpdate,
            (
                handle_connections,
                handle_hello,
                handle_input,
                simulate_and_snapshot,
            ),
        );
        app.add_systems(Startup, start_server);
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
    pub stats: ShipStats,
    pub motion: ShipMotion,
    pub tuning: MotionTuning,
}

#[derive(Resource, Default)]
pub struct ShipIdCounter(pub u32);

fn handle_connections(
    mut commands: Commands,
    mut connect: EventReader<ConnectEvent>,
    mut disconnect: EventReader<DisconnectEvent>,
    ships: Query<(Entity, &ServerShip)>,
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
            tracing::warn!(
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

        let ship_id = counter.0;
        counter.0 += 1;
        let definition = mareforge_domain_ships::ShipDefinition::small_merchant_placeholder();
        let stats = compute_ship_stats(
            &definition,
            &EquippedComponents::default(),
            &ItemCatalog::default(),
        )
        .expect("stats de navio sem equipamento não podem falhar");

        let _ = connection_manager.send_message::<ReliableChannel, _>(
            client_id,
            &ServerWelcome {
                protocol_version: PROTOCOL_VERSION,
                accepted: true,
            },
        );
        let _ = connection_manager
            .send_message::<ReliableChannel, _>(client_id, &AssignShip { ship_id });

        commands.spawn((ServerShip {
            ship_id,
            client_id,
            input: ShipInput {
                throttle: 0.0,
                turn: 0.0,
            },
            stats,
            motion: ShipMotion::default(),
            tuning: MotionTuning::default(),
        },));
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

/// O tick autoritativo: move todos os navios no modelo puro e transmite o
/// snapshot do mundo.
fn simulate_and_snapshot(
    mut connection_manager: ResMut<ConnectionManager>,
    mut counter: ResMut<crate::plugin::TickCounter>,
    time: Res<Time>,
    mut ships: Query<&mut ServerShip>,
) {
    let dt = time.delta_secs();
    let mut snapshot = WorldSnapshot {
        tick: u64::from(counter.0),
        ships: Vec::with_capacity(ships.iter().count()),
    };

    for mut ship in &mut ships {
        let ServerShip {
            ship_id,
            input,
            stats,
            motion,
            tuning,
            ..
        } = ship.as_mut();
        step_motion(
            motion,
            stats,
            MotionInput {
                throttle: input.throttle,
                turn: input.turn,
            },
            tuning,
            dt,
        );
        snapshot.ships.push(ShipState {
            ship_id: *ship_id,
            x: motion.x,
            y: motion.y,
            heading: motion.heading,
            speed: motion.speed,
        });
    }

    if !snapshot.ships.is_empty() {
        let _ = connection_manager
            .send_message_to_target::<UnreliableChannel, _>(&snapshot, NetworkTarget::All);
    }

    counter.0 += 1;
}
