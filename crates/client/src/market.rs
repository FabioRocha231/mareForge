//! Mercado no client (PRD MF-023..026). O client vê catálogo, carteira e o
//! quadro de orders; storage e execução são do servidor (Pilar 4). Teclas:
//! Z deposita o porão, X retira, V vende madeira (dev pricing), N cancela
//! sua order mais antiga, B compra a order mais barata.

use std::collections::HashMap;

use crate::net::ReliableChannel;
use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use mareforge_protocol::{
    BuySellOrder, CancelSellOrder, CatalogSnapshot, CreateSellOrder, ItemLine, MarketResult,
    OrdersSnapshot, StorageDepositAll, StorageWithdrawAll, WalletUpdated,
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

/// Texto do HUD de mercado (carteira + quadro), filho da câmera.
#[derive(Component)]
pub struct MarketReadout;

pub struct MarketPlugin;

impl Plugin for MarketPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KnownCatalog>()
            .init_resource::<Wallet>()
            .init_resource::<KnownOrders>()
            .add_systems(
                Update,
                (
                    handle_catalog_snapshot,
                    handle_wallet_updated,
                    handle_orders_snapshot,
                    handle_market_result,
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
) {
    for event in catalog_events.read() {
        known.0 = event
            .message()
            .items
            .iter()
            .map(|line| (line.name.clone(), line.clone()))
            .collect();
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

fn handle_market_result(mut events: EventReader<ClientReceiveMessage<MarketResult>>) {
    for event in events.read() {
        let result = event.message();
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
) {
    for event in orders_events.read() {
        known.0 = event.message().orders.clone();
    }
}

/// Painel direito da tela: carteira e quadro de orders.
fn update_market_readout(
    wallet: Res<Wallet>,
    known: Res<KnownOrders>,
    mut readouts: Query<&mut Text2d, With<MarketReadout>>,
) {
    let Ok(mut text) = readouts.get_single_mut() else {
        return;
    };
    let mut lines = vec![format!("Ouro: {}g", wallet.0)];
    if known.0.is_empty() {
        lines.push(String::from("Mercado: sem orders"));
    } else {
        lines.push(String::from("Mercado (num: qtd item @preço):"));
        for order in known.0.iter().take(8) {
            let mine = if order.mine { " [sua]" } else { "" };
            lines.push(format!(
                "{}: {}× {} @{}g{}",
                order.order_num, order.quantity, order.item_name, order.unit_price, mine
            ));
        }
    }
    text.0 = lines.join("\n");
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
