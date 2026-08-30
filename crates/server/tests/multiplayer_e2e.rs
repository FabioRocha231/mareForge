//! MF-047: network-layer E2E for two simulated clients against one in-process
//! server. Unlike `e2e.rs`, protocol messages travel through the lightyear
//! wire format over local channels.

use std::time::Duration;

use bevy::input::InputPlugin;
use bevy::prelude::{App, EventReader, MinimalPlugins, Plugin, ResMut, Resource, Update};
use bevy::state::app::StatesPlugin;
use bevy::time::{Fixed, Real, Time, TimeUpdateStrategy};
use bevy::utils::Instant;
use crossbeam_channel::{Receiver, Sender};
use lightyear::prelude::client::{
    Authentication, ClientTransport, ConnectionManager as ClientConnectionManager,
    IoConfig as ClientIoConfig, NetConfig, NetcodeConfig as ClientNetcodeConfig,
};
use lightyear::prelude::server::ServerTransport;
use lightyear::prelude::{ClientReceiveMessage, Key};
use lightyear::transport::LOCAL_SOCKET;
use mareforge_client::net::{
    ClientIdentity, ClientNetOverride, ClientNetPlugin, ReliableChannel, ShipInputOverride,
};
use mareforge_domain_combat::{BroadsideBattery, BroadsideSide};
use mareforge_protocol::{
    AssignShip, FireBroadside, ServerWelcome, ShipDestroyed, ShipInput, ShipState, WalletUpdated,
    WorldSnapshot,
};
use mareforge_server::net::{ServerNetPlugin, ServerShip, ServerTransportOverride};
use mareforge_server::plugin::ServerPlugin;

const CLIENT_A_ID: u64 = 101;
const CLIENT_B_ID: u64 = 102;

fn tick() -> Duration {
    Duration::from_secs_f64(1.0 / 30.0)
}

#[derive(Resource, Default)]
struct Recorded {
    welcome: Option<ServerWelcome>,
    assign: Option<AssignShip>,
    snapshots: Vec<WorldSnapshot>,
    destroyed: Vec<ShipDestroyed>,
    wallets: Vec<WalletUpdated>,
}

impl Recorded {
    fn latest_snapshot(&self) -> Option<&WorldSnapshot> {
        self.snapshots.last()
    }

    fn contains_ship(&self, ship_id: u32) -> bool {
        self.latest_snapshot()
            .is_some_and(|snapshot| snapshot.ships.iter().any(|ship| ship.ship_id == ship_id))
    }

    fn latest_ship(&self, ship_id: u32) -> Option<ShipState> {
        self.snapshots.iter().rev().find_map(|snapshot| {
            snapshot
                .ships
                .iter()
                .copied()
                .find(|ship| ship.ship_id == ship_id)
        })
    }
}

struct RecordPlugin;

impl Plugin for RecordPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Recorded>();
        app.add_systems(
            Update,
            (
                record_welcome,
                record_assign,
                record_snapshot,
                record_destroyed,
                record_wallet,
            ),
        );
    }
}

fn record_welcome(
    mut events: EventReader<ClientReceiveMessage<ServerWelcome>>,
    mut recorded: ResMut<Recorded>,
) {
    for event in events.read() {
        recorded.welcome = Some(*event.message());
    }
}

fn record_assign(
    mut events: EventReader<ClientReceiveMessage<AssignShip>>,
    mut recorded: ResMut<Recorded>,
) {
    for event in events.read() {
        recorded.assign = Some(*event.message());
    }
}

fn record_snapshot(
    mut events: EventReader<ClientReceiveMessage<WorldSnapshot>>,
    mut recorded: ResMut<Recorded>,
) {
    for event in events.read() {
        recorded.snapshots.push(event.message().clone());
    }
}

fn record_destroyed(
    mut events: EventReader<ClientReceiveMessage<ShipDestroyed>>,
    mut recorded: ResMut<Recorded>,
) {
    for event in events.read() {
        recorded.destroyed.push(*event.message());
    }
}

fn record_wallet(
    mut events: EventReader<ClientReceiveMessage<WalletUpdated>>,
    mut recorded: ResMut<Recorded>,
) {
    for event in events.read() {
        recorded.wallets.push(*event.message());
    }
}

struct Harness {
    server_app: App,
    client_a: App,
    client_b: App,
    now: Instant,
}

impl Harness {
    fn new() -> Self {
        let (a_to_server, a_from_client) = crossbeam_channel::unbounded();
        let (a_to_client, a_from_server) = crossbeam_channel::unbounded();
        let (b_to_server, b_from_client) = crossbeam_channel::unbounded();
        let (b_to_client, b_from_server) = crossbeam_channel::unbounded();

        let client_a_config = client_net_config(CLIENT_A_ID, a_from_server, a_to_server);
        let client_b_config = client_net_config(CLIENT_B_ID, b_from_server, b_to_server);

        let mut server_app = App::new();
        server_app.add_plugins(MinimalPlugins);
        server_app.insert_resource(ServerTransportOverride(vec![
            ServerTransport::Channels {
                channels: vec![(LOCAL_SOCKET, a_from_client, a_to_client)],
            },
            ServerTransport::Channels {
                channels: vec![(LOCAL_SOCKET, b_from_client, b_to_client)],
            },
        ]));
        server_app.add_plugins(ServerPlugin);
        server_app.add_plugins(ServerNetPlugin);

        let mut client_a = build_client(client_a_config, "multiplayer-token-a");
        let mut client_b = build_client(client_b_config, "multiplayer-token-b");

        server_app.finish();
        server_app.cleanup();
        client_a.finish();
        client_a.cleanup();
        client_b.finish();
        client_b.cleanup();

        let now = Instant::now();
        for app in [&mut server_app, &mut client_a, &mut client_b] {
            app.insert_resource(TimeUpdateStrategy::ManualInstant(now));
            app.world_mut()
                .get_resource_mut::<Time<Real>>()
                .unwrap()
                .update_with_instant(now);
        }

        Self {
            server_app,
            client_a,
            client_b,
            now,
        }
    }

    fn frame_step(&mut self) {
        self.now += tick();
        for app in [&mut self.client_a, &mut self.client_b, &mut self.server_app] {
            app.insert_resource(TimeUpdateStrategy::ManualInstant(self.now));
            app.update();
        }
    }

    fn run_frames(&mut self, frames: usize) {
        for _ in 0..frames {
            self.frame_step();
        }
    }

    fn run_until(&mut self, max_frames: usize, ready: impl Fn(&Harness) -> bool) -> bool {
        for _ in 0..max_frames {
            if ready(self) {
                return true;
            }
            self.frame_step();
        }
        ready(self)
    }

    fn recorded_a(&self) -> &Recorded {
        self.client_a.world().resource::<Recorded>()
    }

    fn recorded_b(&self) -> &Recorded {
        self.client_b.world().resource::<Recorded>()
    }

    fn wait_for_handshake(&mut self) {
        let ready = self.run_until(300, |harness| {
            harness.recorded_a().assign.is_some() && harness.recorded_b().assign.is_some()
        });
        assert!(ready, "both clients should receive AssignShip");
        assert_eq!(
            self.recorded_a().welcome,
            Some(ServerWelcome {
                protocol_version: mareforge_protocol::PROTOCOL_VERSION,
                accepted: true,
            })
        );
        assert!(!self.recorded_a().wallets.is_empty());
    }

    fn ship_ids(&self) -> (u32, u32) {
        (
            self.recorded_a().assign.unwrap().ship_id,
            self.recorded_b().assign.unwrap().ship_id,
        )
    }

    fn set_input_a(&mut self, input: ShipInput) {
        self.client_a
            .world_mut()
            .insert_resource(ShipInputOverride(Some(input)));
    }

    fn stop_input_a(&mut self) {
        self.set_input_a(ShipInput {
            throttle: 0.0,
            turn: 0.0,
        });
    }

    fn send_fire_a(&mut self, side: BroadsideSide) {
        self.client_a
            .world_mut()
            .resource_mut::<ClientConnectionManager>()
            .send_message::<ReliableChannel, _>(&FireBroadside { side })
            .expect("client can queue FireBroadside");
    }

    fn prepare_ships(&mut self, a_id: u32, b_id: u32) {
        set_ship_position(&mut self.server_app, a_id, 300.0, 0.0, 0.0);
        set_ship_position(&mut self.server_app, b_id, 250.0, 0.0, 0.0);
        self.run_frames(30);
        assert!(
            self.recorded_b().contains_ship(a_id),
            "B should see A inside the AOI"
        );
    }

    fn move_a_out_of_aoi(&mut self, a_id: u32) {
        self.set_input_a(ShipInput {
            throttle: 1.0,
            turn: 0.0,
        });
        let left = self.run_until(400, |harness| !harness.recorded_b().contains_ship(a_id));
        assert!(left, "A should leave B's AOI");
        self.run_frames(20);
        assert!(
            !self.recorded_b().contains_ship(a_id),
            "A should stay absent from B's AOI"
        );
        self.stop_input_a();
    }

    fn return_a_to_aoi(&mut self, a_id: u32) {
        set_ship_heading(&mut self.server_app, a_id, std::f32::consts::PI);
        self.set_input_a(ShipInput {
            throttle: 1.0,
            turn: 0.0,
        });
        let returned = self.run_until(400, |harness| harness.recorded_b().contains_ship(a_id));
        assert!(returned, "A should reappear in B's AOI");
        self.stop_input_a();
    }
}

fn client_net_config(
    client_id: u64,
    from_server: Receiver<Vec<u8>>,
    to_server: Sender<Vec<u8>>,
) -> NetConfig {
    NetConfig::Netcode {
        auth: Authentication::Manual {
            server_addr: LOCAL_SOCKET,
            client_id,
            private_key: Key::default(),
            protocol_id: 0,
        },
        config: ClientNetcodeConfig::default(),
        io: ClientIoConfig::from_transport(ClientTransport::LocalChannel {
            recv: from_server,
            send: to_server,
        }),
    }
}

fn build_client(net_config: NetConfig, identity: &str) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin, StatesPlugin));
    app.insert_resource(Time::<Fixed>::from_hz(30.0));
    app.insert_resource(ClientNetOverride(Some(net_config)));
    app.insert_resource(ClientIdentity(Some(identity.to_owned())));
    app.add_plugins(ClientNetPlugin);
    app.add_plugins(RecordPlugin);
    app
}

fn set_ship_position(app: &mut App, ship_id: u32, x: f32, y: f32, heading: f32) {
    let world = app.world_mut();
    let mut query = world.query::<&mut ServerShip>();
    for mut ship in query.iter_mut(world) {
        if ship.ship_id == ship_id {
            ship.motion.x = x;
            ship.motion.y = y;
            ship.motion.heading = heading;
            ship.motion.speed = 0.0;
        }
    }
}

fn set_ship_heading(app: &mut App, ship_id: u32, heading: f32) {
    let world = app.world_mut();
    let mut query = world.query::<&mut ServerShip>();
    for mut ship in query.iter_mut(world) {
        if ship.ship_id == ship_id {
            ship.motion.heading = heading;
        }
    }
}

fn set_ship_hp(app: &mut App, ship_id: u32, hp: u32) {
    let world = app.world_mut();
    let mut query = world.query::<&mut ServerShip>();
    for mut ship in query.iter_mut(world) {
        if ship.ship_id == ship_id {
            ship.hp = hp;
        }
    }
}

fn reset_ship_battery(app: &mut App, ship_id: u32) {
    let world = app.world_mut();
    let mut query = world.query::<&mut ServerShip>();
    for mut ship in query.iter_mut(world) {
        if ship.ship_id == ship_id {
            ship.battery = BroadsideBattery::default();
        }
    }
}

#[test]
fn multiplayer_two_clients_receive_each_other_in_aoi() {
    let mut harness = Harness::new();
    harness.wait_for_handshake();
    let (a_id, b_id) = harness.ship_ids();
    harness.run_frames(30);

    assert!(
        harness.recorded_b().contains_ship(a_id),
        "B should receive A's ShipState in WorldSnapshot"
    );
    assert!(
        harness.recorded_a().contains_ship(b_id),
        "A should receive B's ShipState in WorldSnapshot"
    );
}

#[test]
fn multiplayer_client_outside_aoi_not_in_snapshot() {
    let mut harness = Harness::new();
    harness.wait_for_handshake();
    let (a_id, b_id) = harness.ship_ids();
    harness.prepare_ships(a_id, b_id);
    harness.move_a_out_of_aoi(a_id);
}

#[test]
fn multiplayer_client_returns_to_aoi_reappears() {
    let mut harness = Harness::new();
    harness.wait_for_handshake();
    let (a_id, b_id) = harness.ship_ids();
    harness.prepare_ships(a_id, b_id);
    harness.move_a_out_of_aoi(a_id);
    harness.return_a_to_aoi(a_id);

    assert!(
        harness.recorded_b().contains_ship(a_id),
        "A should be visible again after sailing back"
    );
}

#[test]
fn multiplayer_broadside_damages_other_client_ship() {
    let mut harness = Harness::new();
    harness.wait_for_handshake();
    let (a_id, b_id) = harness.ship_ids();
    harness.prepare_ships(a_id, b_id);
    set_ship_position(
        &mut harness.server_app,
        a_id,
        300.0,
        0.0,
        std::f32::consts::FRAC_PI_2,
    );
    harness.run_frames(10);

    let hp_before = harness.recorded_b().latest_ship(b_id).map(|ship| ship.hp);
    assert!(
        hp_before.is_some(),
        "B should have a ShipState before the hit"
    );

    harness.send_fire_a(BroadsideSide::Port);
    let damaged = harness.run_until(60, |harness| {
        harness
            .recorded_b()
            .latest_ship(b_id)
            .is_some_and(|ship| ship.hp < hp_before.unwrap())
    });
    assert!(damaged, "B should receive reduced hp in WorldSnapshot");
}

#[test]
fn multiplayer_full_scenario_a_b_fire_damage() {
    let mut harness = Harness::new();
    harness.wait_for_handshake();
    let (a_id, b_id) = harness.ship_ids();
    harness.prepare_ships(a_id, b_id);

    harness.move_a_out_of_aoi(a_id);
    harness.return_a_to_aoi(a_id);

    set_ship_position(
        &mut harness.server_app,
        a_id,
        300.0,
        0.0,
        std::f32::consts::FRAC_PI_2,
    );
    harness.run_frames(10);
    assert!(
        harness.recorded_b().contains_ship(a_id),
        "A must be in B's AOI before the broadside"
    );

    let hp_before = harness.recorded_b().latest_ship(b_id).unwrap().hp;
    harness.send_fire_a(BroadsideSide::Port);
    let damaged = harness.run_until(60, |harness| {
        harness
            .recorded_b()
            .latest_ship(b_id)
            .is_some_and(|ship| ship.hp < hp_before)
    });
    assert!(damaged, "full scenario should apply broadside damage");

    set_ship_hp(&mut harness.server_app, b_id, 1);
    reset_ship_battery(&mut harness.server_app, a_id);
    harness.run_frames(2);
    harness.send_fire_a(BroadsideSide::Port);
    let destroyed = harness.run_until(60, |harness| {
        harness
            .recorded_b()
            .destroyed
            .iter()
            .any(|message| message.ship_id == b_id)
    });
    assert!(destroyed, "full scenario should observe ShipDestroyed");
}
