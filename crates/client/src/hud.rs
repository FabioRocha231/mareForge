//! HUD do mar (MF-057A/B/C, MF-058) em bevy_ui: espaço de tela, independente
//! da câmera, do zoom e do tamanho da janela. Painel do navio (topo-esq.),
//! zona (topo-centro), recarga dos bordos + velas + prompt (base-centro),
//! toasts (direita-meio), dica de controles (base-dir.), banner de zona e
//! placa de PvP com fade. Atracado, o HUD do mar esconde; o Port Screen assume.

use bevy::prelude::*;
use mareforge_domain_world::RiskTier;
use mareforge_shared::ids::ItemDefinitionId;

use crate::market::{KnownCatalog, Wallet};
use crate::net::{KnownWrecks, MyShip, GATHER_RADIUS_SQ, LOOT_RADIUS_SQ};
use crate::nodes::KnownNodes;
use crate::ui::{self, UiFade};
use crate::zone::{risk_tag, CurrentZone};

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

/// Texto do indicador de velas/marcha; o texto é dirigido por outro sistema.
#[derive(Component)]
pub struct SailIndicator;

/// Âncora (centro-topo) onde o banner de zona nasce.
#[derive(Component)]
pub struct ZoneBannerAnchor;
/// Âncora (centro) onde a placa de PvP nasce.
#[derive(Component)]
pub struct PvpWarningAnchor;
/// Pilha de toasts de contexto (direita-meio).
#[derive(Component)]
pub struct ToastStack;

#[derive(Component)]
pub struct ZoneBannerPanel;
#[derive(Component)]
pub struct PvpWarningPanel;
/// Toast curto de feedback de ação de contexto (atracar, coletar, saquear).
#[derive(Component)]
pub struct ContextToast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Broadside {
    Port,
    Starboard,
}

/// Qual texto do HUD este nó mostra (um único `Query<&mut Text>` por sistema).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudText {
    ShipName,
    Hp,
    Cargo,
    Gold,
    ZoneName,
    ZoneTag,
    ZoneRisk,
    Reload(Broadside),
    Prompt,
}

/// Qual barra este nó de preenchimento representa.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudFill {
    Hp,
    Cargo,
    Reload(Broadside),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudContext {
    Idle,
    NearPort,
    NearWreck,
    NearNode(ItemDefinitionId),
}

/// Recarga de bordo do servidor (`server::net` tuning.cooldown_secs).
// ponytail: espelha a constante do servidor; mandar no snapshot se virar por navio.
const BROADSIDE_RELOAD_SECS: f32 = 3.0;

pub const CONTROLS_HINT: &str =
    "W/S velas | A/D leme | Q/R canhoes | C municao | E atracar | G coletar | F saquear | roda do mouse: zoom";

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_hud).add_systems(
            Update,
            (
                update_ship_panel,
                update_zone_panel,
                update_cooldown_panel,
                update_prompt_panel,
                update_sail_indicator,
                ui::tick_ui_fades,
            ),
        );
    }
}

/// Nível de pano armado (W/S), lido do `SailLevel` do client.
fn update_sail_indicator(
    sail: Res<crate::net::SailLevel>,
    mut texts: Query<&mut Text, With<SailIndicator>>,
) {
    if !sail.is_changed() {
        return;
    }
    for mut text in &mut texts {
        text.0 = sail_indicator_text(*sail);
    }
}

fn sail_indicator_text(sail: crate::net::SailLevel) -> String {
    let filled = usize::from(sail.0);
    let empty = usize::from(crate::net::SailLevel::MAX) - filled;
    format!(
        "[{}{}]\n{}",
        "#".repeat(filled),
        "-".repeat(empty),
        sail.label()
    )
}

fn anchored(node: Node) -> Node {
    Node {
        position_type: PositionType::Absolute,
        ..node
    }
}

fn spawn_stat_row(parent: &mut ChildBuilder, label: &str, text: HudText, fill: HudFill) {
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                ui::text(label, 11.0, ui::TEXT_DIM),
                Node {
                    width: Val::Px(44.0),
                    ..default()
                },
            ));
            ui::spawn_bar(row, 140.0, ui::OK_GREEN, fill);
            row.spawn((ui::text("-", 12.0, ui::TEXT), text));
        });
}

fn spawn_reload(parent: &mut ChildBuilder, label: &str, side: Broadside) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            width: Val::Px(150.0),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|col| {
            col.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|row| {
                row.spawn(ui::text(label, 11.0, ui::TEXT_DIM));
                row.spawn((
                    ui::text(cooldown_label(0.0), 11.0, ui::OK_GREEN),
                    HudText::Reload(side),
                ));
            });
            ui::spawn_bar(col, 150.0, ui::OK_GREEN, HudFill::Reload(side));
        });
}

pub fn setup_hud(mut commands: Commands) {
    // Topo-esquerda: painel do navio.
    commands
        .spawn((
            ui::panel(anchored(Node {
                left: Val::Px(ui::MARGIN),
                top: Val::Px(ui::MARGIN),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            })),
            SeaHud,
            ShipPanel,
        ))
        .with_children(|panel| {
            panel.spawn((ui::text("-", 15.0, ui::TEXT), HudText::ShipName));
            spawn_stat_row(panel, "CASCO", HudText::Hp, HudFill::Hp);
            spawn_stat_row(panel, "CARGA", HudText::Cargo, HudFill::Cargo);
            panel.spawn((ui::text("0g", 13.0, ui::GOLD), HudText::Gold));
        });

    // Topo-centro: zona atual.
    commands
        .spawn((
            anchored(Node {
                top: Val::Px(ui::MARGIN),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                ..default()
            }),
            SeaHud,
        ))
        .with_children(|row| {
            row.spawn((
                ui::panel(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(2.0),
                    ..default()
                }),
                ZonePanel,
            ))
            .with_children(|panel| {
                panel.spawn((ui::text("-", 16.0, ui::TEXT), HudText::ZoneName));
                panel
                    .spawn(Node {
                        column_gap: Val::Px(8.0),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|tags| {
                        tags.spawn((ui::text("", 12.0, ui::OK_GREEN), HudText::ZoneTag));
                        tags.spawn((ui::text("", 11.0, ui::TEXT_DIM), HudText::ZoneRisk));
                    });
            });
        });

    // Base-centro: prompt de contexto acima da recarga + velas.
    commands
        .spawn((
            anchored(Node {
                bottom: Val::Px(ui::MARGIN),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(8.0),
                ..default()
            }),
            SeaHud,
        ))
        .with_children(|col| {
            col.spawn((
                ui::panel(Node {
                    display: Display::None,
                    ..default()
                }),
                PromptPanel,
            ))
            .with_children(|panel| {
                panel.spawn((ui::text("", 14.0, ui::GOLD), HudText::Prompt));
            });
            col.spawn((
                ui::panel(Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(18.0),
                    ..default()
                }),
                CooldownPanel,
            ))
            .with_children(|panel| {
                spawn_reload(panel, "BOMBORDO (Q)", Broadside::Port);
                panel.spawn((
                    ui::text("VELAS: -", 12.0, ui::TEXT),
                    Node {
                        min_width: Val::Px(96.0),
                        flex_shrink: 0.0,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    TextLayout::new_with_justify(JustifyText::Center),
                    SailIndicator,
                ));
                spawn_reload(panel, "BORESTE (R)", Broadside::Starboard);
            });
        });

    // Base-direita: dica de controles.
    commands
        .spawn((
            ui::panel(anchored(Node {
                right: Val::Px(ui::MARGIN),
                bottom: Val::Px(ui::MARGIN),
                max_width: Val::Px(240.0),
                ..default()
            })),
            SeaHud,
        ))
        .with_children(|panel| {
            panel.spawn(ui::text(CONTROLS_HINT, 11.0, ui::TEXT_DIM));
        });

    // Direita-meio: pilha de toasts.
    commands.spawn((
        anchored(Node {
            right: Val::Px(ui::MARGIN),
            top: Val::Percent(40.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::End,
            row_gap: Val::Px(6.0),
            ..default()
        }),
        SeaHud,
        ToastStack,
    ));

    // Âncoras de banner de zona e aviso de PvP (acima de tudo).
    for (top, anchor_zone) in [(22.0, true), (38.0, false)] {
        let mut anchor = commands.spawn((
            anchored(Node {
                top: Val::Percent(top),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                ..default()
            }),
            GlobalZIndex(10),
        ));
        if anchor_zone {
            anchor.insert(ZoneBannerAnchor);
        } else {
            anchor.insert(PvpWarningAnchor);
        }
    }
}

fn cooldown_label(secs: f32) -> String {
    if secs <= 0.0 {
        String::from("PRONTO")
    } else {
        format!("{:.0}s", secs.ceil())
    }
}

fn reload_fraction(secs: f32) -> f32 {
    1.0 - (secs / BROADSIDE_RELOAD_SECS).clamp(0.0, 1.0)
}

fn hp_color(current: u32, max: u32) -> Color {
    if max == 0 {
        return ui::DANGER;
    }
    let ratio = current as f32 / max as f32;
    if ratio > 0.6 {
        ui::OK_GREEN
    } else if ratio > 0.3 {
        ui::AMBER
    } else {
        ui::DANGER
    }
}

fn ratio(current: u32, max: u32) -> f32 {
    if max == 0 {
        0.0
    } else {
        current as f32 / max as f32
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

fn zone_tag(tier: RiskTier) -> (&'static str, Color) {
    match tier {
        RiskTier::Protected => ("PROTEGIDO", ui::OK_GREEN),
        RiskTier::Frontier => ("FRONTEIRA", ui::AMBER),
        RiskTier::Lawless => ("SEM LEI", ui::DANGER),
    }
}

/// "Águas do Porto da Serra" -> Some("Porto da Serra").
fn port_of_zone(zone_name: &str) -> Option<&str> {
    let rest = zone_name
        .strip_prefix("Águas do ")
        .or_else(|| zone_name.strip_prefix("Aguas do "))?;
    rest.starts_with("Porto").then_some(rest)
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
        .is_some_and(|zone| port_of_zone(&zone.name).is_some())
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

fn context_prompt(context: &HudContext, catalog: &KnownCatalog, port: Option<&str>) -> String {
    match context {
        HudContext::Idle => String::new(),
        HudContext::NearPort => format!("[E] Atracar em {}", port.unwrap_or("porto")),
        HudContext::NearWreck => String::from("[F] Saquear destroço"),
        HudContext::NearNode(item) => {
            let name = catalog
                .0
                .values()
                .find(|line| line.id == *item)
                .map(|line| line.name.as_str())
                .unwrap_or("recurso");
            format!("[G] Coletar {name}")
        }
    }
}

fn my_visual<'a>(
    my_ship: &MyShip,
    visuals: &'a Query<&crate::ship::ShipVisual>,
) -> Option<&'a mareforge_protocol::ShipState> {
    let my_id = my_ship.0?;
    visuals
        .iter()
        .find(|v| v.target.ship_id == my_id)
        .map(|v| &v.target)
}

pub fn update_ship_panel(
    my_ship: Res<MyShip>,
    wallet: Res<Wallet>,
    visuals: Query<&crate::ship::ShipVisual>,
    mut texts: Query<(&mut Text, &HudText)>,
    mut fills: Query<(&mut Node, &mut BackgroundColor, &HudFill)>,
) {
    let Some(state) = my_visual(&my_ship, &visuals) else {
        return;
    };
    for (mut text, kind) in &mut texts {
        let value = match kind {
            HudText::ShipName => ship_kind_label(state.kind).to_owned(),
            HudText::Hp => format!("{}/{}", state.hp, state.max_hp),
            HudText::Cargo => format!("{}/{}", state.cargo_weight, state.cargo_capacity),
            HudText::Gold => format!("{}g", wallet.0),
            _ => continue,
        };
        if text.0 != value {
            text.0 = value;
        }
    }
    for (mut node, mut bg, fill) in &mut fills {
        match fill {
            HudFill::Hp => {
                node.width = ui::bar_width(ratio(state.hp, state.max_hp));
                bg.0 = hp_color(state.hp, state.max_hp);
            }
            HudFill::Cargo => {
                node.width = ui::bar_width(ratio(state.cargo_weight, state.cargo_capacity));
                bg.0 = ui::GOLD.with_alpha(0.85);
            }
            HudFill::Reload(_) => {}
        }
    }
}

pub fn update_zone_panel(
    zone: Res<CurrentZone>,
    mut texts: Query<(&mut Text, &mut TextColor, &HudText)>,
) {
    if !zone.is_changed() {
        return;
    }
    let Some(zone) = zone.0.as_ref() else {
        return;
    };
    let (tag, tag_color) = zone_tag(zone.tier);
    for (mut text, mut color, kind) in &mut texts {
        match kind {
            HudText::ZoneName => text.0 = short_zone_name(&zone.name),
            HudText::ZoneTag => {
                text.0 = tag.to_owned();
                color.0 = tag_color;
            }
            HudText::ZoneRisk => text.0 = risk_tag(zone.tier).to_owned(),
            _ => {}
        }
    }
}

fn short_zone_name(full: &str) -> String {
    // Servidor manda "Águas do Porto da Serra" etc. — encurtamos para caber
    // no painel do HUD.
    if let Some(rest) = full.strip_prefix("Aguas do Porto ") {
        return format!("P. {rest}");
    }
    if let Some(rest) = full.strip_prefix("Águas do Porto ") {
        return format!("P. {rest}");
    }
    if let Some(rest) = full.strip_prefix("Águas da Ilha do ") {
        return rest.to_owned();
    }
    full.to_owned()
}

pub fn update_cooldown_panel(
    my_ship: Res<MyShip>,
    visuals: Query<&crate::ship::ShipVisual>,
    mut texts: Query<(&mut Text, &mut TextColor, &HudText)>,
    mut fills: Query<(&mut Node, &mut BackgroundColor, &HudFill)>,
) {
    let Some(state) = my_visual(&my_ship, &visuals) else {
        return;
    };
    let secs = |side: Broadside| match side {
        Broadside::Port => state.port_cooldown_secs,
        Broadside::Starboard => state.starboard_cooldown_secs,
    };
    for (mut text, mut color, kind) in &mut texts {
        if let HudText::Reload(side) = kind {
            let s = secs(*side);
            text.0 = cooldown_label(s);
            color.0 = if s <= 0.0 { ui::OK_GREEN } else { ui::TEXT_DIM };
        }
    }
    for (mut node, mut bg, fill) in &mut fills {
        if let HudFill::Reload(side) = fill {
            let s = secs(*side);
            node.width = ui::bar_width(reload_fraction(s));
            bg.0 = if s <= 0.0 { ui::OK_GREEN } else { ui::AMBER };
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_prompt_panel(
    my_ship: Res<MyShip>,
    zone: Res<CurrentZone>,
    wrecks: Res<KnownWrecks>,
    nodes: Res<KnownNodes>,
    catalog: Res<KnownCatalog>,
    visuals: Query<&crate::ship::ShipVisual>,
    mut panel: Query<&mut Node, With<PromptPanel>>,
    mut texts: Query<(&mut Text, &HudText)>,
) {
    let Some(state) = my_visual(&my_ship, &visuals) else {
        return;
    };
    let pos = Vec2::new(state.x, state.y);
    let context = hud_context(pos, &zone, &wrecks, &nodes, &catalog);
    let port = zone.0.as_ref().and_then(|zone| port_of_zone(&zone.name));
    let prompt = context_prompt(&context, &catalog, port);
    let display = if prompt.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    for mut node in &mut panel {
        if node.display != display {
            node.display = display;
        }
    }
    for (mut text, kind) in &mut texts {
        if *kind == HudText::Prompt && text.0 != prompt {
            text.0 = prompt.clone();
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

fn spawn_faded_panel(
    commands: &mut Commands,
    parent: Entity,
    fade: (f32, f32, f32),
    border: Color,
    marker: impl Bundle,
    lines: &[(&str, f32, Color)],
) {
    let bg = ui::PANEL_BG;
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                padding: UiRect::axes(Val::Px(22.0), Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(bg.with_alpha(0.0)),
            BorderColor(border.with_alpha(0.0)),
            BorderRadius::all(Val::Px(6.0)),
            UiFade::new(fade.0, fade.1, fade.2, bg, border),
            marker,
        ))
        .with_children(|panel| {
            for (value, size, color) in lines {
                panel.spawn(ui::text(*value, *size, color.with_alpha(0.0)));
            }
        })
        .set_parent(parent);
}

/// Banner momentaneo de zona (fade 0.4s in, até 2.4s, out até 3.2s).
pub fn spawn_zone_banner(commands: &mut Commands, anchor: Entity, name: &str) {
    let display = short_zone_name(name);
    spawn_faded_panel(
        commands,
        anchor,
        (0.4, 2.4, 3.2),
        ui::PANEL_BORDER,
        ZoneBannerPanel,
        &[(&display, 30.0, ui::TEXT)],
    );
}

/// Placa de aviso de PvP (fade 0.5s in, até 4s, out até 5s).
pub fn spawn_pvp_warning(commands: &mut Commands, anchor: Entity) {
    spawn_faded_panel(
        commands,
        anchor,
        (0.5, 4.0, 5.0),
        ui::DANGER,
        PvpWarningPanel,
        &[
            ("AGUAS DE RISCO", 24.0, ui::DANGER),
            (
                "Seu navio, equipamentos e carga podem ser perdidos.",
                14.0,
                ui::TEXT,
            ),
        ],
    );
}

/// Toast de contexto na pilha da direita (fade 0.3s in, até 2s, out até 2.5s).
pub fn spawn_context_toast(commands: &mut Commands, stack: Entity, message: &str) {
    spawn_faded_panel(
        commands,
        stack,
        (0.3, 2.0, 2.5),
        ui::PANEL_BORDER,
        ContextToast,
        &[(message, 13.0, ui::TEXT)],
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Instant;

    use bevy::ecs::schedule::Schedule;
    use mareforge_domain_world::RiskTier;
    use mareforge_protocol::ItemLine;

    use crate::ship::ShipVisual;
    use crate::zone::ServerZone;

    use super::*;

    #[test]
    fn sail_indicator_shows_level_bar_and_label() {
        use crate::net::SailLevel;
        assert_eq!(sail_indicator_text(SailLevel(0)), "[---]\nVelas recolhidas");
        assert_eq!(sail_indicator_text(SailLevel(3)), "[###]\nPano cheio");
    }

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
            sail_hp: 100.0,
            ammo: Default::default(),
        }
    }

    fn texts(world: &mut World) -> Vec<String> {
        let mut q = world.query::<&Text>();
        q.iter(world).map(|t| t.0.clone()).collect()
    }

    #[test]
    fn cooldown_label_is_human_readable() {
        assert_eq!(cooldown_label(0.0), "PRONTO");
        assert_eq!(cooldown_label(0.4), "1s");
        assert_eq!(cooldown_label(3.6), "4s");
    }

    #[test]
    fn reload_bar_fills_as_cooldown_drops() {
        assert_eq!(reload_fraction(BROADSIDE_RELOAD_SECS), 0.0);
        assert_eq!(reload_fraction(0.0), 1.0);
        assert!((reload_fraction(BROADSIDE_RELOAD_SECS * 0.25) - 0.75).abs() < 1e-5);
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
            short_zone_name("Águas da Ilha do Coral Negro"),
            "Coral Negro"
        );
        assert_eq!(short_zone_name("Rota da Costa"), "Rota da Costa");
    }

    #[test]
    fn port_of_zone_accepts_accented_server_names() {
        assert_eq!(
            port_of_zone("Águas do Porto da Serra"),
            Some("Porto da Serra")
        );
        assert_eq!(
            port_of_zone("Aguas do Porto da Mina"),
            Some("Porto da Mina")
        );
        assert_eq!(port_of_zone("Rota da Costa"), None);
    }

    #[test]
    fn context_prompt_is_empty_when_idle() {
        let prompt = context_prompt(&HudContext::Idle, &KnownCatalog::default(), None);
        assert!(prompt.is_empty());
    }

    #[test]
    fn context_prompt_uses_port_label_when_near_port() {
        let prompt = context_prompt(
            &HudContext::NearPort,
            &KnownCatalog::default(),
            Some("Porto da Serra"),
        );
        assert_eq!(prompt, "[E] Atracar em Porto da Serra");
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
        let prompt = context_prompt(&HudContext::NearNode(id), &catalog, None);
        assert_eq!(prompt, "[G] Coletar Madeira");
    }

    #[test]
    fn update_cooldown_panel_renders_both_broadsides() {
        let mut world = World::new();
        world.insert_resource(MyShip(Some(1)));
        world.spawn(ShipVisual {
            target: ship_state(BROADSIDE_RELOAD_SECS * 0.75, 0.0),
            last_seen: Instant::now(),
        });
        world.spawn((
            Text::default(),
            TextColor::default(),
            HudText::Reload(Broadside::Port),
        ));
        world.spawn((
            Text::default(),
            TextColor::default(),
            HudText::Reload(Broadside::Starboard),
        ));
        let port_fill = world
            .spawn((
                Node::default(),
                BackgroundColor::default(),
                HudFill::Reload(Broadside::Port),
            ))
            .id();

        let mut sched = Schedule::default();
        sched.add_systems(update_cooldown_panel);
        sched.run(&mut world);

        let texts = texts(&mut world);
        assert!(texts.iter().any(|t| t == "3s"), "texts={texts:?}");
        assert!(texts.iter().any(|t| t == "PRONTO"), "texts={texts:?}");
        assert_eq!(
            world.get::<Node>(port_fill).unwrap().width,
            Val::Percent(25.0)
        );
    }

    #[test]
    fn update_ship_panel_fills_hp_and_cargo() {
        let mut world = World::new();
        world.insert_resource(MyShip(Some(1)));
        world.insert_resource(Wallet(1000));
        world.spawn(ShipVisual {
            target: ship_state(0.0, 0.0),
            last_seen: Instant::now(),
        });
        for kind in [HudText::Hp, HudText::Cargo, HudText::Gold] {
            world.spawn((Text::default(), kind));
        }
        let hp_fill = world
            .spawn((Node::default(), BackgroundColor::default(), HudFill::Hp))
            .id();

        let mut sched = Schedule::default();
        sched.add_systems(update_ship_panel);
        sched.run(&mut world);

        let texts = texts(&mut world);
        for expected in ["120/150", "8/100", "1000g"] {
            assert!(texts.iter().any(|t| t == expected), "texts={texts:?}");
        }
        assert_eq!(
            world.get::<Node>(hp_fill).unwrap().width,
            Val::Percent(80.0)
        );
    }

    #[test]
    fn update_zone_panel_reflects_current_zone() {
        let mut world = World::new();
        world.insert_resource(CurrentZone(Some(ServerZone {
            tier: RiskTier::Frontier,
            name: String::from("Rota da Costa"),
        })));
        for kind in [HudText::ZoneName, HudText::ZoneTag, HudText::ZoneRisk] {
            world.spawn((Text::default(), TextColor::default(), kind));
        }

        let mut sched = Schedule::default();
        sched.add_systems(update_zone_panel);
        sched.run(&mut world);

        let texts = texts(&mut world);
        assert!(
            texts.iter().any(|t| t == "Rota da Costa"),
            "texts={texts:?}"
        );
        assert!(texts.iter().any(|t| t == "FRONTEIRA"), "texts={texts:?}");
        assert!(
            texts.iter().any(|t| t == risk_tag(RiskTier::Frontier)),
            "texts={texts:?}"
        );
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

    #[test]
    fn hud_does_not_attach_to_camera() {
        let mut world = World::new();
        let camera = world.spawn(Camera2d).id();
        let mut sched = Schedule::default();
        sched.add_systems(setup_hud);
        sched.run(&mut world);

        assert!(world.get::<Children>(camera).is_none());
        let mut q = world.query_filtered::<Entity, With<SailIndicator>>();
        assert_eq!(q.iter(&world).count(), 1);
    }
}
