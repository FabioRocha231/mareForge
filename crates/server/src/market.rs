//! Mercado regional no servidor (PRD §31, §43-47, MF-023..026).
//!
//! Regras do slice:
//! - Gold é carteira global do personagem e não afunda com o navio (§31).
//! - Storage é separado por `RegionId`; não existe GlobalStorage (§30).
//! - Sell order move item do storage pro escrow atomicamente (MF-024).
//! - Order nunca cruza região (§44) e você só opera no porto onde está (§45).
//! - Listing fee (1%) e transaction tax (3%) queimam ouro, com ledger (§46/47).

use std::collections::HashMap;

use bevy::ecs::prelude::*;
use chrono::Utc;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_economy::{
    validate_new_order, FeePolicy, Ledger, LedgerKind, MarketError, MarketOrder, Money, OrderStatus,
};
use mareforge_domain_items::{Custody, ItemCatalog, ItemLocation};
use mareforge_domain_world::WorldMap;
use mareforge_protocol::{
    BuySellOrder, CancelSellOrder, CatalogSnapshot, CreateSellOrder, ItemLine, MarketResult,
    OrderLine, OrdersSnapshot, StorageDepositAll, StorageWithdrawAll, WalletUpdated,
};
use mareforge_shared::ids::{CharacterId, ItemDefinitionId, MarketOrderId, RegionId};
use tracing::{info, warn};

use crate::net::{DevItems, ReliableChannel, ServerShip, ServerWorldMap};

/// Bootstrap dev de ouro (PRD §48: development bootstrap, não é design de
/// produção).
const DEV_SEED_GOLD: u64 = 1_000;

/// Toda a máquina econômica da sessão: carteiras, storage regional, escrow
/// e as orders abertas. Um único Resource porque as operações de mercado
/// (listar, comprar) tocam várias partes atomicamente.
#[derive(Resource)]
pub struct ServerMarket {
    identities: HashMap<u64, CharacterId>,
    balances: HashMap<CharacterId, Money>,
    /// (personagem, região) → custódias guardadas (§30: storage regional).
    storage: HashMap<(CharacterId, RegionId), Vec<Custody>>,
    /// order_num → custódias em escrow, location `MarketEscrow` (MF-024).
    escrow: HashMap<u32, Vec<Custody>>,
    board: Vec<MarketOrder>,
    order_nums: HashMap<u32, MarketOrderId>,
    next_order_num: u32,
    pub policy: FeePolicy,
    pub ledger: Ledger,
}

impl Default for ServerMarket {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerMarket {
    pub fn new() -> Self {
        Self {
            identities: HashMap::new(),
            balances: HashMap::new(),
            storage: HashMap::new(),
            escrow: HashMap::new(),
            board: Vec::new(),
            order_nums: HashMap::new(),
            next_order_num: 0,
            policy: FeePolicy::default(),
            ledger: Ledger::default(),
        }
    }

    /// Identidade do client, cunhando o bootstrap dev no primeiro toque (§48).
    pub fn character(&mut self, client_num: u64) -> CharacterId {
        if let Some(id) = self.identities.get(&client_num) {
            return *id;
        }
        let id = CharacterId::new();
        self.identities.insert(client_num, id);
        self.balances.insert(id, Money(DEV_SEED_GOLD));
        self.ledger.record(
            LedgerKind::Mint,
            Money(DEV_SEED_GOLD),
            "bootstrap dev (§48)",
        );
        info!(character = ?id, gold = DEV_SEED_GOLD, "carteira semeada (dev)");
        id
    }

    pub fn balance(&self, character: CharacterId) -> Money {
        self.balances.get(&character).copied().unwrap_or(Money(0))
    }

    /// Id de personagem já criado (sem cunhar) — para personalizar broadcasts.
    pub fn peek_character(&self, client_num: u64) -> Option<CharacterId> {
        self.identities.get(&client_num).copied()
    }

    fn credit(&mut self, character: CharacterId, amount: Money) {
        let balance = self.balances.entry(character).or_insert(Money(0));
        balance.0 = balance.0.saturating_add(amount.0);
    }

    fn debit(&mut self, character: CharacterId, amount: Money) -> Result<(), MarketError> {
        let available = self.balance(character);
        if available.0 < amount.0 {
            return Err(MarketError::InsufficientFunds {
                needed: amount,
                available,
            });
        }
        self.balances
            .get_mut(&character)
            .expect("saldo verificado acima")
            .0 -= amount.0;
        Ok(())
    }
}

/// A região cujo porto contém o navio (§45: você opera onde está).
pub fn port_region(map: &WorldMap, x: f32, y: f32) -> Option<(RegionId, &str)> {
    map.regions()
        .iter()
        .find(|region| region.port.as_ref().is_some_and(|port| port.contains(x, y)))
        .and_then(|region| region.port.as_ref().map(|_| (region.id, region.name)))
}

fn storage_quantity(storage: &[Custody], item: ItemDefinitionId) -> u32 {
    storage
        .iter()
        .filter(|custody| custody.instance.definition == item)
        .map(|custody| custody.instance.quantity)
        .sum()
}

/// Retira `quantity` unidades de um item da lista, regravando a localização
/// pelo destino. Consome pilhas na ordem; parcial é permitido.
fn take_from_storage(
    storage: &mut Vec<Custody>,
    item: ItemDefinitionId,
    quantity: u32,
    destination: ItemLocation,
) -> Vec<Custody> {
    let mut remaining = quantity;
    let mut taken = Vec::new();
    let mut index = 0;
    while index < storage.len() && remaining > 0 {
        if storage[index].instance.definition != item {
            index += 1;
            continue;
        }
        let available = storage[index].instance.quantity;
        if available <= remaining {
            let custody = storage.remove(index);
            remaining -= available;
            taken.push(custody.with_location(destination));
        } else {
            storage[index].instance.quantity -= remaining;
            let mut partial = storage[index].clone();
            partial.instance.quantity = remaining;
            remaining = 0;
            taken.push(partial.with_location(destination));
        }
    }
    taken
}

fn market_result(
    connection_manager: &mut ConnectionManager,
    client_id: ClientId,
    success: bool,
    reason: &str,
) {
    let _ = connection_manager.send_message::<ReliableChannel, _>(
        client_id,
        &MarketResult {
            success,
            reason: String::from(reason),
        },
    );
}

fn region_name(map: &WorldMap, region: RegionId) -> &'static str {
    map.regions()
        .iter()
        .find(|candidate| candidate.id == region)
        .map(|candidate| candidate.name)
        .unwrap_or("?")
}

/// Deposit/withdraw do porão no storage do porto (PRD MF-023, §63 TransferItem).
pub fn handle_storage(
    mut deposit_events: EventReader<ServerReceiveMessage<StorageDepositAll>>,
    mut withdraw_events: EventReader<ServerReceiveMessage<StorageWithdrawAll>>,
    mut connection_manager: ResMut<ConnectionManager>,
    mut market: ResMut<ServerMarket>,
    dev: Res<DevItems>,
    map: Res<ServerWorldMap>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in deposit_events.read() {
        let client_id = event.from();
        let Some(mut ship) = ships.iter_mut().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        let Some((region_id, name)) = port_region(&map.0, ship.motion.x, ship.motion.y) else {
            info!(
                ship_id = ship.ship_id,
                "depósito recusado: fora de qualquer porto"
            );
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "fora de qualquer porto",
            );
            continue;
        };
        let character = market.character(ship.client_num);
        let drained = ship.hold.drain();
        if drained.is_empty() {
            market_result(&mut connection_manager, client_id, false, "porão vazio");
            continue;
        }
        let weight: u32 = drained
            .iter()
            .filter_map(|custody| {
                dev.catalog
                    .get(custody.instance.definition)
                    .map(|definition| definition.base_weight * custody.instance.quantity)
            })
            .sum();
        let stacks = drained.len();
        market
            .storage
            .entry((character, region_id))
            .or_default()
            .extend(
                drained
                    .into_iter()
                    .map(|custody| custody.with_location(ItemLocation::PortStorage(region_id))),
            );
        info!(
            ship_id = ship.ship_id,
            region = name,
            stacks,
            weight,
            "porão depositado no storage regional"
        );
        market_result(
            &mut connection_manager,
            client_id,
            true,
            &format!("depositado em {name}"),
        );
    }

    for event in withdraw_events.read() {
        let client_id = event.from();
        let Some(mut ship) = ships.iter_mut().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        let Some((region_id, name)) = port_region(&map.0, ship.motion.x, ship.motion.y) else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "fora de qualquer porto",
            );
            continue;
        };
        let character = market.character(ship.client_num);
        let key = (character, region_id);
        let Some(storage) = market.storage.get_mut(&key) else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "storage vazio nesta região",
            );
            continue;
        };
        // Pilha por pilha, enquanto o porão aceita — nada se perde; o que
        // não coube fica guardado (retirada parcial é permitida).
        let mut withdrawn = 0u32;
        let mut index = 0;
        while index < storage.len() {
            let instance = storage[index].instance.clone();
            match ship.hold.insert(&dev.catalog, instance) {
                Ok(()) => {
                    storage.remove(index);
                    withdrawn += 1;
                }
                Err(_) => index += 1,
            }
        }
        if withdrawn == 0 {
            info!(ship_id = ship.ship_id, "saque recusado: porão sem espaço");
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "porão sem espaço",
            );
        } else {
            info!(
                ship_id = ship.ship_id,
                region = name,
                stacks = withdrawn,
                "carga retirada do storage regional"
            );
            market_result(
                &mut connection_manager,
                client_id,
                true,
                &format!("retirado de {name}"),
            );
        }
    }
}

/// Sell order (MF-024/025): storage → escrow atômico; listing fee queima.
pub fn handle_sell(
    mut sell_events: EventReader<ServerReceiveMessage<CreateSellOrder>>,
    mut connection_manager: ResMut<ConnectionManager>,
    mut market: ResMut<ServerMarket>,
    dev: Res<DevItems>,
    map: Res<ServerWorldMap>,
    ships: Query<&ServerShip>,
) {
    for event in sell_events.read() {
        let client_id = event.from();
        let message = event.message();
        let Some(ship) = ships.iter().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        let Some((region_id, name)) = port_region(&map.0, ship.motion.x, ship.motion.y) else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "fora de qualquer porto",
            );
            continue;
        };
        let character = market.character(ship.client_num);
        let unit_price = Money(message.unit_price);
        if let Err(error) = validate_new_order(unit_price, message.quantity) {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                &error.to_string(),
            );
            continue;
        }

        // Consulta e validação SEM borrow mutável do storage (§43: a ordem
        // das operações garante que nenhuma metade acontece sozinha).
        let available = market
            .storage
            .get(&(character, region_id))
            .map(|storage| storage_quantity(storage, message.item))
            .unwrap_or(0);
        let quantity = message.quantity.min(available);
        if quantity == 0 {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "item não está no storage local",
            );
            continue;
        }
        if let Err(error) = validate_new_order(unit_price, quantity) {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                &error.to_string(),
            );
            continue;
        }

        // Listing fee (§46): sobre o valor anunciado, queima, não reembolsa.
        let fee = market.policy.listing_fee(unit_price, quantity);
        if let Err(error) = market.debit(character, fee) {
            warn!(error = %error, "sell order sem ouro para a taxa");
            market_result(
                &mut connection_manager,
                client_id,
                false,
                &error.to_string(),
            );
            continue;
        }
        market
            .ledger
            .record(LedgerKind::Burn, fee, "listing fee (§46)");

        // Escrow atômico (MF-024): sai do storage, entra carimbado como
        // MarketEscrow. Qualquer falha acima acontece antes deste ponto.
        let order_id = MarketOrderId::new();
        let escrowed = take_from_storage(
            market
                .storage
                .get_mut(&(character, region_id))
                .expect("checado acima: storage com o item"),
            message.item,
            quantity,
            ItemLocation::MarketEscrow(order_id),
        );
        let order_num = market.next_order_num;
        market.next_order_num += 1;
        market.order_nums.insert(order_num, order_id);
        market.escrow.insert(order_num, escrowed);
        market.board.push(MarketOrder {
            id: order_id,
            seller: character,
            item: message.item,
            quantity,
            unit_price,
            region: region_id,
            status: OrderStatus::Open,
            created_at: Utc::now(),
            expires_at: Utc::now(),
            filled_quantity: 0,
        });

        let item_name = dev
            .catalog
            .get(message.item)
            .map(|definition| definition.display_name.clone())
            .unwrap_or_default();
        info!(
            order_num,
            region = name,
            item = %item_name,
            quantity,
            unit_price = message.unit_price,
            listing_fee = fee.0,
            "sell order criada; item em escrow"
        );
        broadcast_orders(
            &mut connection_manager,
            &market,
            &dev.catalog,
            &map.0,
            &ships,
        );
        send_wallet(&mut connection_manager, &market, &ships, character);
        market_result(
            &mut connection_manager,
            client_id,
            true,
            &format!("order {order_num} listada em {name}"),
        );
    }
}

/// Compra (MF-025/026): mesma região, ouro sai do comprador, seller recebe
/// líquido da taxa, item migra escrow → storage do comprador.
pub fn handle_buy(
    mut buy_events: EventReader<ServerReceiveMessage<BuySellOrder>>,
    mut connection_manager: ResMut<ConnectionManager>,
    mut market: ResMut<ServerMarket>,
    dev: Res<DevItems>,
    map: Res<ServerWorldMap>,
    ships: Query<&ServerShip>,
) {
    for event in buy_events.read() {
        let client_id = event.from();
        let message = event.message();
        let Some(ship) = ships.iter().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        let Some((buyer_region_id, buyer_region_name)) =
            port_region(&map.0, ship.motion.x, ship.motion.y)
        else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "fora de qualquer porto",
            );
            continue;
        };
        let buyer = market.character(ship.client_num);

        let Some(order_position) = market.board.iter().position(|order| {
            market
                .order_nums
                .get(&message.order_num)
                .is_some_and(|id| *id == order.id)
        }) else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "order inexistente ou concluída",
            );
            continue;
        };
        let order = market.board[order_position].clone();
        if order.region != buyer_region_id {
            // §44: mercado não é global; §45: só o porto onde está.
            warn!(
                order_num = message.order_num,
                "compra cross-region recusada"
            );
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "essa order é de outra região",
            );
            continue;
        }
        if order.status != OrderStatus::Open && order.status != OrderStatus::Partial {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "order não está aberta",
            );
            continue;
        }
        let available = order.quantity - order.filled_quantity;
        let quantity = message.quantity.min(available);
        if quantity == 0 {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                &format!("quantidade insuficiente (tem {available})"),
            );
            continue;
        }
        let total = market.policy.total(order.unit_price, quantity);
        if let Err(error) = market.debit(buyer, total) {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                &error.to_string(),
            );
            continue;
        }

        // O dinheiro flui: seller recebe líquido; a taxa queima (§47).
        let net = market.policy.net_proceeds(total);
        let tax = market.policy.transaction_tax(total);
        market.credit(order.seller, net);
        market
            .ledger
            .record(LedgerKind::Trade, total, "market trade");
        market
            .ledger
            .record(LedgerKind::Burn, tax, "transaction tax (§47)");

        // O item flui: escrow → storage do comprador nesta região (MF-025).
        let escrowed = market.escrow.entry(message.order_num).or_default();
        let moved = take_from_storage(
            escrowed,
            order.item,
            quantity,
            ItemLocation::PortStorage(buyer_region_id),
        );
        market
            .storage
            .entry((buyer, buyer_region_id))
            .or_default()
            .extend(moved);

        // Status (§43: execução por unidade; parcial é permitido).
        let new_filled = order.filled_quantity + quantity;
        let filled_completely = new_filled >= order.quantity;
        let seller = order.seller;
        let board_order = &mut market.board[order_position];
        board_order.filled_quantity = new_filled;
        board_order.status = if filled_completely {
            OrderStatus::Filled
        } else {
            OrderStatus::Partial
        };
        if filled_completely {
            market.board.remove(order_position);
            market.escrow.remove(&message.order_num);
        }

        info!(
            order_num = message.order_num,
            region = buyer_region_name,
            quantity,
            total = total.0,
            net_proceeds = net.0,
            tax = tax.0,
            "order executada; ouro fluiu, item mudou de dono"
        );
        broadcast_orders(
            &mut connection_manager,
            &market,
            &dev.catalog,
            &map.0,
            &ships,
        );
        send_wallet(&mut connection_manager, &market, &ships, buyer);
        send_wallet(&mut connection_manager, &market, &ships, seller);
        market_result(
            &mut connection_manager,
            client_id,
            true,
            &format!("comprado: {quantity} un. por {}g", total.0),
        );
    }
}

/// Cancelamento (§63, §46): item volta do escrow pro storage da região da
/// order; o listing fee NÃO volta.
pub fn handle_cancel(
    mut cancel_events: EventReader<ServerReceiveMessage<CancelSellOrder>>,
    mut connection_manager: ResMut<ConnectionManager>,
    mut market: ResMut<ServerMarket>,
    dev: Res<DevItems>,
    map: Res<ServerWorldMap>,
    ships: Query<&ServerShip>,
) {
    for event in cancel_events.read() {
        let client_id = event.from();
        let order_num = event.message().order_num;
        let Some(ship) = ships.iter().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        let character = market.character(ship.client_num);
        let Some(order_id) = market.order_nums.get(&order_num).copied() else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "order inexistente",
            );
            continue;
        };
        let Some(order_position) = market.board.iter().position(|order| order.id == order_id)
        else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "order não está mais aberta",
            );
            continue;
        };
        if market.board[order_position].seller != character {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "essa order não é sua",
            );
            continue;
        }
        let order = market.board.remove(order_position);
        // Escrow → storage da região da order (regravando a localização).
        if let Some(escrowed) = market.escrow.remove(&order_num) {
            market
                .storage
                .entry((character, order.region))
                .or_default()
                .extend(
                    escrowed.into_iter().map(|custody| {
                        custody.with_location(ItemLocation::PortStorage(order.region))
                    }),
                );
        }
        info!(
            order_num,
            region = region_name(&map.0, order.region),
            "order cancelada; item devolvido ao storage (fee não reembolsada, §46)"
        );
        broadcast_orders(
            &mut connection_manager,
            &market,
            &dev.catalog,
            &map.0,
            &ships,
        );
        market_result(&mut connection_manager, client_id, true, "order cancelada");
    }
}

/// Snapshot de orders personalizado por client (o campo `mine` é do
/// observador). Enviado no hello e a cada mudança no board.
pub fn broadcast_orders(
    connection_manager: &mut ConnectionManager,
    market: &ServerMarket,
    catalog: &ItemCatalog,
    map: &WorldMap,
    ships: &Query<&ServerShip>,
) {
    for ship in ships.iter() {
        let viewer = market.peek_character(ship.client_num);
        let lines: Vec<OrderLine> = market
            .board
            .iter()
            .filter_map(|order| {
                let remaining = order.quantity - order.filled_quantity;
                if remaining == 0 {
                    return None;
                }
                let order_num = market
                    .order_nums
                    .iter()
                    .find(|(_, id)| **id == order.id)
                    .map(|(num, _)| *num)
                    .unwrap_or(0);
                Some(OrderLine {
                    order_num,
                    region: String::from(region_name(map, order.region)),
                    item_name: catalog
                        .get(order.item)
                        .map(|definition| definition.display_name.clone())
                        .unwrap_or_default(),
                    unit_price: order.unit_price.0,
                    quantity: remaining,
                    mine: viewer.is_some_and(|viewer| viewer == order.seller),
                })
            })
            .collect();
        let _ = connection_manager
            .send_message::<ReliableChannel, _>(ship.client_id, &OrdersSnapshot { orders: lines });
    }
}

/// Envia a carteira atualizada ao dono, se ele estiver online (§31: ouro é
/// da personagem, não do navio — a carteira sobrevive ao respawn).
fn send_wallet(
    connection_manager: &mut ConnectionManager,
    market: &ServerMarket,
    ships: &Query<&ServerShip>,
    character: CharacterId,
) {
    for ship in ships.iter() {
        if market.peek_character(ship.client_num) == Some(character) {
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                ship.client_id,
                &WalletUpdated {
                    gold: market.balance(character).0,
                },
            );
        }
    }
}

/// Catálogo de itens para o client no hello (MF-023): nomes e pesos para a
/// UI, ids reais para os intents.
pub fn catalog_snapshot(catalog: &ItemCatalog) -> CatalogSnapshot {
    CatalogSnapshot {
        items: catalog
            .items()
            .map(|definition| ItemLine {
                id: definition.id,
                name: definition.display_name.clone(),
                weight: definition.base_weight,
            })
            .collect(),
    }
}
