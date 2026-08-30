//! Mercado no client (PRD MF-023..026). O client vê catálogo, carteira e o
//! quadro de orders; storage e execução são do servidor (Pilar 4). O painel
//! MF-040 é Text2d + teclado: lista orders, cria/cancela/compra e mostra o
//! veredito `MarketResult` do servidor. Z/X/V/N/B continuam como atalhos dev
//! (escondidos do HUD desde MF-043).

use std::collections::HashMap;

use crate::net::ReliableChannel;
use bevy::ecs::prelude::*;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use mareforge_protocol::{
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
pub struct KnownOrders(pub Vec<mareforge_protocol::OrderLine>);

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

/// Texto do HUD de mercado (carteira + quadro), filho da câmera.
#[derive(Component)]
pub struct MarketReadout;

/// Texto do formulário de venda, filho da câmera.
#[derive(Component)]
pub struct MarketFormReadout;

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
                    update_market_readout,
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

/// Painel de mercado (MF-040): esquerda lista, direita formulário.
pub fn spawn_market_panel(mut commands: Commands, camera: Query<Entity, With<Camera2d>>) {
    let Ok(camera) = camera.get_single() else {
        return;
    };
    commands
        .spawn((
            MarketReadout,
            Text2d::new("Ouro: —\nMercado: —"),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.9, 0.8)),
            Anchor::CenterLeft,
            Transform::from_xyz(-300.0, 100.0, 10.0),
        ))
        .set_parent(camera);
    commands
        .spawn((
            MarketFormReadout,
            Text2d::new("VENDER"),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::srgb(0.95, 0.87, 0.55)),
            Anchor::CenterLeft,
            Transform::from_xyz(80.0, 100.0, 10.0),
        ))
        .set_parent(camera);
}

fn catalog_items(catalog: &KnownCatalog) -> Vec<&ItemLine> {
    let mut items: Vec<_> = catalog.0.values().collect();
    items.sort_by_key(|line| line.name.as_str());
    items
}

/// Texto da lista de orders: ordem, item, qtd, preço, total, região, dono.
fn update_market_readout(
    wallet: Res<Wallet>,
    known: Res<KnownOrders>,
    feedback: Res<MarketFeedback>,
    form: Res<MarketForm>,
    catalog: Res<KnownCatalog>,
    mut readouts: Query<(
        &mut Text2d,
        Option<&MarketReadout>,
        Option<&MarketFormReadout>,
    )>,
) {
    for (mut text, readout, form_readout) in &mut readouts {
        if readout.is_some() {
            text.0 = orders_text(wallet.0, &known.0, &feedback.0, &form);
        } else if form_readout.is_some() {
            text.0 = form_text(&catalog, &form, &feedback.0);
        }
    }
}

fn orders_text(
    wallet: u64,
    orders: &[OrderLine],
    feedback: &Option<MarketResult>,
    form: &MarketForm,
) -> String {
    let mut lines = vec![format!("Ouro: {}g", wallet)];
    if orders.is_empty() {
        lines.push(String::from("Mercado: sem orders"));
    } else {
        lines.push(String::from("Mercado (↑↓ escolhe · Enter executa):"));
        // ponytail: sem rolagem; adicionar scroll quando houver mais orders que a tela aguenta.
        for (index, order) in orders.iter().enumerate() {
            let marker = if index == form.selected_order {
                ">"
            } else {
                " "
            };
            let mine = if order.mine { " [MINHA]" } else { "" };
            let action = if order.mine {
                "[Cancelar]"
            } else {
                "[Comprar]"
            };
            let total = order.unit_price.saturating_mul(u64::from(order.quantity));
            lines.push(format!(
                "{marker}#{:<3} {:<12} {:>3}× @{}g {:<14} total={}g{}{}",
                order.order_num,
                order.item_name,
                order.quantity,
                order.unit_price,
                order.region,
                total,
                mine,
                action,
            ));
        }
    }
    if let Some(result) = feedback {
        let prefix = if result.success { "OK" } else { "ERRO" };
        lines.push(format!("{prefix}: {}", result.reason));
    }
    lines.join("\n")
}

fn form_text(catalog: &KnownCatalog, form: &MarketForm, feedback: &Option<MarketResult>) -> String {
    let items = catalog_items(catalog);
    let item = items
        .get(form.item_index)
        .map(|line| line.name.as_str())
        .unwrap_or("—");
    let marker = |focus: FormFocus| {
        if form.focus == focus {
            ">"
        } else {
            " "
        }
    };
    let quantity = if form.quantity.is_empty() {
        String::from("—")
    } else {
        form.quantity.clone()
    };
    let price = if form.unit_price.is_empty() {
        String::from("—")
    } else {
        format!("{}g", form.unit_price)
    };
    let mut lines = vec![
        String::from("VENDER (Tab troca campo · Enter enviar)"),
        format!("{}Item: {}  <-/->", marker(FormFocus::Item), item),
        format!("{}Qtd: {}", marker(FormFocus::Quantity), quantity),
        format!("{}Preço: {}", marker(FormFocus::Price), price),
    ];
    if let Some(result) = feedback {
        let prefix = if result.success { "OK" } else { "ERRO" };
        lines.push(format!("{prefix}: {}", result.reason));
    }
    lines.join("\n")
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
pub fn handle_market_panel_input(
    mut keyboard: EventReader<KeyboardInput>,
    mut form: ResMut<MarketForm>,
    catalog: Res<KnownCatalog>,
    orders: Res<KnownOrders>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
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
        match intent {
            MarketIntent::Create(message) => {
                let _ = connection_manager.send_message::<ReliableChannel, _>(&message);
            }
            MarketIntent::Cancel(message) => {
                let _ = connection_manager.send_message::<ReliableChannel, _>(&message);
            }
            MarketIntent::Buy(message) => {
                let _ = connection_manager.send_message::<ReliableChannel, _>(&message);
            }
        }
    }
}

/// Z/X/V/N/B — a interface de mercado do slice (§45: você opera no porto
/// onde está; o servidor recusa o resto). MAREFORGE_AUTOMARKET=1 faz o
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
    std::env::var_os("MAREFORGE_AUTOMARKET").is_some()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use mareforge_shared::ids::ItemDefinitionId;

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
        let text = orders_text(1_000, &orders, &None, &MarketForm::default());

        for expected in [
            "#7",
            "Madeira",
            "10×",
            "5g",
            "Porto da Serra",
            "50g",
            "[MINHA]",
            "[Cancelar]",
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
    fn failed_market_result_surfaces_reason_in_readout() {
        let feedback = Some(MarketResult {
            success: false,
            reason: String::from("atraca primeiro (E)"),
        });

        let text = orders_text(0, &[], &feedback, &MarketForm::default());
        assert!(text.contains("atraca primeiro (E)"), "{text}");
    }
}
