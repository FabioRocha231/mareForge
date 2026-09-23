//! Mercado no client (PRD MF-023..026). O client vê catálogo, carteira e o
//! quadro de orders; storage e execução são do servidor (Pilar 4). O painel
//! (MF-040, MF-058) vive na aba Mercado da tela de porto, em bevy_ui: lista
//! orders, cria/cancela/compra por teclado ou mouse, e o veredito
//! `MarketResult` aparece na linha de status da tela de porto. Z/X/V/N/B
//! continuam como atalhos dev (escondidos do HUD desde MF-043).

use std::collections::HashMap;

use crate::net::{MyDocked, ReliableChannel};
use crate::port_screen::{PortScreenState, PortTab};
use crate::ui;
use bevy::ecs::prelude::*;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use marvyr_protocol::{
    BuySellOrder, CancelSellOrder, CatalogSnapshot, CreateSellOrder, ItemLine, MarketResult,
    OrderLine, OrdersSnapshot, StorageDepositAll, StorageWithdrawAll, WalletUpdated,
};

/// Catálogo do servidor: nome → id real (para os intents) + peso (UI).
#[derive(Resource, Debug, Default)]
pub struct KnownCatalog(pub HashMap<String, ItemLine>);

/// Carteira global do personagem (§31: ouro não afunda com o navio).
#[derive(Resource, Debug, Default)]
pub struct Wallet(pub u64);

/// Quadro de orders conhecido (último snapshot do servidor).
#[derive(Resource, Debug, Default)]
pub struct KnownOrders(pub Vec<marvyr_protocol::OrderLine>);

/// Estado local do formulário de venda e da seleção de orders.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct MarketForm {
    pub item_index: usize,
    pub quantity: String,
    pub unit_price: String,
    pub selected_order: usize,
    pub focus: FormFocus,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FormFocus {
    #[default]
    Orders,
    Item,
    Quantity,
    Price,
}

/// Último veredito do servidor, exibido no painel.
#[derive(Resource, Debug, Default)]
pub struct MarketFeedback(pub Option<MarketResult>);

/// Botões do painel de mercado (mouse); mesmo efeito das teclas.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketButton {
    SelectOrder(usize),
    ExecuteOrder(usize),
    Focus(FormFocus),
    Item(i8),
    Step(FormFocus, i8),
    Submit,
}

pub struct MarketPlugin;

impl Plugin for MarketPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KnownCatalog>()
            .init_resource::<Wallet>()
            .init_resource::<KnownOrders>()
            .init_resource::<MarketForm>()
            .init_resource::<MarketFeedback>()
            .add_systems(
                Update,
                (
                    handle_catalog_snapshot,
                    handle_wallet_updated,
                    handle_orders_snapshot,
                    handle_market_result,
                    handle_market_panel_input,
                    handle_market_clicks,
                ),
            );
    }
}

/// Preços dev de venda por item (§39: pricing é tuning; mercado real é de
/// jogadores — aqui é só para o smoke ter número plausível).
fn dev_price(name: &str) -> u64 {
    match name {
        "Madeira" => 5,
        "Minério" => 8,
        "Coral Negro" => 40,
        _ => 10,
    }
}

fn handle_catalog_snapshot(
    mut catalog_events: EventReader<ClientReceiveMessage<CatalogSnapshot>>,
    mut known: ResMut<KnownCatalog>,
    mut form: ResMut<MarketForm>,
) {
    for event in catalog_events.read() {
        known.0 = event
            .message()
            .items
            .iter()
            .map(|line| (line.name.clone(), line.clone()))
            .collect();
        form.item_index = form.item_index.min(known.0.len().saturating_sub(1));
        info!(items = known.0.len(), "catálogo de itens recebido");
    }
}

fn handle_wallet_updated(
    mut wallet_events: EventReader<ClientReceiveMessage<WalletUpdated>>,
    mut wallet: ResMut<Wallet>,
) {
    for event in wallet_events.read() {
        wallet.0 = event.message().gold;
        info!(gold = wallet.0, "carteira atualizada");
    }
}

fn handle_market_result(
    mut events: EventReader<ClientReceiveMessage<MarketResult>>,
    mut feedback: ResMut<MarketFeedback>,
) {
    for event in events.read() {
        let result = event.message();
        feedback.0 = Some(result.clone());
        if result.success {
            info!(reason = %result.reason, "mercado: ok");
        } else {
            warn!(reason = %result.reason, "mercado: recusado");
        }
    }
}

/// Quadro de orders: guarda e redesenha o painel.
fn handle_orders_snapshot(
    mut orders_events: EventReader<ClientReceiveMessage<OrdersSnapshot>>,
    mut known: ResMut<KnownOrders>,
    mut form: ResMut<MarketForm>,
) {
    for event in orders_events.read() {
        known.0 = sorted_orders(event.message().orders.clone());
        form.selected_order = form.selected_order.min(known.0.len().saturating_sub(1));
    }
}

/// Ordena o quadro para a UI: minhas primeiro, depois preço unitário.
fn sorted_orders(mut orders: Vec<OrderLine>) -> Vec<OrderLine> {
    orders.sort_by_key(|order| (!order.mine, order.unit_price, order.order_num));
    orders
}

fn catalog_items(catalog: &KnownCatalog) -> Vec<&ItemLine> {
    let mut items: Vec<_> = catalog.0.values().collect();
    items.sort_by_key(|line| line.name.as_str());
    items
}

/// O que a aba Mercado mostra; a tela de porto reconstrói o corpo quando muda.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketView {
    pub orders: Vec<(String, &'static str)>,
    pub selected: usize,
    pub focus: FormFocus,
    pub item: String,
    pub quantity: String,
    pub price: String,
}

pub fn market_view(form: &MarketForm, orders: &[OrderLine], catalog: &KnownCatalog) -> MarketView {
    let item = catalog_items(catalog)
        .get(form.item_index)
        .map(|line| line.name.clone())
        .unwrap_or_else(|| String::from("—"));
    let or_dash = |value: &str, suffix: &str| {
        if value.is_empty() {
            String::from("—")
        } else {
            format!("{value}{suffix}")
        }
    };
    MarketView {
        orders: orders
            .iter()
            .map(|order| (order_label(order), order_action_label(order)))
            .collect(),
        selected: form.selected_order,
        focus: form.focus,
        item,
        quantity: or_dash(&form.quantity, ""),
        price: or_dash(&form.unit_price, "g"),
    }
}

/// Linha de order: número, item, qtd, preço, total, região, dono.
fn order_label(order: &OrderLine) -> String {
    let total = order.unit_price.saturating_mul(u64::from(order.quantity));
    let mine = if order.mine { " [MINHA]" } else { "" };
    format!(
        "#{:<3} {:<12} {:>3}× @{}g  total {}g  {}{}",
        order.order_num,
        order.item_name,
        order.quantity,
        order.unit_price,
        total,
        order.region,
        mine,
    )
}

fn order_action_label(order: &OrderLine) -> &'static str {
    if order.mine {
        "Cancelar"
    } else {
        "Comprar"
    }
}

fn selected_bg(selected: bool) -> Color {
    if selected {
        ui::BUTTON_SELECTED
    } else {
        ui::BUTTON_BG
    }
}

fn small_button(parent: &mut ChildBuilder, label: &str, action: MarketButton) {
    parent
        .spawn((
            ui::button(
                Node {
                    min_width: Val::Px(30.0),
                    ..default()
                },
                ui::BUTTON_BG,
            ),
            action,
        ))
        .with_children(|b| {
            b.spawn(ui::text(label, 13.0, ui::TEXT));
        });
}

fn form_row(parent: &mut ChildBuilder, view: &MarketView, label: &str, field: FormFocus) {
    let value = match field {
        FormFocus::Item => view.item.as_str(),
        FormFocus::Quantity => view.quantity.as_str(),
        _ => view.price.as_str(),
    };
    let (minus, plus, minus_label, plus_label) = match field {
        FormFocus::Item => (MarketButton::Item(-1), MarketButton::Item(1), "<", ">"),
        _ => (
            MarketButton::Step(field, -1),
            MarketButton::Step(field, 1),
            "-",
            "+",
        ),
    };
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                ui::text(label, 12.0, ui::TEXT_DIM),
                Node {
                    width: Val::Px(48.0),
                    ..default()
                },
            ));
            small_button(row, minus_label, minus);
            row.spawn((
                ui::button(
                    Node {
                        flex_grow: 1.0,
                        justify_content: JustifyContent::Start,
                        ..default()
                    },
                    selected_bg(view.focus == field),
                ),
                MarketButton::Focus(field),
            ))
            .with_children(|b| {
                b.spawn(ui::text(value, 13.0, ui::TEXT));
            });
            small_button(row, plus_label, plus);
        });
}

/// Corpo da aba Mercado: orders à esquerda, formulário de venda à direita.
pub fn spawn_market_body(parent: &mut ChildBuilder, view: &MarketView) {
    parent
        .spawn(Node {
            column_gap: Val::Px(16.0),
            flex_grow: 1.0,
            ..default()
        })
        .with_children(|columns| {
            columns
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    flex_basis: Val::Px(0.0),
                    row_gap: Val::Px(4.0),
                    overflow: Overflow::clip_y(),
                    ..default()
                })
                .with_children(|list| {
                    list.spawn(ui::text("ORDENS DO MERCADO", 12.0, ui::PANEL_BORDER));
                    if view.orders.is_empty() {
                        list.spawn(ui::text("Mercado: sem orders", 13.0, ui::TEXT_DIM));
                    }
                    // ponytail: sem rolagem; adicionar scroll quando houver mais orders que a tela aguenta.
                    for (index, (label, action)) in view.orders.iter().enumerate() {
                        let selected = index == view.selected && view.focus == FormFocus::Orders;
                        list.spawn(Node {
                            column_gap: Val::Px(6.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                ui::button(
                                    Node {
                                        flex_grow: 1.0,
                                        justify_content: JustifyContent::Start,
                                        ..default()
                                    },
                                    selected_bg(selected),
                                ),
                                MarketButton::SelectOrder(index),
                            ))
                            .with_children(|b| {
                                b.spawn(ui::text(label.as_str(), 13.0, ui::TEXT));
                            });
                            row.spawn((
                                ui::button(Node::default(), ui::BUTTON_BG),
                                MarketButton::ExecuteOrder(index),
                            ))
                            .with_children(|b| {
                                b.spawn(ui::text(*action, 13.0, ui::GOLD));
                            });
                        });
                    }
                });
            columns
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    width: Val::Px(300.0),
                    flex_shrink: 0.0,
                    row_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|form| {
                    form.spawn(ui::text("VENDER", 12.0, ui::PANEL_BORDER));
                    form_row(form, view, "Item", FormFocus::Item);
                    form_row(form, view, "Qtd", FormFocus::Quantity);
                    form_row(form, view, "Preço", FormFocus::Price);
                    form.spawn((
                        ui::button(Node::default(), ui::BUTTON_SELECTED),
                        MarketButton::Submit,
                    ))
                    .with_children(|b| {
                        b.spawn(ui::text("Criar ordem de venda", 13.0, ui::GOLD));
                    });
                    form.spawn(ui::text(
                        "Clique no campo e digite · Shift+clique: ±10 · Enter envia",
                        11.0,
                        ui::TEXT_DIM,
                    ));
                });
        });
}

/// Intenção de mercado produzida pelo painel (testável sem rede).
#[derive(Debug, Clone, PartialEq, Eq)]
enum MarketIntent {
    Create(CreateSellOrder),
    Cancel(CancelSellOrder),
    Buy(BuySellOrder),
}

/// Sem validação local: o servidor é a lei e responde via `MarketResult`.
fn form_create_intent(form: &MarketForm, catalog: &KnownCatalog) -> Option<MarketIntent> {
    let item = catalog_items(catalog).get(form.item_index)?.id;
    let quantity = form.quantity.parse::<u32>().unwrap_or_default();
    let unit_price = form.unit_price.parse::<u64>().unwrap_or_default();
    Some(MarketIntent::Create(CreateSellOrder {
        item,
        quantity,
        unit_price,
    }))
}

fn order_intent(order: &OrderLine) -> MarketIntent {
    if order.mine {
        MarketIntent::Cancel(CancelSellOrder {
            order_num: order.order_num,
        })
    } else {
        MarketIntent::Buy(BuySellOrder {
            order_num: order.order_num,
            quantity: order.quantity,
        })
    }
}

fn next_focus(focus: FormFocus) -> FormFocus {
    match focus {
        FormFocus::Orders => FormFocus::Item,
        FormFocus::Item => FormFocus::Quantity,
        FormFocus::Quantity => FormFocus::Price,
        FormFocus::Price => FormFocus::Orders,
    }
}

/// Teclado do painel: Tab troca campo, setas escolhem, Enter envia.
/// Só vale com a aba Mercado aberta (antes digitava/comprava até no mar).
#[allow(clippy::too_many_arguments)]
pub fn handle_market_panel_input(
    mut keyboard: EventReader<KeyboardInput>,
    docked: Res<MyDocked>,
    state: Res<PortScreenState>,
    mut form: ResMut<MarketForm>,
    catalog: Res<KnownCatalog>,
    orders: Res<KnownOrders>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    if !docked.0 || state.active_tab != PortTab::Market {
        keyboard.clear();
        return;
    }
    let mut intent = None;
    for event in keyboard.read() {
        if event.state != ButtonState::Pressed || event.repeat {
            continue;
        }
        match &event.logical_key {
            Key::Tab => form.focus = next_focus(form.focus),
            Key::ArrowUp => {
                if form.focus == FormFocus::Orders {
                    form.selected_order = form.selected_order.saturating_sub(1);
                }
            }
            Key::ArrowDown => {
                if form.focus == FormFocus::Orders {
                    form.selected_order = form
                        .selected_order
                        .saturating_add(1)
                        .min(orders.0.len().saturating_sub(1));
                }
            }
            Key::ArrowLeft => {
                if form.focus == FormFocus::Item {
                    form.item_index = form.item_index.saturating_sub(1);
                }
            }
            Key::ArrowRight => {
                if form.focus == FormFocus::Item {
                    form.item_index = form
                        .item_index
                        .saturating_add(1)
                        .min(catalog_items(&catalog).len().saturating_sub(1));
                }
            }
            Key::Enter => {
                intent = match form.focus {
                    FormFocus::Orders => orders.0.get(form.selected_order).map(order_intent),
                    _ => form_create_intent(&form, &catalog),
                };
            }
            Key::Character(chars) => {
                if let Some(digit) = chars.chars().next().filter(|digit| digit.is_ascii_digit()) {
                    match form.focus {
                        FormFocus::Quantity => form.quantity.push(digit),
                        FormFocus::Price => form.unit_price.push(digit),
                        _ => {}
                    }
                }
            }
            Key::Backspace => match form.focus {
                FormFocus::Quantity => {
                    form.quantity.pop();
                }
                FormFocus::Price => {
                    form.unit_price.pop();
                }
                _ => {}
            },
            _ => {}
        }
    }
    if let Some(intent) = intent {
        send_intent(&mut connection_manager, intent);
    }
}

fn send_intent(connection_manager: &mut ConnectionManager, intent: MarketIntent) {
    let _ = match intent {
        MarketIntent::Create(message) => {
            connection_manager.send_message::<ReliableChannel, _>(&message)
        }
        MarketIntent::Cancel(message) => {
            connection_manager.send_message::<ReliableChannel, _>(&message)
        }
        MarketIntent::Buy(message) => {
            connection_manager.send_message::<ReliableChannel, _>(&message)
        }
    };
}

/// Soma `delta` a um campo numérico do formulário, sem ficar negativo.
fn step_field(value: &str, delta: i64) -> String {
    let current = value.parse::<i64>().unwrap_or_default();
    current.saturating_add(delta).max(0).to_string()
}

/// Efeito de um clique no painel: mesmas transições do teclado.
fn apply_market_button(
    form: &mut MarketForm,
    button: MarketButton,
    shift: bool,
    catalog: &KnownCatalog,
    orders: &[OrderLine],
) -> Option<MarketIntent> {
    let scale: i64 = if shift { 10 } else { 1 };
    match button {
        MarketButton::SelectOrder(index) => {
            form.focus = FormFocus::Orders;
            form.selected_order = index;
        }
        MarketButton::ExecuteOrder(index) => {
            form.focus = FormFocus::Orders;
            form.selected_order = index;
            return orders.get(index).map(order_intent);
        }
        MarketButton::Focus(field) => form.focus = field,
        MarketButton::Item(delta) => {
            form.focus = FormFocus::Item;
            let last = catalog_items(catalog).len().saturating_sub(1);
            form.item_index = form
                .item_index
                .saturating_add_signed(isize::from(delta))
                .min(last);
        }
        MarketButton::Step(field, delta) => {
            form.focus = field;
            let delta = i64::from(delta) * scale;
            match field {
                FormFocus::Quantity => form.quantity = step_field(&form.quantity, delta),
                FormFocus::Price => form.unit_price = step_field(&form.unit_price, delta),
                _ => {}
            }
        }
        MarketButton::Submit => return form_create_intent(form, catalog),
    }
    None
}

pub fn handle_market_clicks(
    buttons: Query<(&Interaction, &MarketButton), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut form: ResMut<MarketForm>,
    catalog: Res<KnownCatalog>,
    orders: Res<KnownOrders>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Some(intent) = apply_market_button(&mut form, *button, shift, &catalog, &orders.0) {
            send_intent(&mut connection_manager, intent);
        }
    }
}

/// Z/X/V/N/B — a interface de mercado do slice (§45: você opera no porto
/// onde está; o servidor recusa o resto). MARVYR_AUTOMARKET=1 faz o
/// ciclo depositar → vender → comprar → retirar sozinho (§39).
// System Bevy: params são injeção de dependência, não assinatura.
#[allow(clippy::too_many_arguments)]
pub fn send_market_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    wallet: Res<Wallet>,
    known_catalog: Res<KnownCatalog>,
    known_orders: Res<KnownOrders>,
    mut auto_timer: Local<f32>,
    mut auto_step: Local<u8>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    let manual_deposit = keys.just_pressed(KeyCode::KeyZ);
    let manual_withdraw = keys.just_pressed(KeyCode::KeyX);
    let manual_sell = keys.just_pressed(KeyCode::KeyV);
    let manual_cancel = keys.just_pressed(KeyCode::KeyN);
    let manual_buy = keys.just_pressed(KeyCode::KeyB);

    let mut auto = None;
    if automarket_enabled() {
        *auto_timer += time.delta_secs();
        if *auto_timer >= 2.5 {
            *auto_timer = 0.0;
            auto = Some(match *auto_step {
                0 => AutoStep::Deposit,
                1 => AutoStep::Sell,
                2 => AutoStep::Buy,
                _ => AutoStep::Withdraw,
            });
            *auto_step = (*auto_step + 1) % 4;
        }
    }

    if manual_deposit || auto.is_some_and(|step| step == AutoStep::Deposit) {
        let _ = connection_manager.send_message::<ReliableChannel, _>(&StorageDepositAll);
    }
    if manual_withdraw || auto.is_some_and(|step| step == AutoStep::Withdraw) {
        let _ = connection_manager.send_message::<ReliableChannel, _>(&StorageWithdrawAll);
    }
    if manual_sell || auto.is_some_and(|step| step == AutoStep::Sell) {
        // Dev pricing: vende TODO o estoque local de Madeira a 5g.
        if let Some(line) = known_catalog.0.get("Madeira") {
            let intent = CreateSellOrder {
                item: line.id,
                quantity: u32::MAX, // servidor corta ao que existe no storage
                unit_price: dev_price("Madeira"),
            };
            let _ = connection_manager.send_message::<ReliableChannel, _>(&intent);
        }
    }
    if manual_cancel {
        // Cancela a order sua mais antiga que ainda está no quadro.
        if let Some(order) = known_orders.0.iter().find(|order| order.mine) {
            let _ = connection_manager.send_message::<ReliableChannel, _>(&CancelSellOrder {
                order_num: order.order_num,
            });
        }
    }
    if manual_buy || auto.is_some_and(|step| step == AutoStep::Buy) {
        // Compra a order mais barata que a carteira alcança (o servidor
        // valida a região do porto onde você está, §44/§45).
        let affordable = known_orders
            .0
            .iter()
            .filter(|order| order.unit_price <= wallet.0)
            .min_by_key(|order| order.unit_price)
            .cloned();
        if let Some(order) = affordable {
            let quantity = (wallet.0 / order.unit_price).min(order.quantity as u64) as u32;
            if quantity > 0 {
                let _ = connection_manager.send_message::<ReliableChannel, _>(&BuySellOrder {
                    order_num: order.order_num,
                    quantity,
                });
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoStep {
    Deposit,
    Sell,
    Buy,
    Withdraw,
}

fn automarket_enabled() -> bool {
    std::env::var_os("MARVYR_AUTOMARKET").is_some()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use marvyr_shared::ids::ItemDefinitionId;

    use super::*;

    fn line(id: ItemDefinitionId, name: &str) -> ItemLine {
        ItemLine {
            id,
            name: String::from(name),
            weight: 2,
            equipment_slot: None,
        }
    }

    fn order(
        order_num: u32,
        item_name: &str,
        unit_price: u64,
        quantity: u32,
        mine: bool,
    ) -> OrderLine {
        OrderLine {
            order_num,
            region: String::from("Porto da Serra"),
            item_name: String::from(item_name),
            unit_price,
            quantity,
            mine,
        }
    }

    #[test]
    fn market_panel_renders_item_quantity_price_total_region_and_mine_tag() {
        let orders = vec![order(7, "Madeira", 5, 10, true)];
        let view = market_view(&MarketForm::default(), &orders, &KnownCatalog::default());
        let (label, action) = &view.orders[0];
        let text = format!("{label} {action}");

        for expected in [
            "#7",
            "Madeira",
            "10×",
            "5g",
            "Porto da Serra",
            "50g",
            "[MINHA]",
            "Cancelar",
        ] {
            assert!(text.contains(expected), "{text}");
        }
    }

    #[test]
    fn orders_sort_mine_first_then_cheapest() {
        let orders = sorted_orders(vec![
            order(1, "Madeira", 10, 1, false),
            order(2, "Madeira", 20, 1, true),
            order(3, "Madeira", 5, 1, false),
        ]);

        assert_eq!(
            orders
                .iter()
                .map(|order| order.order_num)
                .collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }

    #[test]
    fn form_submission_triggers_create_sell_order() {
        let id = ItemDefinitionId::new();
        let catalog = KnownCatalog(HashMap::from([(
            String::from("Madeira"),
            line(id, "Madeira"),
        )]));
        let form = MarketForm {
            item_index: 0,
            quantity: String::from("12"),
            unit_price: String::from("5"),
            ..MarketForm::default()
        };

        let MarketIntent::Create(intent) =
            form_create_intent(&form, &catalog).expect("catálogo tem item")
        else {
            panic!("esperava CreateSellOrder");
        };
        assert_eq!(intent.item, id);
        assert_eq!(intent.quantity, 12);
        assert_eq!(intent.unit_price, 5);
    }

    #[test]
    fn mine_row_triggers_cancel_sell_order() {
        let order = order(7, "Madeira", 5, 10, true);

        assert_eq!(
            order_intent(&order),
            MarketIntent::Cancel(CancelSellOrder { order_num: 7 })
        );
    }

    #[test]
    fn other_row_triggers_full_buy_sell_order() {
        let order = order(7, "Madeira", 5, 10, false);

        assert_eq!(
            order_intent(&order),
            MarketIntent::Buy(BuySellOrder {
                order_num: 7,
                quantity: 10,
            })
        );
    }

    #[test]
    fn step_buttons_adjust_fields_and_never_go_negative() {
        let catalog = KnownCatalog::default();
        let mut form = MarketForm::default();
        let step = |form: &mut MarketForm, field, delta, shift| {
            apply_market_button(form, MarketButton::Step(field, delta), shift, &catalog, &[])
        };

        assert_eq!(step(&mut form, FormFocus::Quantity, 1, false), None);
        assert_eq!(form.quantity, "1");
        assert_eq!(form.focus, FormFocus::Quantity);
        step(&mut form, FormFocus::Price, 1, true);
        assert_eq!(form.unit_price, "10");
        step(&mut form, FormFocus::Price, -1, true);
        step(&mut form, FormFocus::Price, -1, true);
        assert_eq!(form.unit_price, "0");
    }

    #[test]
    fn order_buttons_select_or_execute_like_enter() {
        let orders = vec![
            order(3, "Madeira", 5, 2, false),
            order(4, "Madeira", 6, 1, true),
        ];
        let catalog = KnownCatalog::default();
        let mut form = MarketForm {
            focus: FormFocus::Price,
            ..MarketForm::default()
        };

        let intent = apply_market_button(
            &mut form,
            MarketButton::SelectOrder(1),
            false,
            &catalog,
            &orders,
        );
        assert_eq!(intent, None);
        assert_eq!((form.focus, form.selected_order), (FormFocus::Orders, 1));

        let intent = apply_market_button(
            &mut form,
            MarketButton::ExecuteOrder(1),
            false,
            &catalog,
            &orders,
        );
        assert_eq!(
            intent,
            Some(MarketIntent::Cancel(CancelSellOrder { order_num: 4 }))
        );
    }

    #[test]
    fn submit_button_creates_sell_order_from_form() {
        let id = ItemDefinitionId::new();
        let catalog = KnownCatalog(HashMap::from([(
            String::from("Madeira"),
            line(id, "Madeira"),
        )]));
        let mut form = MarketForm {
            quantity: String::from("3"),
            unit_price: String::from("7"),
            ..MarketForm::default()
        };
        let intent = apply_market_button(&mut form, MarketButton::Submit, false, &catalog, &[]);
        assert_eq!(
            intent,
            Some(MarketIntent::Create(CreateSellOrder {
                item: id,
                quantity: 3,
                unit_price: 7,
            }))
        );
    }
}
