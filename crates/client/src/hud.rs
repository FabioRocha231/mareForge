//! HUD do mar (MF-057A, MF-057B, MF-057C, MF-057O, MF-057R).
//! Quatro paineis discretos nos cantos da tela, banner momentaneo de zona
//! e placa de aviso de PvP com fade. Tudo eh filho da Camera2d para navegar
//! com o navio. Atracado, o HUD do mar esconde; o Port Screen assume.

use bevy::prelude::*;
use mareforge_shared::ids::ItemDefinitionId;

use crate::assets::{layers, GameAssets};
use crate::crafting::KnownRecipes;
use crate::market::{KnownCatalog, Wallet};
use crate::net::{KnownWrecks, MyShip, GATHER_RADIUS_SQ, LOOT_RADIUS_SQ};
use crate::nodes::KnownNodes;
use crate::zone::CurrentZone;

/// Tudo que vive no HUD do mar. Atracado, todos os paineis escondem juntos.
#[derive(Component)]
pub struct SeaHud;

#[derive(Component)]
pub struct ShipPanel;
#[derive(Component)]
pub struct ZonePanel;
#[derive(Component)]
pub struct CooldownPanel;
#[derive(Component)]
pub struct PromptPanel;

/// Sprite de icone anexado a uma linha do HUD.
#[derive(Component)]
pub struct HudIcon;

/// Marcador do texto de banner momentaneo de zona (zone_changed -> fade).
#[derive(Component)]
pub struct ZoneBannerText;

/// Marcador da placa de fundo do banner momentaneo de zona.
#[derive(Component)]
pub struct ZoneBannerPanel;

/// Marcador da placa de fundo do aviso de PvP com fade.
#[derive(Component)]
pub struct PvpWarningPanel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudContext {
    Idle,
    NearPort,
    NearWreck,
    NearNode(ItemDefinitionId),
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_ship_panel,
                update_zone_panel,
                update_cooldown_panel,
                update_prompt_panel,
                tick_zone_banner,
                tick_pvp_warning,
            ),
        );
    }
}

fn panel_pos(local_x: f32, local_y: f32) -> Vec3 {
    Vec3::new(local_x, local_y, layers::HUD)
}

fn spawn_hud_line(
    parent: &mut ChildBuilder,
    icon: Handle<Image>,
    text: &str,
    font_size: f32,
    color: Color,
    local_offset: Vec3,
) {
    parent.spawn((
        Sprite {
            image: icon,
            color: Color::WHITE,
            ..default()
        },
        Transform::from_translation(Vec3::new(local_offset.x, local_offset.y, layers::HUD + 0.1)),
        HudIcon,
    ));
    parent.spawn((
        Text2d::new(text.to_owned()),
        TextFont {
            font_size,
            ..default()
        },
        TextColor(color),
        Transform::from_translation(Vec3::new(
            local_offset.x + 16.0,
            local_offset.y,
            layers::HUD + 0.1,
        )),
    ));
}

pub fn setup_hud(
    mut commands: Commands,
    camera: Query<Entity, With<Camera2d>>,
    assets: Res<GameAssets>,
) {
    let Ok(camera) = camera.get_single() else {
        return;
    };
    let win_w = 1280.0_f32;
    let win_h = 720.0_f32;
    let half_w = win_w / 2.0 * 0.5;
    let half_h = win_h / 2.0 * 0.5;

    let ship_origin = Vec2::new(-half_w + 96.0, half_h - 56.0);
    commands
        .spawn((
            Sprite {
                image: assets.panel_ship.clone(),
                color: Color::WHITE,
                ..default()
            },
            Transform::from_translation(panel_pos(ship_origin.x, ship_origin.y)),
            SeaHud,
            ShipPanel,
        ))
        .with_children(|parent| {
            spawn_hud_line(
                parent,
                assets.icon_ship.clone(),
                "Mercante",
                14.0,
                Color::srgb(0.95, 0.92, 0.78),
                Vec3::new(-78.0, 28.0, 0.0),
            );
            spawn_hud_line(
                parent,
                assets.icon_hp.clone(),
                "120/150",
                13.0,
                Color::srgb(0.7, 0.95, 0.75),
                Vec3::new(-78.0, 8.0, 0.0),
            );
            spawn_hud_line(
                parent,
                assets.icon_cargo.clone(),
                "0/100",
                13.0,
                Color::srgb(0.85, 0.85, 0.85),
                Vec3::new(-78.0, -12.0, 0.0),
            );
            spawn_hud_line(
                parent,
                assets.icon_gold.clone(),
                "0g",
                13.0,
                Color::srgb(0.95, 0.85, 0.55),
                Vec3::new(-78.0, -32.0, 0.0),
            );
        });

    let zone_origin = Vec2::new(half_w - 110.0, half_h - 36.0);
    commands
        .spawn((
            Sprite {
                image: assets.panel_zone.clone(),
                color: Color::WHITE,
                ..default()
            },
            Transform::from_translation(panel_pos(zone_origin.x, zone_origin.y)),
            SeaHud,
            ZonePanel,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text2d::new(String::from("Aguas da Ilha")),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.92, 1.0)),
                Transform::from_translation(Vec3::new(-90.0, 8.0, layers::HUD + 0.1)),
            ));
            parent.spawn((
                Text2d::new(String::from("protegido")),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.85, 0.65)),
                Transform::from_translation(Vec3::new(-90.0, -10.0, layers::HUD + 0.1)),
            ));
        });

    let cd_origin = Vec2::new(-half_w + 76.0, -half_h + 32.0);
    commands
        .spawn((
            Sprite {
                image: assets.panel_cooldowns.clone(),
                color: Color::WHITE,
                ..default()
            },
            Transform::from_translation(panel_pos(cd_origin.x, cd_origin.y)),
            SeaHud,
            CooldownPanel,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text2d::new(String::from("BOM  pronto")),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
                Transform::from_translation(Vec3::new(-58.0, 6.0, layers::HUD + 0.1)),
            ));
            parent.spawn((
                Text2d::new(String::from("EST  pronto")),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
                Transform::from_translation(Vec3::new(-58.0, -10.0, layers::HUD + 0.1)),
            ));
        });

    let prompt_origin = Vec2::new(half_w - 140.0, -half_h + 26.0);
    commands
        .spawn((
            Sprite {
                image: assets.panel_prompt.clone(),
                color: Color::WHITE,
                ..default()
            },
            Transform::from_translation(panel_pos(prompt_origin.x, prompt_origin.y)),
            SeaHud,
            PromptPanel,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text2d::new(String::new()),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.75, 0.92, 0.72)),
                Transform::from_translation(Vec3::new(-118.0, 0.0, layers::HUD + 0.1)),
            ));
        });

    let _ = camera;
}

fn cooldown_label(secs: f32) -> String {
    if secs <= 0.0 {
        String::from("pronto")
    } else {
        format!("{:.0}s", secs.ceil())
    }
}

fn hp_color(current: u32, max: u32) -> Color {
    if max == 0 {
        return Color::srgb(0.95, 0.65, 0.55);
    }
    let ratio = current as f32 / max as f32;
    if ratio > 0.6 {
        Color::srgb(0.7, 0.95, 0.75)
    } else if ratio > 0.3 {
        Color::srgb(0.95, 0.85, 0.55)
    } else {
        Color::srgb(0.95, 0.55, 0.5)
    }
}

fn ship_kind_label(kind: mareforge_domain_ships::ShipKind) -> &'static str {
    use mareforge_domain_ships::ShipKind;
    match kind {
        ShipKind::SmallMerchant => "Mercante",
        ShipKind::Patrol => "Patrulha",
        ShipKind::Corsair => "Corsario",
    }
}

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
        .is_some_and(|zone| zone.name.starts_with("Aguas do Porto"))
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
        HudContext::NearPort => String::from("E - Atracar"),
        HudContext::NearWreck => String::from("F - Saquear destroco"),
        HudContext::NearNode(item) => {
            let name = catalog
                .0
                .values()
                .find(|line| line.id == *item)
                .map(|line| line.name.as_str())
                .unwrap_or("recurso");
            format!("G - Coletar {name}")
        }
    }
}

/// Retorna as entidades Text2d filhas do painel na ordem do spawn.
fn text_children(children: &Children, texts: &Query<&mut Text2d>) -> Vec<Entity> {
    children
        .iter()
        .filter(|c| texts.get(**c).is_ok())
        .copied()
        .collect()
}

#[allow(clippy::type_complexity)]
pub fn update_ship_panel(
    my_ship: Res<MyShip>,
    wallet: Res<Wallet>,
    visuals: Query<&crate::ship::ShipVisual>,
    panel: Query<&Children, With<ShipPanel>>,
    mut texts: Query<&mut Text2d>,
    mut icons: Query<&mut TextColor, With<HudIcon>>,
) {
    let Some(my_id) = my_ship.0 else {
        return;
    };
    let Some(visual) = visuals.iter().find(|v| v.target.ship_id == my_id) else {
        return;
    };
    let state = &visual.target;
    let Ok(children) = panel.get_single() else {
        return;
    };
    let text_entities = text_children(children, &texts);
    if text_entities.len() >= 4 {
        if let Ok([mut t0, mut t1, mut t2, mut t3]) = texts.get_many_mut([
            text_entities[0],
            text_entities[1],
            text_entities[2],
            text_entities[3],
        ]) {
            t0.0 = ship_kind_label(state.kind).to_owned();
            t1.0 = format!("{}/{}", state.hp, state.max_hp);
            t2.0 = format!("{}/{}", state.cargo_weight, state.cargo_capacity);
            t3.0 = format!("{}g", wallet.0);
        }
    }
    let hp_color_v = hp_color(state.hp, state.max_hp);
    if let Some(child) = children.get(1) {
        if let Ok(mut tc) = icons.get_mut(*child) {
            tc.0 = hp_color_v;
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn update_zone_panel(
    zone: Res<CurrentZone>,
    panel: Query<&Children, With<ZonePanel>>,
    mut texts: Query<&mut Text2d>,
    mut icons: Query<&mut TextColor, With<HudIcon>>,
) {
    if !zone.is_changed() {
        return;
    }
    let Some(zone) = zone.0.as_ref() else {
        return;
    };
    let name = short_zone_name(&zone.name);
    let tag_color = match zone.tier {
        mareforge_domain_world::RiskTier::Protected => Color::srgb(0.55, 0.85, 0.65),
        mareforge_domain_world::RiskTier::Frontier => Color::srgb(0.95, 0.75, 0.45),
        mareforge_domain_world::RiskTier::Lawless => Color::srgb(0.95, 0.55, 0.5),
    };
    let tag = match zone.tier {
        mareforge_domain_world::RiskTier::Protected => "protegido",
        mareforge_domain_world::RiskTier::Frontier => "pvp ativo",
        mareforge_domain_world::RiskTier::Lawless => "pvp ativo",
    };
    let Ok(children) = panel.get_single() else {
        return;
    };
    let text_entities = text_children(children, &texts);
    if text_entities.len() >= 2 {
        if let Ok([mut t0, mut t1]) = texts.get_many_mut([text_entities[0], text_entities[1]]) {
            t0.0 = name;
            t1.0 = tag.to_owned();
        }
    }
    for child in children.iter() {
        if let Ok(mut tc) = icons.get_mut(*child) {
            tc.0 = tag_color;
        }
    }
}

fn short_zone_name(full: &str) -> String {
    if let Some(rest) = full.strip_prefix("Aguas do Porto ") {
        return format!("P. {rest}");
    }
    if let Some(rest) = full.strip_prefix("Aguas da Ilha do ") {
        return rest.to_owned();
    }
    full.to_owned()
}

#[allow(clippy::type_complexity)]
pub fn update_cooldown_panel(
    my_ship: Res<MyShip>,
    visuals: Query<&crate::ship::ShipVisual>,
    panel: Query<&Children, With<CooldownPanel>>,
    mut texts: Query<&mut Text2d>,
) {
    let Some(my_id) = my_ship.0 else {
        return;
    };
    let Some(visual) = visuals.iter().find(|v| v.target.ship_id == my_id) else {
        return;
    };
    let bom = cooldown_label(visual.target.port_cooldown_secs);
    let est = cooldown_label(visual.target.starboard_cooldown_secs);
    let Ok(children) = panel.get_single() else {
        return;
    };
    let text_entities = text_children(children, &texts);
    if text_entities.len() >= 2 {
        if let Ok([mut t0, mut t1]) = texts.get_many_mut([text_entities[0], text_entities[1]]) {
            t0.0 = format!("BOM  {bom}");
            t1.0 = format!("EST  {est}");
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_prompt_panel(
    my_ship: Res<MyShip>,
    zone: Res<CurrentZone>,
    wrecks: Res<KnownWrecks>,
    nodes: Res<KnownNodes>,
    catalog: Res<KnownCatalog>,
    visuals: Query<&crate::ship::ShipVisual>,
    panel: Query<&Children, With<PromptPanel>>,
    mut texts: Query<&mut Text2d>,
) {
    let Some(my_id) = my_ship.0 else {
        return;
    };
    let Some(visual) = visuals.iter().find(|v| v.target.ship_id == my_id) else {
        return;
    };
    let pos = Vec2::new(visual.target.x, visual.target.y);
    let context = hud_context(pos, &zone, &wrecks, &nodes, &catalog);
    let prompt = context_prompt(&context, &catalog);
    let Ok(children) = panel.get_single() else {
        return;
    };
    for child in children.iter() {
        if let Ok(mut t) = texts.get_mut(*child) {
            t.0 = prompt.clone();
        }
    }
}

pub fn toggle_sea_hud(
    docked: Res<crate::net::MyDocked>,
    mut hud: Query<&mut Visibility, With<SeaHud>>,
) {
    let visibility = if docked.0 {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    for mut entity in &mut hud {
        *entity = visibility;
    }
}

#[allow(dead_code)]
fn _recipes_unused(_recipes: &KnownRecipes) {}

/// Spawna o banner momentaneo de zona (fade-in/out) ao entrar em uma nova
/// regiao. Vive enquanto o timer estiver ativo; some depois.
pub fn spawn_zone_banner(commands: &mut Commands, camera: Entity, assets: &GameAssets, name: &str) {
    commands
        .spawn((
            Sprite {
                image: assets.panel_zone.clone(),
                color: Color::srgba(1.0, 1.0, 1.0, 0.0),
                ..default()
            },
            Transform::from_translation(Vec3::new(0.0, 220.0, layers::HUD + 5.0)),
            ZoneBannerPanel,
            ZoneBannerFade::default(),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text2d::new(name.to_owned()),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::srgba(0.95, 0.95, 0.95, 0.0)),
                Transform::from_translation(Vec3::new(0.0, 0.0, layers::HUD + 5.1)),
                ZoneBannerText,
            ));
        })
        .set_parent(camera);
}

#[derive(Component, Default)]
pub struct ZoneBannerFade {
    pub elapsed: f32,
}

/// Spawna a placa de aviso de PvP com fade out.
pub fn spawn_pvp_warning(commands: &mut Commands, camera: Entity, assets: &GameAssets) {
    commands
        .spawn((
            Sprite {
                image: assets.panel_warning.clone(),
                color: Color::srgba(1.0, 0.6, 0.5, 0.0),
                ..default()
            },
            Transform::from_translation(Vec3::new(0.0, -40.0, layers::OVERLAY)),
            PvpWarningPanel,
            PvpWarningFade::default(),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text2d::new(String::from("AGUAS DE RISCO")),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.85, 0.65, 0.0)),
                Transform::from_translation(Vec3::new(0.0, 18.0, layers::OVERLAY + 0.1)),
                PvpWarningText,
            ));
            parent.spawn((
                Text2d::new(String::from(
                    "Seu navio, equipamentos e carga podem ser perdidos.",
                )),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgba(0.95, 0.85, 0.7, 0.0)),
                Transform::from_translation(Vec3::new(0.0, -8.0, layers::OVERLAY + 0.1)),
                PvpWarningSubText,
            ));
        })
        .set_parent(camera);
}

#[derive(Component)]
pub struct PvpWarningText;

#[derive(Component)]
pub struct PvpWarningSubText;

#[derive(Component, Default)]
pub struct PvpWarningFade {
    pub elapsed: f32,
}

pub fn tick_zone_banner(
    time: Res<Time>,
    mut commands: Commands,
    mut banners: Query<(Entity, &mut ZoneBannerFade, &mut Sprite, &Children)>,
    mut texts: Query<&mut TextColor, With<ZoneBannerText>>,
) {
    for (entity, mut fade, mut sprite, children) in &mut banners {
        fade.elapsed += time.delta_secs();
        let alpha = if fade.elapsed < 0.4 {
            fade.elapsed / 0.4
        } else if fade.elapsed < 2.4 {
            1.0
        } else if fade.elapsed < 3.2 {
            1.0 - (fade.elapsed - 2.4) / 0.8
        } else {
            0.0
        };
        let srgba = sprite.color.to_srgba();
        sprite.color = Color::srgba(srgba.red, srgba.green, srgba.blue, alpha);
        for child in children.iter() {
            if let Ok(mut tc) = texts.get_mut(*child) {
                let c = tc.0.to_srgba();
                tc.0 = Color::srgba(c.red, c.green, c.blue, alpha);
            }
        }
        if fade.elapsed >= 3.2 {
            commands.entity(entity).despawn();
        }
    }
}

pub fn tick_pvp_warning(
    time: Res<Time>,
    mut commands: Commands,
    mut plates: Query<(Entity, &mut PvpWarningFade, &mut Sprite, &Children)>,
    mut titles: Query<&mut TextColor, With<PvpWarningText>>,
    mut subs: Query<&mut TextColor, With<PvpWarningSubText>>,
) {
    for (entity, mut fade, mut sprite, children) in &mut plates {
        fade.elapsed += time.delta_secs();
        let alpha = if fade.elapsed < 0.5 {
            fade.elapsed / 0.5
        } else if fade.elapsed < 4.0 {
            1.0
        } else if fade.elapsed < 5.0 {
            1.0 - (fade.elapsed - 4.0) / 1.0
        } else {
            0.0
        };
        let srgba = sprite.color.to_srgba();
        sprite.color = Color::srgba(srgba.red, srgba.green, srgba.blue, alpha);
        for child in children.iter() {
            if let Ok(mut tc) = titles.get_mut(*child) {
                let c = tc.0.to_srgba();
                tc.0 = Color::srgba(c.red, c.green, c.blue, alpha);
            }
            if let Ok(mut tc) = subs.get_mut(*child) {
                let c = tc.0.to_srgba();
                tc.0 = Color::srgba(c.red, c.green, c.blue, alpha);
            }
        }
        if fade.elapsed >= 5.0 {
            commands.entity(entity).despawn();
        }
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

    fn ship_state(port_cooldown: f32, starboard_cooldown: f32) -> mareforge_protocol::ShipState {
        mareforge_protocol::ShipState {
            ship_id: 1,
            kind: mareforge_domain_ships::ShipKind::SmallMerchant,
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
            cargo_capacity: 100,
        }
    }

    #[test]
    fn cooldown_label_is_human_readable() {
        assert_eq!(cooldown_label(0.0), "pronto");
        assert_eq!(cooldown_label(0.4), "1s");
        assert_eq!(cooldown_label(3.6), "4s");
    }

    #[test]
    fn hp_color_thresholds_are_well_ordered() {
        let high = hp_color(100, 100);
        let mid = hp_color(40, 100);
        let low = hp_color(10, 100);
        assert_ne!(high, mid);
        assert_ne!(mid, low);
    }

    #[test]
    fn short_zone_name_strips_hulls() {
        assert_eq!(short_zone_name("Aguas do Porto da Serra"), "P. da Serra");
        assert_eq!(short_zone_name("Aguas do Porto da Mina"), "P. da Mina");
        assert_eq!(
            short_zone_name("Aguas da Ilha do Coral Negro"),
            "Coral Negro"
        );
        assert_eq!(short_zone_name("Rota da Costa"), "Rota da Costa");
    }

    #[test]
    fn context_prompt_is_empty_when_idle() {
        let prompt = context_prompt(&HudContext::Idle, &KnownCatalog::default());
        assert!(prompt.is_empty());
    }

    #[test]
    fn context_prompt_uses_port_label_when_near_port() {
        let prompt = context_prompt(&HudContext::NearPort, &KnownCatalog::default());
        assert!(prompt.contains("Atracar"));
    }

    #[test]
    fn context_prompt_names_resource_from_catalog() {
        let id = ItemDefinitionId::new();
        let catalog = KnownCatalog(HashMap::from([(
            String::from("Madeira"),
            ItemLine {
                id,
                name: String::from("Madeira"),
                weight: 2,
                equipment_slot: None,
            },
        )]));
        let prompt = context_prompt(&HudContext::NearNode(id), &catalog);
        assert!(prompt.contains("Madeira"), "{prompt}");
    }

    #[test]
    fn update_cooldown_panel_renders_bom_and_est() {
        let mut world = World::new();
        world.insert_resource(MyShip(Some(1)));
        let parent = world
            .spawn(ShipVisual {
                target: ship_state(3.2, 0.0),
                last_seen: Instant::now(),
            })
            .id();
        let panel_ent = world
            .spawn((CooldownPanel, Transform::default(), Visibility::default()))
            .id();
        world.entity_mut(panel_ent).add_child(parent);
        // First text is BOM line, second is EST line.
        let t0 = world
            .spawn((Text2d::new(String::new()), Transform::default()))
            .id();
        let t1 = world
            .spawn((Text2d::new(String::new()), Transform::default()))
            .id();
        world.entity_mut(panel_ent).add_child(t0);
        world.entity_mut(panel_ent).add_child(t1);

        let mut sched = Schedule::default();
        sched.add_systems(update_cooldown_panel);
        sched.run(&mut world);

        let mut q = world.query::<&Text2d>();
        let texts: Vec<String> = q.iter(&world).map(|t| t.0.clone()).collect();
        assert!(texts.iter().any(|t| t.contains("BOM")), "texts={texts:?}");
        assert!(texts.iter().any(|t| t.contains("EST")), "texts={texts:?}");
        assert!(texts.iter().any(|t| t.contains("4s")), "texts={texts:?}");
        assert!(
            texts.iter().any(|t| t.contains("pronto")),
            "texts={texts:?}"
        );
    }

    #[test]
    fn update_zone_panel_reflects_current_zone() {
        let mut world = World::new();
        world.insert_resource(CurrentZone(Some(ServerZone {
            tier: RiskTier::Frontier,
            name: String::from("Rota da Costa"),
        })));
        let panel_ent = world
            .spawn((ZonePanel, Transform::default(), Visibility::default()))
            .id();
        let t0 = world
            .spawn((Text2d::new(String::new()), Transform::default()))
            .id();
        let t1 = world
            .spawn((Text2d::new(String::new()), Transform::default()))
            .id();
        world.entity_mut(panel_ent).add_child(t0);
        world.entity_mut(panel_ent).add_child(t1);

        let mut sched = Schedule::default();
        sched.add_systems(update_zone_panel);
        sched.run(&mut world);

        let mut q = world.query::<&Text2d>();
        let texts: Vec<String> = q.iter(&world).map(|t| t.0.clone()).collect();
        assert!(
            texts.iter().any(|t| t == "Rota da Costa"),
            "texts={texts:?}"
        );
        assert!(texts.iter().any(|t| t == "pvp ativo"), "texts={texts:?}");
    }

    #[test]
    fn docked_hides_sea_hud() {
        let mut world = World::new();
        world.insert_resource(crate::net::MyDocked(true));
        let hud_ent = world.spawn((SeaHud, Visibility::Visible)).id();

        let mut sched = Schedule::default();
        sched.add_systems(toggle_sea_hud);
        sched.run(&mut world);
        assert_eq!(
            *world.get::<Visibility>(hud_ent).unwrap(),
            Visibility::Hidden
        );

        world.insert_resource(crate::net::MyDocked(false));
        sched.run(&mut world);
        assert_eq!(
            *world.get::<Visibility>(hud_ent).unwrap(),
            Visibility::Visible
        );
    }
}
