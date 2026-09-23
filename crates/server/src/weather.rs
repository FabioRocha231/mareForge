//! Clima e aparelho (MF-059): o servidor avança o vento e as tempestades,
//! remenda/rasga o pano dos navios, guarda a munição escolhida e anuncia o
//! clima a todos a ~1 Hz. As regras vivem em `domain-ships`/`domain-combat`.

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_ships::sailing::{SAIL_REPAIR_PER_SEC, STORM_SAIL_DAMAGE_PER_SEC};
use mareforge_domain_ships::{VesselPresence, Weather, WeatherBounds, SAIL_HP_MAX};
use mareforge_domain_world::{RiskTier, WorldMap};
use mareforge_protocol::{SelectAmmo, StormState, WeatherUpdate};
use tracing::info;

use crate::net::{ReliableChannel, ServerShip, ServerWorldMap};
use crate::sets::SimulationSet;

/// Clima autoritativo do mundo.
#[derive(Resource)]
pub struct ServerWeather(pub Weather);

/// Área onde tempestades nascem: o triângulo econômico e arredores.
const STORM_BOUNDS: WeatherBounds = WeatherBounds {
    min_x: -1400.0,
    min_y: -700.0,
    max_x: 1400.0,
    max_y: 1500.0,
};
const BROADCAST_EVERY_SECS: f32 = 1.0;

pub fn install(app: &mut App) {
    app.insert_resource(ServerWeather(initial_weather()));
    app.add_systems(FixedUpdate, handle_select_ammo.in_set(SimulationSet::Input));
    app.add_systems(
        FixedUpdate,
        (advance_weather, update_sails)
            .chain()
            .in_set(SimulationSet::Movement),
    );
    app.add_systems(
        FixedUpdate,
        broadcast_weather.in_set(SimulationSet::Snapshot),
    );
}

/// Semente: `MAREFORGE_WEATHER_SEED` (reprodutível) ou o relógio. Dev:
/// `MAREFORGE_STORM_AT=x,y` força uma tempestade de 5 min ali.
fn initial_weather() -> Weather {
    let seed = std::env::var("MAREFORGE_WEATHER_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0)
        });
    let mut weather = Weather::new(seed);
    if let Some((x, y)) = std::env::var("MAREFORGE_STORM_AT")
        .ok()
        .and_then(|value| parse_point(&value))
    {
        weather.spawn_storm_at(x, y, 320.0, 300.0);
        info!(x, y, "dev: tempestade forçada");
    }
    weather
}

fn parse_point(value: &str) -> Option<(f32, f32)> {
    let (x, y) = value.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Tempestade só em águas de risco declaradas: nunca em águas de porto.
pub(crate) fn storm_allowed(map: &WorldMap, x: f32, y: f32) -> bool {
    map.zone_at(x, y)
        .is_ok_and(|zone| zone.tier != RiskTier::Protected)
}

fn advance_weather(time: Res<Time>, map: Res<ServerWorldMap>, mut weather: ResMut<ServerWeather>) {
    weather.0.step(time.delta_secs(), STORM_BOUNDS, |x, y| {
        storm_allowed(&map.0, x, y)
    });
}

/// Atracado: velas remendadas. No mar: remendo lento, e a tempestade rasga.
fn update_sails(
    time: Res<Time>,
    map: Res<ServerWorldMap>,
    weather: Res<ServerWeather>,
    mut ships: Query<&mut ServerShip>,
) {
    let dt = time.delta_secs();
    for mut ship in &mut ships {
        if matches!(ship.presence, VesselPresence::Docked(_)) {
            ship.sail_hp = SAIL_HP_MAX;
            continue;
        }
        let (x, y) = (ship.motion.x, ship.motion.y);
        let storm = if storm_allowed(&map.0, x, y) {
            weather.0.storm_influence(x, y)
        } else {
            0.0
        };
        let delta = (SAIL_REPAIR_PER_SEC - STORM_SAIL_DAMAGE_PER_SEC * storm) * dt;
        ship.sail_hp = (ship.sail_hp + delta).clamp(0.0, SAIL_HP_MAX);
    }
}

/// Tecla C do client: o servidor guarda a munição; vale no próximo disparo.
fn handle_select_ammo(
    mut events: EventReader<ServerReceiveMessage<SelectAmmo>>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in events.read() {
        let client_id = event.from();
        let ammo = event.message().ammo;
        if let Some(mut ship) = ships
            .iter_mut()
            .find(|ship| ship.client_id == Some(client_id))
        {
            if ship.ammo != ammo {
                info!(ship_id = ship.ship_id, ?ammo, "municao trocada");
                ship.ammo = ammo;
            }
        }
    }
}

pub(crate) fn weather_update(weather: &Weather) -> WeatherUpdate {
    let wind = weather.base_wind();
    WeatherUpdate {
        wind_dir: wind.direction,
        wind_strength: wind.strength,
        storms: weather
            .storms()
            .iter()
            .map(|storm| StormState {
                storm_id: storm.id,
                x: storm.x,
                y: storm.y,
                radius: storm.radius,
                intensity: storm.intensity(),
            })
            .collect(),
    }
}

fn broadcast_weather(
    time: Res<Time>,
    mut since: Local<f32>,
    weather: Res<ServerWeather>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    *since += time.delta_secs();
    if *since < BROADCAST_EVERY_SECS {
        return;
    }
    *since = 0.0;
    let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
        &weather_update(&weather.0),
        NetworkTarget::All,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storms_are_forbidden_in_port_waters_and_off_map() {
        let map = WorldMap::vertical_slice();
        assert!(!storm_allowed(&map, -600.0, 0.0), "Porto da Serra");
        assert!(!storm_allowed(&map, 600.0, 0.0), "Porto da Mina");
        assert!(storm_allowed(&map, 0.0, 900.0), "Coral Negro");
        assert!(!storm_allowed(&map, 20_000.0, 0.0), "fora do mar declarado");
    }

    #[test]
    fn live_map_storms_never_touch_port_waters() {
        let map = WorldMap::vertical_slice();
        let mut weather = Weather::new(99);
        let dt = 1.0 / 30.0;
        for _ in 0..(30 * 60 * 30) {
            weather.step(dt, STORM_BOUNDS, |x, y| storm_allowed(&map, x, y));
            for storm in weather.storms().iter().filter(|s| s.age <= dt * 1.5) {
                for (px, py) in [(-600.0_f32, 0.0_f32), (600.0, 0.0)] {
                    let d = ((storm.x - px).powi(2) + (storm.y - py).powi(2)).sqrt();
                    assert!(d > storm.radius, "{storm:?} nasceu sobre o porto");
                }
            }
        }
    }

    #[test]
    fn weather_update_mirrors_the_weather() {
        let mut weather = Weather::new(1);
        weather.spawn_storm_at(0.0, 900.0, 300.0, 200.0);
        let update = weather_update(&weather);
        assert_eq!(update.wind_dir, weather.base_wind().direction);
        assert_eq!(update.storms.len(), 1);
        assert_eq!(update.storms[0].radius, 300.0);
    }

    #[test]
    fn parse_point_reads_dev_coordinates() {
        assert_eq!(parse_point("10, -20.5"), Some((10.0, -20.5)));
        assert_eq!(parse_point("nope"), None);
    }
}
