//! Networking do client (PRD MF-006, ADR-0002/0003).
//!
//! O client é uma janela sobre a verdade do servidor: envia `ShipInput`
//! (intenção) e desenha o `WorldSnapshot` (realidade). Nada de física aqui —
//! client prediction/interpolação fina entram depois (Phase 2).
//!
//! Nota: canais e registros de mensagens devem ser um espelho exato do
//! `server/src/net.rs`.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use mareforge_protocol::{
    AssignShip, ClientHello, ServerWelcome, ShipInput, WorldSnapshot, PROTOCOL_VERSION,
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
        app.register_message::<ServerWelcome>(ChannelDirection::ServerToClient);
        app.register_message::<AssignShip>(ChannelDirection::ServerToClient);
        app.register_message::<WorldSnapshot>(ChannelDirection::ServerToClient);
        app.add_systems(Startup, (connect, log_connecting));
        app.add_systems(FixedUpdate, (send_hello_on_connect, send_ship_input));
        app.add_systems(Update, (handle_handshake,));
    }
}

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
    let throttle = if keys.pressed(KeyCode::KeyW) {
        1.0
    } else {
        0.0
    };
    let turn = (keys.pressed(KeyCode::KeyA) as i32 - keys.pressed(KeyCode::KeyD) as i32) as f32;
    let _ = connection_manager.send_message::<UnreliableChannel, _>(&ShipInput { throttle, turn });
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
