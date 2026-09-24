//! Mar profundo no client (MV-061): teclas das ações novas, marcas no mar
//! (evento em curso, X do tesouro, tentáculos do Kraken), ilhas ocultas que
//! aparecem quando avistadas e a linha de estado de bordo no HUD.
//!
//! Tudo aqui é apresentação: o servidor decide se o reparo, a abordagem ou
//! a escavação acontecem (`ActionResult` cai no feed).

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::ClientReceiveMessage;
use marvyr_protocol::{
    BoardShip, DigTreasure, Faction, HireCrew, IslandState, IslandsInSight, SeaEventKind,
    SeaEventState, SeaEventsUpdate, SetRepair, ShipState, TreasureHint, TreasureHints,
};

use crate::assets::layers;
use crate::hud::SeaHud;
use crate::net::{MyDocked, MyShip, ReliableChannel};
use crate::ship::ShipVisual;
use crate::ui;

/// Alcance da abordagem no client (45 m do servidor, com folga do lerp).
const BOARD_RANGE: f32 = 44.0;

#[derive(Resource, Debug, Default)]
pub struct SeaEvents(pub Vec<SeaEventState>);

#[derive(Resource, Debug, Default)]
pub struct TreasureMarks(pub Vec<TreasureHint>);

/// Ilhas ocultas já avistadas nesta sessão (ficam no mapa depois).
#[derive(Resource, Debug, Default)]
pub struct SeenIslands(pub HashMap<u32, IslandState>);

#[derive(Component)]
struct SeaStatusText;
#[derive(Component)]
struct EventBannerText;

pub struct SeafaringPlugin;

impl Plugin for SeafaringPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SeaEvents>()
            .init_resource::<TreasureMarks>()
            .init_resource::<SeenIslands>()
            .add_systems(Startup, setup_sea_hud)
            .add_systems(
                Update,
                (
                    receive_sea_state,
                    send_sea_input,
                    draw_sea_marks,
                    update_sea_hud,
                ),
            );
    }
}

fn setup_sea_hud(mut commands: Commands) {
    commands
        .spawn((
            ui::panel(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(ui::MARGIN),
                bottom: Val::Px(96.0),
                ..default()
            }),
            SeaHud,
        ))
        .with_child((ui::text("", 12.0, ui::TEXT), SeaStatusText));
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(84.0), // abaixo do painel de região
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            SeaHud,
        ))
        .with_child((
            ui::text("", 15.0, ui::AMBER),
            TextLayout::new_with_justify(JustifyText::Center),
            EventBannerText,
        ));
}

#[allow(clippy::too_many_arguments)]
fn receive_sea_state(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut event_updates: EventReader<ClientReceiveMessage<SeaEventsUpdate>>,
    mut hint_updates: EventReader<ClientReceiveMessage<TreasureHints>>,
    mut island_updates: EventReader<ClientReceiveMessage<IslandsInSight>>,
    mut events: ResMut<SeaEvents>,
    mut marks: ResMut<TreasureMarks>,
    mut seen: ResMut<SeenIslands>,
) {
    if let Some(update) = event_updates.read().last() {
        events.0 = update.message().events.clone();
    }
    if let Some(update) = hint_updates.read().last() {
        marks.0 = update.message().hints.clone();
    }
    for update in island_updates.read() {
        for island in &update.message().islands {
            if seen.0.contains_key(&island.island_id) {
                continue;
            }
            info!(island = %island.name, "ilha oculta avistada");
            spawn_island(&mut commands, &mut meshes, &mut materials, island);
            seen.0.insert(island.island_id, island.clone());
        }
    }
}

fn spawn_island(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    island: &IslandState,
) {
    let at = Vec3::new(island.x, island.y, layers::LAND);
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(island.radius + 6.0))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgb(0.86, 0.78, 0.55)))),
        Transform::from_translation(at),
    ));
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(island.radius * 0.7))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgb(0.30, 0.52, 0.28)))),
        Transform::from_translation(at + Vec3::Z * 0.1),
    ));
    commands.spawn((
        Text2d::new(ui::fold(&island.name)),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(ui::TEXT),
        Transform::from_translation(Vec3::new(
            island.x,
            island.y + island.radius + 14.0,
            layers::LABELS,
        )),
    ));
}

fn my_state<'a>(my_ship: &MyShip, visuals: &'a Query<&ShipVisual>) -> Option<&'a ShipState> {
    let id = my_ship.0?;
    visuals
        .iter()
        .find(|visual| visual.target.ship_id == id)
        .map(|visual| &visual.target)
}

/// Navio mais próximo ao alcance da abordagem (o servidor revalida).
pub fn board_target(mine: &ShipState, others: &[ShipState]) -> Option<u32> {
    let at = Vec2::new(mine.x, mine.y);
    others
        .iter()
        .filter(|other| other.ship_id != mine.ship_id && other.faction != Faction::Monster)
        .map(|other| (other.ship_id, at.distance(Vec2::new(other.x, other.y))))
        .filter(|(_, distance)| *distance <= BOARD_RANGE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// K reparo · H abordagem · J cavar · P contratar marujos (no porto).
fn send_sea_input(
    keys: Res<ButtonInput<KeyCode>>,
    my_ship: Res<MyShip>,
    docked: Res<MyDocked>,
    visuals: Query<&ShipVisual>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    let Some(mine) = my_state(&my_ship, &visuals) else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyK) {
        let _ = connection_manager.send_message::<ReliableChannel, _>(&SetRepair {
            active: !mine.repairing,
        });
    }
    if keys.just_pressed(KeyCode::KeyH) {
        let others: Vec<ShipState> = visuals.iter().map(|visual| visual.target).collect();
        if let Some(target_ship_id) = board_target(mine, &others) {
            let _ = connection_manager
                .send_message::<ReliableChannel, _>(&BoardShip { target_ship_id });
        } else {
            // Sem alvo por perto: o servidor responde com o motivo.
            let _ = connection_manager.send_message::<ReliableChannel, _>(&BoardShip {
                target_ship_id: u32::MAX,
            });
        }
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        let _ = connection_manager.send_message::<ReliableChannel, _>(&DigTreasure);
    }
    if keys.just_pressed(KeyCode::KeyP) && docked.0 {
        let count = mine.crew_max.saturating_sub(mine.crew).max(1);
        let _ = connection_manager.send_message::<ReliableChannel, _>(&HireCrew { count });
    }
}

fn event_color(kind: SeaEventKind) -> Color {
    match kind {
        SeaEventKind::Tempest => Color::srgb(0.55, 0.60, 0.95),
        SeaEventKind::TreasureFleet => ui::GOLD,
        SeaEventKind::Kraken => Color::srgb(0.75, 0.30, 0.85),
        SeaEventKind::ContestedTide => Color::srgb(0.45, 0.90, 0.90),
    }
}

fn draw_sea_marks(
    mut gizmos: Gizmos,
    time: Res<Time>,
    events: Res<SeaEvents>,
    marks: Res<TreasureMarks>,
    visuals: Query<&ShipVisual>,
) {
    let t = time.elapsed_secs();
    for event in &events.0 {
        let pulse = 0.35 + 0.15 * (t * 2.0).sin();
        gizmos.circle_2d(
            Isometry2d::from_translation(Vec2::new(event.x, event.y)),
            event.radius,
            event_color(event.kind).with_alpha(pulse),
        );
    }
    for mark in &marks.0 {
        let at = Vec2::new(mark.x, mark.y);
        let arm = 14.0;
        gizmos.line_2d(at - Vec2::splat(arm), at + Vec2::splat(arm), ui::DANGER);
        gizmos.line_2d(
            at + Vec2::new(-arm, arm),
            at + Vec2::new(arm, -arm),
            ui::DANGER,
        );
        gizmos.circle_2d(
            Isometry2d::from_translation(at),
            40.0,
            ui::DANGER.with_alpha(0.4),
        );
    }
    // Tentáculos do Kraken: seis braços ondulando em volta do casco.
    for visual in &visuals {
        let state = &visual.target;
        if state.faction != Faction::Monster {
            continue;
        }
        let center = Vec2::new(state.x, state.y);
        for arm in 0..6 {
            let base = arm as f32 * std::f32::consts::TAU / 6.0 + t * 0.4;
            let mut previous = center;
            for segment in 1..=5 {
                let s = segment as f32;
                let angle = base + (t * 3.0 + s * 0.8 + arm as f32).sin() * 0.35;
                let point = center + Vec2::from_angle(angle) * (s * 9.0);
                gizmos.line_2d(previous, point, Color::srgb(0.55, 0.20, 0.65));
                previous = point;
            }
        }
    }
}

/// Rumo em pontos cardeais (convenção do mapa: +Y norte, +X leste).
pub fn bearing_label(from: Vec2, to: Vec2) -> &'static str {
    let delta = to - from;
    let angle = delta.y.atan2(delta.x).to_degrees().rem_euclid(360.0);
    const LABELS: [&str; 8] = ["L", "NE", "N", "NO", "O", "SO", "S", "SE"];
    LABELS[(((angle + 22.5) / 45.0) as usize) % 8]
}

fn distance_label(meters: f32) -> String {
    if meters >= 1000.0 {
        format!("{:.1} km", meters / 1000.0)
    } else {
        format!("{meters:.0} m")
    }
}

/// Linha de bordo: tripulação, leme, reparo e escavação.
pub fn sea_status_line(state: &ShipState) -> String {
    let mut line = format!(
        "Tripulação {}/{}  ·  Leme {:.0}%",
        state.crew, state.crew_max, state.rudder_hp
    );
    if state.repairing {
        line.push_str("  ·  REPARANDO (K)");
    }
    if state.dig_progress > 0.0 {
        line.push_str(&format!("  ·  Cavando {:.0}%", state.dig_progress * 100.0));
    }
    line.push_str("\nK reparo · H abordar · J cavar · P marujos (porto)");
    line
}

fn update_sea_hud(
    my_ship: Res<MyShip>,
    events: Res<SeaEvents>,
    marks: Res<TreasureMarks>,
    visuals: Query<&ShipVisual>,
    mut status: Query<&mut Text, (With<SeaStatusText>, Without<EventBannerText>)>,
    mut banner: Query<&mut Text, (With<EventBannerText>, Without<SeaStatusText>)>,
) {
    let Some(mine) = my_state(&my_ship, &visuals) else {
        return;
    };
    let here = Vec2::new(mine.x, mine.y);
    for mut text in &mut status {
        let line = ui::fold(&sea_status_line(mine));
        if text.0 != line {
            text.0 = line;
        }
    }
    let mut lines: Vec<String> = events
        .0
        .iter()
        .map(|event| {
            let there = Vec2::new(event.x, event.y);
            let secs = event.remaining_secs as u32;
            format!(
                "{} — {}:{:02}  ·  {} {}",
                event.name.to_uppercase(),
                secs / 60,
                secs % 60,
                distance_label(here.distance(there)),
                bearing_label(here, there)
            )
        })
        .collect();
    lines.extend(marks.0.iter().map(|mark| {
        let there = Vec2::new(mark.x, mark.y);
        format!(
            "Mapa do Tesouro: {}  ·  {} {}",
            mark.island,
            distance_label(here.distance(there)),
            bearing_label(here, there)
        )
    }));
    let joined = ui::fold(&lines.join("\n"));
    for mut text in &mut banner {
        if text.0 != joined {
            text.0 = joined.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(id: u32, x: f32, faction: Faction) -> ShipState {
        ShipState {
            ship_id: id,
            kind: marvyr_domain_ships::ShipKind::SmallMerchant,
            x,
            y: 0.0,
            heading: 0.0,
            speed: 0.0,
            cargo_weight: 0,
            hp: 100,
            max_hp: 100,
            max_speed: 30.0,
            weapon_damage: 10,
            weapon_range: 50.0,
            port_cooldown_secs: 0.0,
            starboard_cooldown_secs: 0.0,
            is_npc: false,
            cargo_capacity: 100,
            sail_hp: 100.0,
            ammo: Default::default(),
            faction,
            notoriety_tier: 0,
            rudder_hp: 80.0,
            crew: 6,
            crew_max: 8,
            repairing: true,
            dig_progress: 0.5,
        }
    }

    #[test]
    fn boarding_picks_nearest_ship_in_reach_never_a_monster() {
        let mine = state(1, 0.0, Faction::Player);
        let others = [
            mine,
            state(2, 40.0, Faction::Merchant),
            state(3, 20.0, Faction::Monster),
            state(4, 30.0, Faction::Pirate),
            state(5, 90.0, Faction::Player),
        ];
        assert_eq!(board_target(&mine, &others), Some(4));
        assert_eq!(
            board_target(&mine, &[mine, state(9, 200.0, Faction::Player)]),
            None
        );
    }

    #[test]
    fn bearings_follow_the_map_compass() {
        assert_eq!(bearing_label(Vec2::ZERO, Vec2::new(0.0, 100.0)), "N");
        assert_eq!(bearing_label(Vec2::ZERO, Vec2::new(100.0, 0.0)), "L");
        assert_eq!(bearing_label(Vec2::ZERO, Vec2::new(-100.0, -100.0)), "SO");
    }

    #[test]
    fn status_line_shows_crew_rudder_and_work() {
        let line = sea_status_line(&state(1, 0.0, Faction::Player));
        assert!(line.contains("Tripulação 6/8"));
        assert!(line.contains("Leme 80%"));
        assert!(line.contains("REPARANDO"));
        assert!(line.contains("Cavando 50%"));
    }
}
