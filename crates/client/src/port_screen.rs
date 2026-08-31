//! Tela de porto (MF-042): overlay quando atracado, com abas de storage,
//! loadout, crafting, shipyard e mercado. Reusa os readouts existentes do
//! mercado; as demais abas são Text2d + teclado, sem animação nem snapshot
//! de storage (gap documentado na aba Loadout).

use bevy::ecs::prelude::*;
use bevy::ecs::query::Or;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use mareforge_domain_crafting::recipe::StationKind;
use mareforge_domain_items::{
    EquipmentDefinition, EquipmentSlot, EquipmentStats, ItemDefinition, ItemKind,
};
use mareforge_domain_ships::{can_equip, ShipDefinition, ShipKind, SlotSpec};
use mareforge_protocol::{
    CraftItem, CraftResult, DockResult, EquipItem, ItemLine, LoadoutLine, LoadoutResult,
    LoadoutSnapshot, MarketResult, PortStorageSnapshot, RecipeEntry, StorageDepositAll,
    StorageLine, StorageWithdrawAll, Undock, UnequipItem,
};
use mareforge_shared::ids::{ItemDefinitionId, ShipDefinitionId};

use crate::assets::layers;
use crate::crafting::KnownRecipes;
use crate::market::{KnownCatalog, MarketFeedback, MarketFormReadout, MarketReadout};
use crate::net::{KnownShipKind, MyDocked, MyShip, ReliableChannel};
use crate::ship::ShipVisual;

/// Nome do porto atracado mais recente, extraído do `DockResult.reason`.
#[derive(Resource, Debug, Default)]
pub struct DockedPortName(pub String);

/// Último snapshot de loadout do servidor.
#[derive(Resource, Debug, Default)]
pub struct KnownLoadout(pub Vec<LoadoutLine>);

/// Último snapshot de storage do porto onde o jogador atracou.
#[derive(Resource, Debug, Default)]
pub struct KnownPortStorage(pub Vec<StorageLine>);

/// Último veredito de loadout para a aba correspondente.
#[derive(Resource, Debug, Default)]
pub struct LoadoutFeedback(pub Option<LoadoutResult>);

/// Último veredito de craft para as abas Crafting/Shipyard.
#[derive(Resource, Debug, Default)]
pub struct CraftFeedback(pub Option<CraftResult>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortTab {
    Storage,
    Loadout,
    Crafting,
    Shipyard,
    Market,
}

impl PortTab {
    pub const ALL: [PortTab; 5] = [
        PortTab::Storage,
        PortTab::Loadout,
        PortTab::Crafting,
        PortTab::Shipyard,
        PortTab::Market,
    ];

    pub fn next(self) -> Self {
        match self {
            PortTab::Storage => PortTab::Loadout,
            PortTab::Loadout => PortTab::Crafting,
            PortTab::Crafting => PortTab::Shipyard,
            PortTab::Shipyard => PortTab::Market,
            PortTab::Market => PortTab::Storage,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            PortTab::Storage => PortTab::Market,
            PortTab::Loadout => PortTab::Storage,
            PortTab::Crafting => PortTab::Loadout,
            PortTab::Shipyard => PortTab::Crafting,
            PortTab::Market => PortTab::Shipyard,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PortTab::Storage => "Porão",
            PortTab::Loadout => "Equipamento",
            PortTab::Crafting => "Fabricação",
            PortTab::Shipyard => "Estaleiro",
            PortTab::Market => "Mercado",
        }
    }
}

/// Estado local da tela: aba ativa e ação selecionada.
#[derive(Resource, Debug)]
pub struct PortScreenState {
    pub active_tab: PortTab,
    pub selected_action: usize,
}

impl Default for PortScreenState {
    fn default() -> Self {
        Self {
            active_tab: PortTab::Storage,
            selected_action: 0,
        }
    }
}

/// Raiz da tela de porto; existe apenas enquanto atracado.
#[derive(Component)]
pub struct PortScreen;

type MarketPanelQuery<'w, 's> =
    Query<'w, 's, Entity, Or<(With<MarketReadout>, With<MarketFormReadout>)>>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PortAction {
    DepositAll,
    WithdrawAll,
    Unequip(EquipmentSlot),
    Equip(ItemDefinitionId, EquipmentSlot, String),
    Craft(u32),
    Undock,
}

pub struct PortPlugin;

impl Plugin for PortPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DockedPortName>()
            .init_resource::<KnownLoadout>()
            .init_resource::<KnownPortStorage>()
            .init_resource::<PortScreenState>()
            .init_resource::<LoadoutFeedback>()
            .init_resource::<CraftFeedback>()
            .add_systems(
                Update,
                (
                    handle_loadout_snapshot,
                    handle_port_storage_snapshot,
                    handle_loadout_result,
                    handle_craft_result,
                    handle_dock_result,
                    toggle_port_screen,
                    toggle_market_panel,
                    handle_port_input,
                    update_port_screen,
                ),
            );
    }
}

fn handle_loadout_snapshot(
    mut events: EventReader<ClientReceiveMessage<LoadoutSnapshot>>,
    mut known: ResMut<KnownLoadout>,
) {
    for event in events.read() {
        known.0 = event.message().slots.clone();
    }
}

fn handle_port_storage_snapshot(
    mut events: EventReader<ClientReceiveMessage<PortStorageSnapshot>>,
    mut known: ResMut<KnownPortStorage>,
) {
    for event in events.read() {
        known.0 = event.message().lines.clone();
    }
}

fn ship_definition(kind: Option<ShipKind>, loadout: &[LoadoutLine]) -> Option<ShipDefinition> {
    let kind = kind?;
    Some(ShipDefinition {
        id: ShipDefinitionId::new(),
        kind,
        display_name: String::new(),
        slots: loadout
            .iter()
            .map(|line| SlotSpec {
                kind: line.slot,
                accepts_tag: None,
            })
            .collect(),
        cargo_capacity: 0,
        base_speed: 0.0,
        base_turn_rate: 0.0,
        base_hp: 0,
        base_weapon_damage: 0,
        base_weapon_range: 0.0,
    })
}

fn catalog_line(catalog: &KnownCatalog, item: ItemDefinitionId) -> Option<&ItemLine> {
    catalog.0.values().find(|line| line.id == item)
}

fn item_definition(line: &ItemLine) -> Option<ItemDefinition> {
    let slot = line.equipment_slot?;
    Some(ItemDefinition {
        id: line.id,
        kind: ItemKind::Equipment,
        equipment: Some(EquipmentDefinition {
            slot,
            stats: EquipmentStats::default(),
        }),
        max_stack: 1,
        base_weight: line.weight,
        tags: Default::default(),
        display_name: line.name.clone(),
    })
}

fn compatible_equip(
    storage: &[StorageLine],
    catalog: &KnownCatalog,
    loadout: &[LoadoutLine],
    kind: Option<ShipKind>,
) -> Vec<(EquipmentSlot, StorageLine)> {
    let Some(ship) = ship_definition(kind, loadout) else {
        return Vec::new();
    };
    storage
        .iter()
        .filter_map(|storage_line| {
            let item_line = catalog_line(catalog, storage_line.item)?;
            let item = item_definition(item_line)?;
            let slot = can_equip(&ship, &item).ok()?;
            Some((slot, storage_line.clone()))
        })
        .collect()
}

fn handle_loadout_result(
    mut events: EventReader<ClientReceiveMessage<LoadoutResult>>,
    mut feedback: ResMut<LoadoutFeedback>,
) {
    for event in events.read() {
        feedback.0 = Some(event.message().clone());
    }
}

fn handle_craft_result(
    mut events: EventReader<ClientReceiveMessage<CraftResult>>,
    mut feedback: ResMut<CraftFeedback>,
) {
    for event in events.read() {
        feedback.0 = Some(*event.message());
    }
}

fn port_name_from_reason(reason: &str) -> String {
    reason
        .strip_prefix("atracado em ")
        .unwrap_or(reason)
        .to_owned()
}

fn handle_dock_result(
    mut events: EventReader<ClientReceiveMessage<DockResult>>,
    mut port_name: ResMut<DockedPortName>,
    mut commands: Commands,
    camera: Query<Entity, With<Camera2d>>,
    screens: Query<Entity, With<PortScreen>>,
) {
    for event in events.read() {
        let result = event.message();
        if result.success && result.docked {
            port_name.0 = port_name_from_reason(&result.reason);
        }
        if result.docked {
            if screens.iter().next().is_none() {
                spawn_port_screen(&mut commands, &camera);
            }
        } else {
            for entity in &screens {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn spawn_port_screen(commands: &mut Commands, camera: &Query<Entity, With<Camera2d>>) {
    let Ok(camera) = camera.get_single() else {
        return;
    };
    commands
        .spawn((
            PortScreen,
            Text2d::new(String::new()),
            TextFont {
                font_size: 12.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.9, 0.85)),
            Anchor::TopLeft,
            Transform::from_xyz(-560.0, 320.0, layers::OVERLAY),
            Visibility::Visible,
        ))
        .set_parent(camera);
}

fn toggle_port_screen(
    docked: Res<MyDocked>,
    mut screens: Query<&mut Visibility, With<PortScreen>>,
) {
    let visibility = if docked.0 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut entity in &mut screens {
        *entity = visibility;
    }
}

fn toggle_market_panel(
    docked: Res<MyDocked>,
    state: Res<PortScreenState>,
    mut commands: Commands,
    panels: MarketPanelQuery,
) {
    let visibility = if docked.0 && state.active_tab == PortTab::Market {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for entity in &panels {
        commands.entity(entity).insert(visibility);
    }
}

fn port_actions(
    tab: PortTab,
    loadout: &[LoadoutLine],
    recipes: &[RecipeEntry],
    storage: &[StorageLine],
    catalog: &KnownCatalog,
    ship_kind: Option<ShipKind>,
) -> Vec<PortAction> {
    let mut actions = match tab {
        PortTab::Storage => vec![PortAction::DepositAll, PortAction::WithdrawAll],
        PortTab::Loadout => {
            let mut actions: Vec<PortAction> = loadout
                .iter()
                .filter(|line| line.equipped)
                .map(|line| PortAction::Unequip(line.slot))
                .collect();
            actions.extend(
                compatible_equip(storage, catalog, loadout, ship_kind)
                    .into_iter()
                    .map(|(slot, line)| PortAction::Equip(line.item, slot, line.item_name.clone())),
            );
            actions
        }
        PortTab::Crafting => recipes_for_station(recipes, false)
            .into_iter()
            .map(|entry| PortAction::Craft(entry.recipe_id))
            .collect(),
        PortTab::Shipyard => recipes_for_station(recipes, true)
            .into_iter()
            .map(|entry| PortAction::Craft(entry.recipe_id))
            .collect(),
        PortTab::Market => Vec::new(),
    };
    actions.push(PortAction::Undock);
    actions
}

fn recipes_for_station(recipes: &[RecipeEntry], dock: bool) -> Vec<&RecipeEntry> {
    recipes
        .iter()
        .filter(|entry| (entry.station == StationKind::Dock) == dock)
        .collect()
}

fn send_port_action(connection_manager: &mut ConnectionManager, action: &PortAction) {
    let _ = match action {
        PortAction::DepositAll => {
            connection_manager.send_message::<ReliableChannel, _>(&StorageDepositAll)
        }
        PortAction::WithdrawAll => {
            connection_manager.send_message::<ReliableChannel, _>(&StorageWithdrawAll)
        }
        PortAction::Unequip(slot) => {
            connection_manager.send_message::<ReliableChannel, _>(&UnequipItem { slot: *slot })
        }
        PortAction::Equip(_, _, _) => {
            connection_manager.send_message::<ReliableChannel, _>(&equip_item_for(action).unwrap())
        }
        PortAction::Craft(recipe_id) => {
            connection_manager.send_message::<ReliableChannel, _>(&CraftItem {
                recipe_id: *recipe_id,
            })
        }
        PortAction::Undock => connection_manager.send_message::<ReliableChannel, _>(&Undock),
    };
}

fn equip_item_for(action: &PortAction) -> Option<EquipItem> {
    match action {
        PortAction::Equip(item, _, _) => Some(EquipItem { item: *item }),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_port_input(
    keys: Res<ButtonInput<KeyCode>>,
    docked: Res<MyDocked>,
    mut state: ResMut<PortScreenState>,
    loadout: Res<KnownLoadout>,
    storage: Res<KnownPortStorage>,
    catalog: Res<KnownCatalog>,
    ship_kind: Res<KnownShipKind>,
    recipes: Res<KnownRecipes>,
    screens: Query<Entity, With<PortScreen>>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    if !docked.0 || screens.is_empty() {
        return;
    }

    if keys.just_pressed(KeyCode::Escape) {
        let _ = connection_manager.send_message::<ReliableChannel, _>(&Undock);
        return;
    }

    if keys.just_pressed(KeyCode::Tab) {
        let backward = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        state.active_tab = if backward {
            state.active_tab.previous()
        } else {
            state.active_tab.next()
        };
        state.selected_action = 0;
        return;
    }

    if state.active_tab == PortTab::Market {
        return;
    }

    let actions = port_actions(
        state.active_tab,
        &loadout.0,
        &recipes.0,
        &storage.0,
        &catalog,
        ship_kind.0,
    );
    if keys.just_pressed(KeyCode::ArrowUp) {
        state.selected_action = state.selected_action.saturating_sub(1);
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        state.selected_action = state
            .selected_action
            .saturating_add(1)
            .min(actions.len().saturating_sub(1));
    }
    if keys.just_pressed(KeyCode::Enter) {
        if let Some(action) = actions.get(state.selected_action) {
            send_port_action(&mut connection_manager, action);
        }
    }
}

fn action_label(action: &PortAction, recipes: &[RecipeEntry]) -> String {
    match action {
        PortAction::DepositAll => String::from("Depositar tudo"),
        PortAction::WithdrawAll => String::from("Retirar tudo"),
        PortAction::Unequip(slot) => format!("Desequipar {}", slot_label(*slot)),
        PortAction::Equip(_, slot, item_name) => {
            format!("{}: {item_name} [Equipar]", slot_label(*slot))
        }
        PortAction::Craft(recipe_id) => {
            let entry = recipes.iter().find(|entry| entry.recipe_id == *recipe_id);
            let verb = if entry.is_some_and(|entry| entry.station == StationKind::Dock) {
                "Construir"
            } else {
                "Fabricar"
            };
            let name = entry
                .map(|entry| entry.display_name.as_str())
                .unwrap_or("receita");
            format!("{verb} {name}")
        }
        PortAction::Undock => String::from("Desatracar"),
    }
}

fn slot_label(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Hull => "Hull",
        EquipmentSlot::Sail => "Sail",
        EquipmentSlot::Weapon => "Weapon",
        EquipmentSlot::Aux => "Aux",
    }
}

fn station_label(station: StationKind) -> &'static str {
    match station {
        StationKind::None => "Qualquer estação",
        StationKind::Workbench => "Bancada",
        StationKind::Anvil => "Bigorna",
        StationKind::Dock => "Doca",
    }
}

fn tab_bar(active: PortTab) -> String {
    let tabs: Vec<String> = PortTab::ALL
        .iter()
        .map(|tab| {
            if *tab == active {
                format!("[{}]", tab.label())
            } else {
                tab.label().to_owned()
            }
        })
        .collect();
    tabs.join(" | ")
}

fn clamped_selection(actions: &[PortAction], selected: usize) -> usize {
    selected.min(actions.len().saturating_sub(1))
}

fn action_lines(actions: &[PortAction], selected: usize, recipes: &[RecipeEntry]) -> Vec<String> {
    let selected = clamped_selection(actions, selected);
    actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let marker = if index == selected { ">" } else { " " };
            format!("{marker} [{}]", action_label(action, recipes))
        })
        .collect()
}

fn feedback_line(success: bool, reason: &str) -> String {
    format!("{}: {reason}", if success { "OK" } else { "ERRO" })
}

fn storage_lines(
    cargo_weight: Option<u32>,
    feedback: Option<&MarketResult>,
    state: &PortScreenState,
    recipes: &[RecipeEntry],
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Porão: {}",
            cargo_weight.map_or_else(|| String::from("—"), |weight| weight.to_string())
        ),
        String::from("Storage: conteúdo oculto — use Depositar/Retirar tudo"),
    ];
    if let Some(result) = feedback {
        lines.push(feedback_line(result.success, &result.reason));
    }
    let actions = port_actions(
        PortTab::Storage,
        &[],
        recipes,
        &[],
        &KnownCatalog::default(),
        None,
    );
    lines.extend(action_lines(&actions, state.selected_action, recipes));
    lines
}

fn loadout_lines(
    loadout: &[LoadoutLine],
    storage: &[StorageLine],
    catalog: &KnownCatalog,
    ship_kind: Option<ShipKind>,
    feedback: Option<&LoadoutResult>,
    state: &PortScreenState,
    recipes: &[RecipeEntry],
) -> Vec<String> {
    let mut lines = loadout
        .iter()
        .map(|line| {
            let name = if line.item_name.is_empty() {
                String::from("(vazio)")
            } else {
                line.item_name.clone()
            };
            format!("{}: {name}", slot_label(line.slot))
        })
        .collect::<Vec<_>>();
    if compatible_equip(storage, catalog, loadout, ship_kind).is_empty() {
        lines.push(String::from("Storage: nada compatível com este casco"));
    }
    if let Some(result) = feedback {
        lines.push(feedback_line(result.success, &result.reason));
    }
    let actions = port_actions(
        PortTab::Loadout,
        loadout,
        recipes,
        storage,
        catalog,
        ship_kind,
    );
    lines.extend(action_lines(&actions, state.selected_action, recipes));
    lines
}

fn recipe_lines(
    recipes: &[RecipeEntry],
    dock: bool,
    feedback: Option<&CraftResult>,
    state: &PortScreenState,
) -> Vec<String> {
    let mut lines = Vec::new();
    if dock {
        lines.push(String::from(
            "Receitas de casco — custos saem do storage do porto.",
        ));
    }
    for entry in recipes_for_station(recipes, dock) {
        let ingredients = entry
            .ingredients
            .iter()
            .map(|ingredient| format!("{}x {}", ingredient.quantity, ingredient.name))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "{} — Estação: {}",
            entry.display_name,
            station_label(entry.station)
        ));
        lines.push(format!(
            "  Insumos: {}",
            if ingredients.is_empty() {
                String::from("—")
            } else {
                ingredients
            }
        ));
        lines.push(format!(
            "  Saída: {} x{}",
            entry.output_name, entry.output_quantity
        ));
    }
    if let Some(result) = feedback {
        let name = recipes
            .iter()
            .find(|entry| entry.recipe_id == result.recipe_id)
            .map(|entry| entry.display_name.as_str())
            .unwrap_or("receita");
        lines.push(format!(
            "{}: {name}",
            if result.success { "OK" } else { "ERRO" }
        ));
    }
    let tab = if dock {
        PortTab::Shipyard
    } else {
        PortTab::Crafting
    };
    let actions = port_actions(tab, &[], recipes, &[], &KnownCatalog::default(), None);
    lines.extend(action_lines(&actions, state.selected_action, recipes));
    lines
}

#[allow(clippy::too_many_arguments)]
fn content_lines(
    state: &PortScreenState,
    cargo_weight: Option<u32>,
    loadout: &[LoadoutLine],
    storage: &[StorageLine],
    catalog: &KnownCatalog,
    ship_kind: Option<ShipKind>,
    recipes: &[RecipeEntry],
    loadout_feedback: Option<&LoadoutResult>,
    craft_feedback: Option<&CraftResult>,
    market_feedback: Option<&MarketResult>,
) -> Vec<String> {
    match state.active_tab {
        PortTab::Storage => storage_lines(cargo_weight, market_feedback, state, recipes),
        PortTab::Loadout => loadout_lines(
            loadout,
            storage,
            catalog,
            ship_kind,
            loadout_feedback,
            state,
            recipes,
        ),
        PortTab::Crafting => recipe_lines(recipes, false, craft_feedback, state),
        PortTab::Shipyard => recipe_lines(recipes, true, craft_feedback, state),
        PortTab::Market => vec![String::from("Mercado regional")],
    }
}

#[allow(clippy::too_many_arguments)]
fn port_screen_text(
    port_name: &str,
    state: &PortScreenState,
    cargo_weight: Option<u32>,
    loadout: &[LoadoutLine],
    storage: &[StorageLine],
    catalog: &KnownCatalog,
    ship_kind: Option<ShipKind>,
    recipes: &[RecipeEntry],
    loadout_feedback: Option<&LoadoutResult>,
    craft_feedback: Option<&CraftResult>,
    market_feedback: Option<&MarketResult>,
) -> String {
    let header = if port_name.is_empty() {
        String::from("Porto: ?")
    } else {
        format!("Porto: {port_name}")
    };
    let mut lines = vec![header, tab_bar(state.active_tab), String::new()];
    lines.extend(content_lines(
        state,
        cargo_weight,
        loadout,
        storage,
        catalog,
        ship_kind,
        recipes,
        loadout_feedback,
        craft_feedback,
        market_feedback,
    ));
    lines.join("\n")
}

#[allow(clippy::too_many_arguments)]
fn update_port_screen(
    state: Res<PortScreenState>,
    port_name: Res<DockedPortName>,
    my_ship: Res<MyShip>,
    visuals: Query<&ShipVisual>,
    loadout: Res<KnownLoadout>,
    storage: Res<KnownPortStorage>,
    catalog: Res<KnownCatalog>,
    ship_kind: Res<KnownShipKind>,
    recipes: Res<KnownRecipes>,
    loadout_feedback: Res<LoadoutFeedback>,
    craft_feedback: Res<CraftFeedback>,
    market_feedback: Res<MarketFeedback>,
    mut screens: Query<&mut Text2d, With<PortScreen>>,
) {
    let cargo_weight = my_ship.0.and_then(|ship_id| {
        visuals
            .iter()
            .find(|visual| visual.target.ship_id == ship_id)
            .map(|visual| visual.target.cargo_weight)
    });
    let text = port_screen_text(
        &port_name.0,
        &state,
        cargo_weight,
        &loadout.0,
        &storage.0,
        &catalog,
        ship_kind.0,
        &recipes.0,
        loadout_feedback.0.as_ref(),
        craft_feedback.0.as_ref(),
        market_feedback.0.as_ref(),
    );
    for mut screen in &mut screens {
        screen.0 = text.clone();
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use mareforge_protocol::IngredientLine;

    use super::*;

    fn recipe(recipe_id: u32, station: StationKind) -> RecipeEntry {
        RecipeEntry {
            recipe_id,
            display_name: format!("Receita {recipe_id}"),
            station,
            ship_build: station == StationKind::Dock,
            output_name: format!("Saída {recipe_id}"),
            output_quantity: 1,
            ingredients: vec![IngredientLine {
                name: String::from("Madeira"),
                quantity: 5,
            }],
        }
    }

    fn loadout() -> KnownLoadout {
        KnownLoadout(vec![
            LoadoutLine {
                slot: EquipmentSlot::Hull,
                item_name: String::from("Casco Reforçado"),
                equipped: true,
            },
            LoadoutLine {
                slot: EquipmentSlot::Sail,
                item_name: String::new(),
                equipped: false,
            },
            LoadoutLine {
                slot: EquipmentSlot::Weapon,
                item_name: String::from("Canhão de Bronze"),
                equipped: true,
            },
            LoadoutLine {
                slot: EquipmentSlot::Aux,
                item_name: String::new(),
                equipped: false,
            },
        ])
    }

    fn storage_line(id: ItemDefinitionId, item_name: &str, quantity: u32) -> StorageLine {
        StorageLine {
            item: id,
            item_name: String::from(item_name),
            quantity,
        }
    }

    fn item_line(id: ItemDefinitionId, item_name: &str, slot: Option<EquipmentSlot>) -> ItemLine {
        ItemLine {
            id,
            name: String::from(item_name),
            weight: 5,
            equipment_slot: slot,
        }
    }

    #[test]
    fn port_screen_visibility_follows_docked_state() {
        let mut world = World::new();
        world.insert_resource(MyDocked(true));
        let entity = world.spawn((PortScreen, Visibility::Hidden)).id();

        world.run_system_once(toggle_port_screen).unwrap();
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Visible
        );

        world.insert_resource(MyDocked(false));
        world.run_system_once(toggle_port_screen).unwrap();
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn tab_cycles_forward_and_backward() {
        let mut tab = PortTab::Storage;
        for expected in [
            PortTab::Loadout,
            PortTab::Crafting,
            PortTab::Shipyard,
            PortTab::Market,
            PortTab::Storage,
        ] {
            tab = tab.next();
            assert_eq!(tab, expected);
        }

        let mut tab = PortTab::Storage;
        for expected in [
            PortTab::Market,
            PortTab::Shipyard,
            PortTab::Crafting,
            PortTab::Loadout,
            PortTab::Storage,
        ] {
            tab = tab.previous();
            assert_eq!(tab, expected);
        }
    }

    #[test]
    fn storage_actions_send_deposit_and_withdraw() {
        let actions = port_actions(
            PortTab::Storage,
            &[],
            &[],
            &[],
            &KnownCatalog::default(),
            None,
        );

        assert_eq!(actions[0], PortAction::DepositAll);
        assert_eq!(actions[1], PortAction::WithdrawAll);
    }

    #[test]
    fn loadout_lists_slots_and_only_unequips_equipped_slots() {
        let loadout = loadout();
        let text = port_screen_text(
            "Porto da Serra",
            &PortScreenState {
                active_tab: PortTab::Loadout,
                selected_action: 0,
            },
            Some(8),
            &loadout.0,
            &[],
            &KnownCatalog::default(),
            Some(ShipKind::SmallMerchant),
            &[],
            None,
            None,
            None,
        );
        for expected in [
            "Hull: Casco Reforçado",
            "Sail: (vazio)",
            "Weapon: Canhão de Bronze",
            "Aux: (vazio)",
            "Storage: nada compatível com este casco",
        ] {
            assert!(text.contains(expected), "{text}");
        }
        assert!(!text.contains("use T/Y/U (debug)"), "{text}");

        let actions = port_actions(
            PortTab::Loadout,
            &loadout.0,
            &[],
            &[],
            &KnownCatalog::default(),
            Some(ShipKind::SmallMerchant),
        );
        assert_eq!(actions[0], PortAction::Unequip(EquipmentSlot::Hull));
        assert_eq!(actions[1], PortAction::Unequip(EquipmentSlot::Weapon));
        assert!(!actions
            .iter()
            .any(|action| matches!(action, PortAction::Craft(_))));
    }

    #[test]
    fn crafting_tab_lists_non_dock_recipes() {
        let recipes = vec![
            recipe(1, StationKind::Workbench),
            recipe(2, StationKind::Dock),
            recipe(3, StationKind::None),
            recipe(4, StationKind::Anvil),
        ];
        let text = port_screen_text(
            "Porto da Serra",
            &PortScreenState {
                active_tab: PortTab::Crafting,
                selected_action: 0,
            },
            None,
            &[],
            &[],
            &KnownCatalog::default(),
            None,
            &recipes,
            None,
            None,
            None,
        );
        assert!(text.contains("Receita 1"), "{text}");
        assert!(text.contains("Receita 3"), "{text}");
        assert!(text.contains("Receita 4"), "{text}");
        assert!(!text.contains("Receita 2"), "{text}");

        let entries = recipes_for_station(&recipes, false);
        assert_eq!(entries.len(), 3);
        let actions = port_actions(
            PortTab::Crafting,
            &[],
            &recipes,
            &[],
            &KnownCatalog::default(),
            None,
        );
        assert_eq!(actions[0], PortAction::Craft(1));
        assert_eq!(actions[1], PortAction::Craft(3));
    }

    #[test]
    fn shipyard_tab_lists_dock_recipes_only() {
        let recipes = vec![
            recipe(1, StationKind::Workbench),
            recipe(2, StationKind::Dock),
        ];
        let text = port_screen_text(
            "Porto da Serra",
            &PortScreenState {
                active_tab: PortTab::Shipyard,
                selected_action: 0,
            },
            None,
            &[],
            &[],
            &KnownCatalog::default(),
            None,
            &recipes,
            None,
            None,
            None,
        );
        assert!(text.contains("Receita 2"), "{text}");
        assert!(!text.contains("Receita 1"), "{text}");
        assert!(
            text.contains("Receitas de casco — custos saem do storage do porto."),
            "{text}"
        );

        let entries = recipes_for_station(&recipes, true);
        assert_eq!(entries.len(), 1);
        let actions = port_actions(
            PortTab::Shipyard,
            &[],
            &recipes,
            &[],
            &KnownCatalog::default(),
            None,
        );
        assert_eq!(actions[0], PortAction::Craft(2));
    }

    #[test]
    fn craft_action_sends_craft_item_for_recipe_id() {
        let recipes = vec![recipe(7, StationKind::Workbench)];
        let actions = port_actions(
            PortTab::Crafting,
            &[],
            &recipes,
            &[],
            &KnownCatalog::default(),
            None,
        );

        assert_eq!(actions[0], PortAction::Craft(7));
    }

    #[test]
    fn market_tab_uses_existing_market_panel() {
        let mut world = World::new();
        world.insert_resource(MyDocked(true));
        world.insert_resource(PortScreenState {
            active_tab: PortTab::Market,
            selected_action: 0,
        });
        let entity = world.spawn((MarketReadout, Visibility::Hidden)).id();

        world.run_system_once(toggle_market_panel).unwrap();
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Visible
        );

        world.insert_resource(PortScreenState::default());
        world.run_system_once(toggle_market_panel).unwrap();
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn undock_action_sends_undock() {
        let actions = port_actions(
            PortTab::Market,
            &[],
            &[],
            &[],
            &KnownCatalog::default(),
            None,
        );

        assert_eq!(actions[0], PortAction::Undock);
    }

    #[test]
    fn loadout_renders_equip_button_for_matching_storage_items() {
        let hull = ItemDefinitionId::new();
        let storage = vec![storage_line(hull, "Casco Reforçado", 1)];
        let catalog = KnownCatalog(std::collections::HashMap::from([(
            String::from("Casco Reforçado"),
            item_line(hull, "Casco Reforçado", Some(EquipmentSlot::Hull)),
        )]));
        let text = port_screen_text(
            "Porto da Serra",
            &PortScreenState {
                active_tab: PortTab::Loadout,
                selected_action: 0,
            },
            Some(8),
            &loadout().0,
            &storage,
            &catalog,
            Some(ShipKind::SmallMerchant),
            &[],
            None,
            None,
            None,
        );

        assert!(text.contains("Hull: Casco Reforçado [Equipar]"), "{text}");
        assert!(!text.contains("use T/Y/U (debug)"), "{text}");
    }

    #[test]
    fn equip_button_builds_equip_item_intent() {
        let hull = ItemDefinitionId::new();
        let storage = vec![storage_line(hull, "Casco Reforçado", 1)];
        let catalog = KnownCatalog(std::collections::HashMap::from([(
            String::from("Casco Reforçado"),
            item_line(hull, "Casco Reforçado", Some(EquipmentSlot::Hull)),
        )]));
        let actions = port_actions(
            PortTab::Loadout,
            &loadout().0,
            &[],
            &storage,
            &catalog,
            Some(ShipKind::SmallMerchant),
        );

        let equip = actions
            .iter()
            .find_map(|action| match action {
                PortAction::Equip(item, slot, name) => Some((*item, *slot, name.as_str())),
                _ => None,
            })
            .expect("storage compatível gera ação Equipar");
        assert_eq!(equip.0, hull);
        assert_eq!(equip.1, EquipmentSlot::Hull);
        assert_eq!(equip.2, "Casco Reforçado");
        assert_eq!(
            equip_item_for(&PortAction::Equip(
                hull,
                EquipmentSlot::Hull,
                String::from("Casco Reforçado")
            )),
            Some(EquipItem { item: hull })
        );
    }

    #[test]
    fn items_missing_from_port_storage_do_not_spawn_equip_ui() {
        let actions = port_actions(
            PortTab::Loadout,
            &loadout().0,
            &[],
            &[],
            &KnownCatalog::default(),
            Some(ShipKind::SmallMerchant),
        );

        assert!(!actions
            .iter()
            .any(|action| matches!(action, PortAction::Equip(_, _, _))));
    }
}
