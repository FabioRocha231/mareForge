use bevy::prelude::*;

use crate::assets::AssetManifestPlugin;
use crate::crafting::{send_craft_input, CraftPlugin};
use crate::market::{send_market_input, spawn_market_panel, KnownCatalog, MarketPlugin, Wallet};
use crate::net::{
    ClientNetPlugin, KnownWrecks, MyDocked, MyShip, GATHER_RADIUS_SQ, LOOT_RADIUS_SQ,
};
use crate::nodes::{KnownNodes, NodePlugin};
use crate::port_screen::PortPlugin;
use crate::ship::{
    expire_stale_visuals, lerp_projectile_visuals, lerp_ship_visuals, update_cargo_readout,
    upsert_projectile_visuals, upsert_ship_visuals, upsert_wreck_visuals, CargoReadout,
};
use crate::zone::{risk_tag, CurrentZone, ZonePlugin};
use mareforge_protocol::ShipState;
use mareforge_shared::ids::ItemDefinitionId;

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.04, 0.13, 0.22)))
            // ADR-0008: simulação a 30 Hz; render desacoplado.
            .insert_resource(Time::<Fixed>::from_hz(30.0))
            .add_plugins(AssetManifestPlugin)
            .add_plugins(ClientNetPlugin)
            .add_plugins(ZonePlugin)
            .add_plugins(NodePlugin)
            .add_plugins(CraftPlugin)
            .add_plugins(MarketPlugin)
            .add_plugins(PortPlugin)
            .add_systems(
                Startup,
                (setup_camera, setup_hud, spawn_market_panel).chain(),
            )
            .add_systems(
                Update,
                (
                    upsert_ship_visuals,
                    lerp_ship_visuals,
                    upsert_projectile_visuals,
                    lerp_projectile_visuals,
                    upsert_wreck_visuals,
                    expire_stale_visuals,
                    update_cargo_readout,
                    update_sea_hud,
                    toggle_sea_hud,
                    crate::ship::follow_camera,
                    send_craft_input,
                    send_market_input,
                    close_on_esc,
                ),
            );
    }
}

fn setup_camera(mut commands: Commands) {
    // 2 px por metro: o navio de 26 m ocupa 52 px e a velocidade de cruzeiro
    // (30 m/s) fica visível — 1:1 fazia o mar parecer congelado.
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 0.5,
            ..OrthographicProjection::default_2d()
        }),
    ));
}

/// Marca os elementos do HUD do mar; atracado, todos escondem juntos.
#[derive(Component)]
pub struct SeaHud;

/// Linha principal do HUD do mar (HP, navio, zona, ouro, recarga).
#[derive(Component)]
pub struct SeaReadout;

/// Prompt contextual de uma ação (atracar/saquear/coletar).
#[derive(Component)]
pub struct PromptReadout;

/// HUD filho da câmera: a câmera segue o navio (follow_camera) e o HUD vai
/// junto — texto em coordenada de mundo some quando se navega.
fn setup_hud(mut commands: Commands, camera: Query<Entity, With<Camera2d>>) {
    let Ok(camera) = camera.get_single() else {
        return;
    };
    let hud = |commands: &mut Commands, text: &str, size: f32, color: Color, y: f32| {
        commands
            .spawn((
                Text2d::new(text.to_owned()),
                TextFont {
                    font_size: size,
                    ..default()
                },
                TextColor(color),
                Transform::from_xyz(0.0, y, 10.0),
                SeaHud,
            ))
            .set_parent(camera)
            .id()
    };
    let sea = hud(
        &mut commands,
        "HP: —\nNavio: —\nZona: —\nOuro: —\nBombordo: —\nEstibordo: —",
        13.0,
        Color::srgb(0.85, 0.85, 0.85),
        140.0,
    );
    commands.entity(sea).insert(SeaReadout);
    let cargo = hud(
        &mut commands,
        "Carga: —",
        13.0,
        Color::srgb(0.95, 0.8, 0.5),
        75.0,
    );
    commands.entity(cargo).insert(CargoReadout);
    let prompt = hud(&mut commands, "", 13.0, Color::srgb(0.75, 0.92, 0.72), 35.0);
    commands.entity(prompt).insert(PromptReadout);
}

/// Atracado, o HUD do mar some; a tela de porto (MF-042) assume o espaço.
fn toggle_sea_hud(docked: Res<MyDocked>, mut hud: Query<&mut Visibility, With<SeaHud>>) {
    let visibility = if docked.0 {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    for mut entity in &mut hud {
        *entity = visibility;
    }
}

fn cooldown_label(secs: f32) -> String {
    if secs <= 0.0 {
        String::from("pronto")
    } else {
        format!("{:.0}s", secs.ceil())
    }
}

fn sea_hud_text(state: &ShipState, wallet: u64, zone: &CurrentZone) -> String {
    let zone_line = zone
        .0
        .as_ref()
        .map(|zone| format!("{} — {}", zone.name, risk_tag(zone.tier)))
        .unwrap_or_else(|| String::from("Zona: fora do mar"));
    format!(
        "HP: {}/{}\nNavio: {}\n{}\nOuro: {}g\nBombordo: {}\nEstibordo: {}",
        state.hp,
        state.max_hp,
        state.ship_id,
        zone_line,
        wallet,
        cooldown_label(state.port_cooldown_secs),
        cooldown_label(state.starboard_cooldown_secs),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HudContext {
    Idle,
    NearPort,
    NearWreck,
    NearNode(ItemDefinitionId),
}

/// Proximidade local do próprio navio. O porto ainda não chega como
/// geometria ao client; o servidor envia a zona, e as águas de porto usam
/// esse nome como aproximação.
fn hud_context(
    pos: Vec2,
    zone: &CurrentZone,
    wrecks: &KnownWrecks,
    nodes: &KnownNodes,
    catalog: &KnownCatalog,
) -> HudContext {
    if zone
        .0
        .as_ref()
        .is_some_and(|zone| zone.name.starts_with("Águas do Porto"))
    {
        return HudContext::NearPort;
    }
    if wrecks
        .0
        .values()
        .any(|wreck| pos.distance_squared(*wreck) <= LOOT_RADIUS_SQ)
    {
        return HudContext::NearWreck;
    }
    let nearest = nodes
        .0
        .values()
        .filter(|info| info.stock > 0 && pos.distance_squared(info.pos) <= GATHER_RADIUS_SQ)
        .min_by(|a, b| {
            let da = pos.distance_squared(a.pos);
            let db = pos.distance_squared(b.pos);
            da.total_cmp(&db)
        });
    if let Some(node) = nearest {
        if let Some(line) = catalog.0.get(&node.resource_name) {
            return HudContext::NearNode(line.id);
        }
    }
    HudContext::Idle
}

fn context_prompt(context: &HudContext, catalog: &KnownCatalog) -> String {
    match context {
        HudContext::Idle => String::new(),
        HudContext::NearPort => String::from("E — Atracar"),
        HudContext::NearWreck => String::from("F — Saquear destroço"),
        HudContext::NearNode(item) => {
            let name = catalog
                .0
                .values()
                .find(|line| line.id == *item)
                .map(|line| line.name.as_str())
                .unwrap_or("recurso");
            format!("G — Coletar {name}")
        }
    }
}

/// O HUD lê só o snapshot autoritativo (ShipVisual) e os recursos do
/// servidor; nenhuma verdade de mundo é inferida aqui.
#[allow(clippy::too_many_arguments)]
fn update_sea_hud(
    my_ship: Res<MyShip>,
    wallet: Res<Wallet>,
    zone: Res<CurrentZone>,
    wrecks: Res<KnownWrecks>,
    nodes: Res<KnownNodes>,
    catalog: Res<KnownCatalog>,
    visuals: Query<&crate::ship::ShipVisual>,
    mut texts: Query<(&mut Text2d, Option<&SeaReadout>, Option<&PromptReadout>)>,
) {
    let Some(my_id) = my_ship.0 else {
        return;
    };
    let Some(visual) = visuals.iter().find(|visual| visual.target.ship_id == my_id) else {
        return;
    };
    let state = &visual.target;
    let context = hud_context(
        Vec2::new(state.x, state.y),
        &zone,
        &wrecks,
        &nodes,
        &catalog,
    );
    for (mut text, sea, prompt) in &mut texts {
        if sea.is_some() {
            text.0 = sea_hud_text(state, wallet.0, &zone);
        }
        if prompt.is_some() {
            text.0 = context_prompt(&context, &catalog);
        }
    }
}

fn close_on_esc(
    keys: Res<ButtonInput<KeyCode>>,
    docked: Res<MyDocked>,
    mut exit: EventWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) && !docked.0 {
        exit.send(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Instant;

    use bevy::ecs::system::RunSystemOnce;
    use mareforge_domain_world::RiskTier;
    use mareforge_protocol::ItemLine;

    use crate::net::KnownWrecks;
    use crate::nodes::NodeInfo;
    use crate::ship::ShipVisual;
    use crate::zone::ServerZone;

    use super::*;

    fn ship_state(port_cooldown: f32, starboard_cooldown: f32) -> ShipState {
        ShipState {
            ship_id: 1,
            x: 0.0,
            y: 0.0,
            heading: 0.0,
            speed: 0.0,
            cargo_weight: 8,
            hp: 120,
            max_hp: 150,
            max_speed: 30.0,
            weapon_damage: 20,
            weapon_range: 50.0,
            port_cooldown_secs: port_cooldown,
            starboard_cooldown_secs: starboard_cooldown,
            is_npc: false,
        }
    }

    #[test]
    fn sea_hud_text_contains_hp_zone_gold_and_cooldowns() {
        let state = ship_state(3.2, 0.0);
        let zone = CurrentZone(Some(ServerZone {
            tier: RiskTier::Frontier,
            name: String::from("Rota da Costa"),
        }));

        let text = sea_hud_text(&state, 500, &zone);
        assert!(text.contains("HP: 120/150"), "{text}");
        assert!(text.contains("Navio: 1"), "{text}");
        assert!(text.contains("Rota da Costa"), "{text}");
        assert!(text.contains("PvP ATIVO"), "{text}");
        assert!(text.contains("Ouro: 500g"), "{text}");
        assert!(text.contains("Bombordo: 4s"), "{text}");
        assert!(text.contains("Estibordo: pronto"), "{text}");
    }

    #[test]
    fn snapshot_system_updates_sea_hud() {
        let mut world = World::new();
        let item = ItemDefinitionId::new();
        world.insert_resource(MyShip(Some(1)));
        world.insert_resource(Wallet(500));
        world.insert_resource(CurrentZone(Some(ServerZone {
            tier: RiskTier::Protected,
            name: String::from("Águas do Porto da Serra"),
        })));
        world.insert_resource(KnownWrecks(HashMap::new()));
        world.insert_resource(KnownNodes(HashMap::from([(
            7,
            NodeInfo {
                pos: Vec2::new(0.0, 0.0),
                stock: 10,
                resource_name: String::from("Madeira"),
            },
        )])));
        world.insert_resource(KnownCatalog(HashMap::from([(
            String::from("Madeira"),
            ItemLine {
                id: item,
                name: String::from("Madeira"),
                weight: 2,
                equipment_slot: None,
            },
        )])));
        world.spawn((SeaReadout, Text2d::new(String::new())));
        world.spawn((PromptReadout, Text2d::new(String::new())));
        world.spawn((CargoReadout, Text2d::new(String::new())));
        world.spawn((ShipVisual {
            target: ship_state(0.0, 0.0),
            last_seen: Instant::now(),
        },));

        world.run_system_once(update_sea_hud).unwrap();
        world.run_system_once(update_cargo_readout).unwrap();

        let mut texts = world.query::<&Text2d>();
        let text = texts
            .iter(&world)
            .map(|text| text.0.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("HP: 120/150"), "{text}");
        assert!(text.contains("Águas do Porto da Serra"), "{text}");
        assert!(text.contains("Carga: 8"), "{text}");
        assert!(text.contains("Ouro: 500g"), "{text}");
        assert!(text.contains("E — Atracar"), "{text}");
    }

    #[test]
    fn docked_hides_sea_hud() {
        let mut world = World::new();
        world.insert_resource(MyDocked(true));
        let hud = world.spawn((SeaHud, Visibility::Visible)).id();

        world.run_system_once(toggle_sea_hud).unwrap();
        assert_eq!(*world.get::<Visibility>(hud).unwrap(), Visibility::Hidden);

        world.insert_resource(MyDocked(false));
        world.run_system_once(toggle_sea_hud).unwrap();
        assert_eq!(*world.get::<Visibility>(hud).unwrap(), Visibility::Visible);
    }

    #[test]
    fn sea_hud_text_has_no_recipe_list() {
        let text = sea_hud_text(&ship_state(0.0, 0.0), 0, &CurrentZone::default());
        assert!(!text.contains("Receitas"), "{text}");
        assert!(!text.contains("1-9"), "{text}");
    }
}
