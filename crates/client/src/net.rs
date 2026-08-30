//! Networking do client (PRD MF-006, ADR-0002/0003).
//!
//! O client é uma janela sobre a verdade do servidor: envia `ShipInput`
//! (intenção) e desenha o `WorldSnapshot` (realidade). Nada de física aqui —
//! client prediction/interpolação fina entram depois (Phase 2).
//!
//! Nota: canais e registros de mensagens devem ser um espelho exato do
//! `server/src/net.rs`.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use mareforge_domain_combat::BroadsideSide;
use mareforge_protocol::{
    AssignShip, ClientHello, CraftItem, CraftResult, FireBroadside, GatherNode, GatherResult,
    LootResult, LootWreck, NodeUpdated, NodesSnapshot, RecipesSnapshot, ServerWelcome,
    ShipDestroyed, ShipInput, WorldSnapshot, WreckRemoved, WreckSpawned, ZoneChanged,
    PROTOCOL_VERSION,
};

pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);
/// Porta 0: o SO escolhe a porta efêmera — permite vários clients na mesma máquina.
const CLIENT_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
const SIM_HZ: f64 = 30.0;

/// Espelho do canal confiável do servidor.
#[derive(Channel)]
pub struct ReliableChannel;

/// Espelho do canal não-confiável do servidor.
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

/// Navio atribuído a este client (destacado no visual).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct MyShip(pub Option<u32>);

pub struct ClientNetPlugin;

impl Plugin for ClientNetPlugin {
    fn build(&self, app: &mut App) {
        // Id único por processo: dois clients na mesma máquina não podem
        // disputar o mesmo client_id no netcode (o servidor rejeita duplicado).
        let client_id = u64::from(std::process::id());
        let auth = Authentication::Manual {
            server_addr: SERVER_ADDR,
            client_id,
            private_key: Key::default(),
            protocol_id: 0,
        };
        let net_config = NetConfig::Netcode {
            auth,
            io: IoConfig {
                transport: ClientTransport::UdpSocket(CLIENT_ADDR),
                ..default()
            },
            config: NetcodeConfig::default(),
        };
        app.init_resource::<MyShip>();
        app.add_plugins(ClientPlugins::new(ClientConfig {
            shared: shared_config(),
            net: net_config,
            ..default()
        }));
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
        app.register_message::<LootWreck>(ChannelDirection::ClientToServer);
        app.register_message::<GatherNode>(ChannelDirection::ClientToServer);
        app.register_message::<CraftItem>(ChannelDirection::ClientToServer);
        app.register_message::<ServerWelcome>(ChannelDirection::ServerToClient);
        app.register_message::<AssignShip>(ChannelDirection::ServerToClient);
        app.register_message::<WorldSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<ShipDestroyed>(ChannelDirection::ServerToClient);
        app.register_message::<WreckSpawned>(ChannelDirection::ServerToClient);
        app.register_message::<WreckRemoved>(ChannelDirection::ServerToClient);
        app.register_message::<LootResult>(ChannelDirection::ServerToClient);
        app.register_message::<ZoneChanged>(ChannelDirection::ServerToClient);
        app.register_message::<NodesSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<NodeUpdated>(ChannelDirection::ServerToClient);
        app.register_message::<GatherResult>(ChannelDirection::ServerToClient);
        app.register_message::<RecipesSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<CraftResult>(ChannelDirection::ServerToClient);
        app.init_resource::<crate::ship::DestroyedShips>();
        app.init_resource::<KnownWrecks>();
        app.add_systems(Startup, (connect, log_connecting));
        app.add_systems(
            FixedUpdate,
            (
                send_hello_on_connect,
                send_ship_input,
                send_fire_input,
                send_loot_input,
                send_gather_input,
            ),
        );
        app.add_systems(
            Update,
            (
                handle_handshake,
                handle_ship_destroyed,
                handle_wreck_lifecycle,
                handle_loot_result,
            ),
        );
    }
}

/// Wrecks conhecidos pelo client (posições para saque e visuais).
#[derive(Resource, Debug, Default)]
pub struct KnownWrecks(pub HashMap<u32, Vec2>);

fn connect(mut commands: Commands) {
    commands.connect_client();
}

fn log_connecting() {
    info!(server = %SERVER_ADDR, "conectando ao servidor mareforge");
}

/// Handshake (ADR-0011): primeira mensagem após conectar é o hello com a
/// versão do protocolo.
fn send_hello_on_connect(
    mut connect: EventReader<ConnectEvent>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    for _ in connect.read() {
        info!("conectado; enviando ClientHello");
        let _ = connection_manager.send_message::<ReliableChannel, _>(&ClientHello::current());
    }
}

/// Intenção de navegação local — o servidor valida e aplica (Pilar 4).
fn send_ship_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    // Dev tooling (PRD §39): MAREFORGE_AUTOSAIL=1 segura o W sozinho.
    let autosail = std::env::var_os("MAREFORGE_AUTOSAIL").is_some();
    let throttle = if keys.pressed(KeyCode::KeyW) || autosail {
        1.0
    } else {
        0.0
    };
    let turn = (keys.pressed(KeyCode::KeyA) as i32 - keys.pressed(KeyCode::KeyD) as i32) as f32;
    let _ = connection_manager.send_message::<UnreliableChannel, _>(&ShipInput { throttle, turn });
}

/// Comando de tiro (PRD §19): Q = bordo esquerdo, E = bordo direito.
/// Confiável: cada apertada é um tiro.
fn send_fire_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut autofire_timer: Local<f32>,
    mut autofire_side: Local<u8>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    let side = if keys.just_pressed(KeyCode::KeyQ) {
        Some(BroadsideSide::Port)
    } else if keys.just_pressed(KeyCode::KeyE) {
        Some(BroadsideSide::Starboard)
    } else if autofire_enabled() {
        // Dev tooling (PRD §39): MAREFORGE_AUTOFIRE=1 dispara bordos
        // alternados sozinho, respeitando a recarga — smoke/playtest sem
        // interação. Não é mecânica de jogo.
        *autofire_timer += time.delta_secs();
        if *autofire_timer >= 4.2 {
            *autofire_timer = 0.0;
            *autofire_side ^= 1;
            Some(if *autofire_side == 0 {
                BroadsideSide::Port
            } else {
                BroadsideSide::Starboard
            })
        } else {
            None
        }
    } else {
        None
    };
    if let Some(side) = side {
        info!(?side, "disparando bordo");
        let _ = connection_manager.send_message::<ReliableChannel, _>(&FireBroadside { side });
    }
}

fn autofire_enabled() -> bool {
    std::env::var_os("MAREFORGE_AUTOFIRE").is_some()
}

/// Navio afundou: remove o visual e memoriza o id (snapshots antigos ainda
/// podem trazer o casco; ids não são reutilizados — o respawn tem id novo).
fn handle_ship_destroyed(
    mut events: EventReader<ClientReceiveMessage<ShipDestroyed>>,
    mut destroyed: ResMut<crate::ship::DestroyedShips>,
    mut commands: Commands,
    visuals: Query<(Entity, &crate::ship::ShipVisual)>,
) {
    for event in events.read() {
        let ship_id = event.message().ship_id;
        warn!(ship_id, "navio destruído no horizonte");
        destroyed.0.insert(ship_id);
        for (entity, visual) in &visuals {
            if visual.target.ship_id == ship_id {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Ciclo de vida dos wrecks: memoriza posições e remove os que somirem.
fn handle_wreck_lifecycle(
    mut spawned: EventReader<ClientReceiveMessage<WreckSpawned>>,
    mut removed: EventReader<ClientReceiveMessage<WreckRemoved>>,
    mut known: ResMut<KnownWrecks>,
    mut commands: Commands,
    visuals: Query<(Entity, &crate::ship::WreckVisual)>,
) {
    for event in spawned.read() {
        let wreck = event.message();
        info!(
            wreck_id = wreck.wreck_id,
            stacks = wreck.stack_count,
            "wreck avistado — carga esperando saqueador"
        );
        known.0.insert(wreck.wreck_id, Vec2::new(wreck.x, wreck.y));
    }
    for event in removed.read() {
        let wreck_id = event.message().wreck_id;
        known.0.remove(&wreck_id);
        for (entity, visual) in &visuals {
            if visual.wreck_num == wreck_id {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn handle_loot_result(mut events: EventReader<ClientReceiveMessage<LootResult>>) {
    for event in events.read() {
        let result = event.message();
        if result.success {
            info!(
                wreck_id = result.wreck_id,
                "saque concluído: carga no porão"
            );
        } else {
            warn!(wreck_id = result.wreck_id, "saque recusado pelo servidor");
        }
    }
}

/// F saqueia o wreck mais próximo (PRD §27). No modo dev (PRD §39), o
/// autofire também saqueia sozinho para o smoke do loop econômico.
fn send_loot_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    my_ship: Res<MyShip>,
    known_wrecks: Res<KnownWrecks>,
    visuals: Query<&crate::ship::ShipVisual>,
    mut autofire_timer: Local<f32>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    let manual = keys.just_pressed(KeyCode::KeyF);
    let mut auto = false;
    if autofire_enabled() {
        *autofire_timer += time.delta_secs();
        if *autofire_timer >= 2.0 {
            *autofire_timer = 0.0;
            auto = true;
        }
    }
    if !manual && !auto {
        return;
    }

    let Some(my_id) = my_ship.0 else { return };
    let Some(my_visual) = visuals.iter().find(|v| v.target.ship_id == my_id) else {
        return;
    };
    let mine = Vec2::new(my_visual.target.x, my_visual.target.y);

    // Raio de interação do servidor (30 m) com folga para o lerp visual.
    const INTERACT_RADIUS_SQ: f32 = 28.0 * 28.0;
    let nearest = known_wrecks
        .0
        .iter()
        .filter(|(_, pos)| mine.distance_squared(**pos) <= INTERACT_RADIUS_SQ)
        .min_by(|a, b| {
            let da = mine.distance_squared(*a.1);
            let db = mine.distance_squared(*b.1);
            da.total_cmp(&db)
        })
        .map(|(id, _)| *id);

    if let Some(wreck_id) = nearest {
        if manual {
            info!(wreck_id, "saqueando wreck");
        }
        let _ = connection_manager.send_message::<ReliableChannel, _>(&LootWreck { wreck_id });
    }
}

/// G coleta o node mais próximo com estoque (PRD MF-019). Dev tooling
/// (§39): MAREFORGE_AUTOGATHER=1 coleta sozinho — smoke do loop de recursos.
fn send_gather_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    my_ship: Res<MyShip>,
    known_nodes: Res<crate::nodes::KnownNodes>,
    visuals: Query<&crate::ship::ShipVisual>,
    mut auto_timer: Local<f32>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    let manual = keys.just_pressed(KeyCode::KeyG);
    let mut auto = false;
    if autogather_enabled() {
        *auto_timer += time.delta_secs();
        if *auto_timer >= 1.0 {
            *auto_timer = 0.0;
            auto = true;
        }
    }
    if !manual && !auto {
        return;
    }

    let Some(my_id) = my_ship.0 else { return };
    let Some(my_visual) = visuals.iter().find(|v| v.target.ship_id == my_id) else {
        return;
    };
    let mine = Vec2::new(my_visual.target.x, my_visual.target.y);

    // Mesmo raio do servidor (30 m) com folga para o lerp visual.
    const GATHER_RADIUS_SQ: f32 = 28.0 * 28.0;
    let nearest = known_nodes
        .0
        .iter()
        .filter(|(_, info)| info.stock > 0 && mine.distance_squared(info.pos) <= GATHER_RADIUS_SQ)
        .min_by(|a, b| {
            let da = mine.distance_squared(a.1.pos);
            let db = mine.distance_squared(b.1.pos);
            da.total_cmp(&db)
        })
        .map(|(id, _)| *id);

    if let Some(node_id) = nearest {
        if manual {
            info!(node_id, "coletando node");
        }
        let _ = connection_manager.send_message::<ReliableChannel, _>(&GatherNode { node_id });
    }
}

fn autogather_enabled() -> bool {
    std::env::var_os("MAREFORGE_AUTOGATHER").is_some()
}

fn handle_handshake(
    mut welcome_events: EventReader<ClientReceiveMessage<ServerWelcome>>,
    mut assign_events: EventReader<ClientReceiveMessage<AssignShip>>,
    mut my_ship: ResMut<MyShip>,
) {
    for event in welcome_events.read() {
        let welcome = event.message();
        if welcome.accepted {
            info!(
                server_protocol = welcome.protocol_version,
                "handshake aceito pelo servidor"
            );
        } else {
            error!(
                server_protocol = welcome.protocol_version,
                our_protocol = PROTOCOL_VERSION,
                "servidor rejeitou a versão do protocolo; atualize o client"
            );
        }
    }
    for event in assign_events.read() {
        my_ship.0 = Some(event.message().ship_id);
        info!(
            ship_id = event.message().ship_id,
            "navio atribuído a este client"
        );
    }
}
