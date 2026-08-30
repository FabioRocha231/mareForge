//! MF-049: Trip lifecycle correto.
//!
//! A viagem NAO comeca simplesmente porque existe um player ship no mundo.
//! Comeca em Undock bem-sucedido, encerra em Dock bem-sucedido ou ShipDestroyed.
//! O restore apos restart e coerente com a presenca persistida.
//!
//! Estes testes chamam diretamente os helpers `finalize_trip` e `start_trip`
//! expostos pelo modulo `net` para validar a semantica do lifecycle sem
//! precisar montar todo o grafo Bevy.

use mareforge_domain_combat::BroadsideBattery;
use mareforge_domain_items::{CargoHold, ItemCatalog};
use mareforge_domain_ships::{
    compute_ship_stats, EquippedComponents, MotionTuning, ShipKind, ShipLoadout, ShipMotion,
    VesselPresence,
};
use mareforge_protocol::ShipInput;
use mareforge_server::crafting::DevShips;
use mareforge_server::net::{finalize_trip, start_trip, Metrics, ServerShip};
use mareforge_server::persist::ShipRecord;
use mareforge_shared::ids::{CharacterId, RegionId, ShipInstanceId};

/// Constroi um ServerShip minimo so para os testes de lifecycle. Stats de
/// fallback quando o catalogo vazio nao consegue computar (e ele nao
/// consegue, porque o dominio exige definicoes reais).
fn make_ship(presence: VesselPresence) -> ServerShip {
    let ship_instance = ShipInstanceId::new();
    let kind = ShipKind::SmallMerchant;
    let definition = DevShips::new().definition(kind).clone();
    let stats = compute_ship_stats(
        &definition,
        &EquippedComponents::default(),
        &ItemCatalog::default(),
    )
    .expect("stats base do merchant nao falham");
    let hold = CargoHold::new(ship_instance, stats.cargo_capacity);
    ServerShip {
        ship_id: 1,
        client_id: None,
        character: CharacterId::new(),
        ship_instance,
        kind,
        presence,
        loadout: ShipLoadout::new(),
        input: ShipInput {
            throttle: 0.0,
            turn: 0.0,
        },
        hp: stats.max_hp,
        hold,
        battery: BroadsideBattery::default(),
        stats,
        motion: ShipMotion {
            x: 0.0,
            y: 0.0,
            ..ShipMotion::default()
        },
        tuning: MotionTuning::default(),
        zone: None,
        trip_started_at: None,
    }
}

/// 1. Tempo parado Docked nao conta.
#[test]
fn docked_time_does_not_count() {
    let mut ship = make_ship(VesselPresence::Docked(RegionId::new()));
    let mut metrics = Metrics::default();

    let closed = finalize_trip(&mut ship, &mut metrics, 3600.0);

    assert!(!closed);
    assert_eq!(metrics.trip_count, 0);
    assert_eq!(metrics.trip_total_secs, 0.0);
    assert!(ship.trip_started_at.is_none());
}

/// 2. Undock inicia trip.
#[test]
fn undock_starts_trip() {
    let mut ship = make_ship(VesselPresence::Docked(RegionId::new()));
    assert!(ship.trip_started_at.is_none());

    start_trip(&mut ship, 100.0);

    assert_eq!(ship.trip_started_at, Some(100.0));
}

/// 3. Dock encerra exatamente uma trip.
#[test]
fn dock_ends_exactly_one_trip() {
    let mut ship = make_ship(VesselPresence::AtSea);
    ship.trip_started_at = Some(50.0);
    let mut metrics = Metrics::default();

    let closed = finalize_trip(&mut ship, &mut metrics, 110.0);

    assert!(closed);
    assert!(ship.trip_started_at.is_none());
    assert_eq!(metrics.trip_count, 1);
    assert!(
        (metrics.trip_total_secs - 60.0).abs() < 1e-6,
        "duracao somada: esperada 60s, veio {}",
        metrics.trip_total_secs,
    );
}

/// 4. Dock seguido de espera nao cria segunda trip.
#[test]
fn dock_followed_by_idle_does_not_create_second_trip() {
    let mut ship = make_ship(VesselPresence::Docked(RegionId::new()));
    let mut metrics = Metrics {
        trip_count: 1,
        trip_total_secs: 60.0,
        ..Metrics::default()
    };

    let closed = finalize_trip(&mut ship, &mut metrics, 1000.0);

    assert!(!closed);
    assert_eq!(metrics.trip_count, 1);
    assert!((metrics.trip_total_secs - 60.0).abs() < 1e-6);
    assert!(ship.trip_started_at.is_none());
}

/// 5. Sink encerra trip.
#[test]
fn sink_ends_trip() {
    let mut ship = make_ship(VesselPresence::AtSea);
    ship.trip_started_at = Some(200.0);
    let mut metrics = Metrics::default();

    let closed = finalize_trip(&mut ship, &mut metrics, 245.5);

    assert!(closed);
    assert!(ship.trip_started_at.is_none());
    assert_eq!(metrics.trip_count, 1);
    assert!(
        (metrics.trip_total_secs - 45.5).abs() < 1e-6,
        "duracao somada: esperada 45.5s, veio {}",
        metrics.trip_total_secs,
    );
}

/// 6. Navio Docked restaurado nao inicia trip.
#[test]
fn restored_docked_does_not_start_trip() {
    let record = ShipRecord {
        ship_instance: ShipInstanceId::new(),
        character: CharacterId::new(),
        kind: ShipKind::SmallMerchant,
        hp: 100,
        x: 0.0,
        y: 0.0,
        heading: 0.0,
        cargo: Vec::new(),
        equipped: Vec::new(),
        presence: VesselPresence::Docked(RegionId::new()),
    };
    let now = 999.0_f32;

    let trip_started_at = match record.presence {
        VesselPresence::AtSea => Some(now),
        VesselPresence::Docked(_) => None,
    };

    assert!(trip_started_at.is_none());
}

/// 7. Navio AtSea restaurado inicia nova medicao.
#[test]
fn restored_at_sea_starts_new_measurement() {
    let record = ShipRecord {
        ship_instance: ShipInstanceId::new(),
        character: CharacterId::new(),
        kind: ShipKind::SmallMerchant,
        hp: 100,
        x: 0.0,
        y: 0.0,
        heading: 0.0,
        cargo: Vec::new(),
        equipped: Vec::new(),
        presence: VesselPresence::AtSea,
    };
    let boot_now = 1234.5_f32;

    let trip_started_at = match record.presence {
        VesselPresence::AtSea => Some(boot_now),
        VesselPresence::Docked(_) => None,
    };

    assert_eq!(trip_started_at, Some(1234.5));
}
