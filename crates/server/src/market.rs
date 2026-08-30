//! Mercado regional no servidor (PRD §31, §43-47, MF-023..026).
//!
//! Regras do slice:
//! - Gold é carteira global do personagem e não afunda com o navio (§31).
//! - Storage é separado por `RegionId`; não existe GlobalStorage (§30).
//! - Sell order move item do storage pro escrow atomicamente (MF-024).
//! - Order nunca cruza região (§44) e você só opera no porto onde está (§45).
//! - Listing fee (1%) e transaction tax (3%) queimam ouro, com ledger (§46/47).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use bevy::app::AppExit;
use bevy::ecs::prelude::*;
use bevy::time::Time;
use chrono::Utc;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_economy::{
    validate_new_order, FeePolicy, Ledger, LedgerKind, MarketError, MarketOrder, Money, OrderStatus,
};
use mareforge_domain_items::{CargoHold, Custody, ItemCatalog, ItemLocation};
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

/// Estado econômico serializável (MF-027, Phase 9): sobrevive a restart.
/// O alvo de produção é o Postgres do ADR-0004; o slice persiste snapshot
/// em arquivo (`MAREFORGE_STATE_PATH`), mesmo contrato de estado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub identities: HashMap<u64, CharacterId>,
    pub balances: HashMap<CharacterId, Money>,
    /// Chave composta vira lista de entradas: JSON não aceita chave-tupla.
    pub storage: Vec<StorageEntry>,
    pub escrow: Vec<EscrowEntry>,
    pub board: Vec<MarketOrder>,
    pub order_nums: HashMap<u32, MarketOrderId>,
    pub next_order_num: u32,
    pub ledger: Ledger,
}

/// Uma gaveta de storage regional no snapshot (personagem × região).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    pub character: CharacterId,
    pub region: RegionId,
    pub stacks: Vec<Custody>,
}

/// Um escrow de order no snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowEntry {
    pub order_num: u32,
    pub stacks: Vec<Custody>,
}

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

    /// Estado atual como snapshot persistível.
    pub fn snapshot(&self) -> MarketSnapshot {
        MarketSnapshot {
            identities: self.identities.clone(),
            balances: self.balances.clone(),
            storage: self
                .storage
                .iter()
                .map(|((character, region), stacks)| StorageEntry {
                    character: *character,
                    region: *region,
                    stacks: stacks.clone(),
                })
                .collect(),
            escrow: self
                .escrow
                .iter()
                .map(|(order_num, stacks)| EscrowEntry {
                    order_num: *order_num,
                    stacks: stacks.clone(),
                })
                .collect(),
            board: self.board.clone(),
            order_nums: self.order_nums.clone(),
            next_order_num: self.next_order_num,
            ledger: self.ledger.clone(),
        }
    }

    /// Reconstrói o estado de um snapshot (boot com MAREFORGE_STATE_PATH).
    pub fn restore(snapshot: MarketSnapshot) -> Self {
        Self {
            identities: snapshot.identities,
            balances: snapshot.balances,
            storage: snapshot
                .storage
                .into_iter()
                .map(|entry| ((entry.character, entry.region), entry.stacks))
                .collect(),
            escrow: snapshot
                .escrow
                .into_iter()
                .map(|entry| (entry.order_num, entry.stacks))
                .collect(),
            board: snapshot.board,
            order_nums: snapshot.order_nums,
            next_order_num: snapshot.next_order_num,
            policy: FeePolicy::default(),
            ledger: snapshot.ledger,
        }
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

    // ===== Operações atômicas (§70: apenas uma vence) =====
    // Os handlers Bevy são casca fina: detectam porto e traduzem resultado.
    // A lógica vive aqui, onde testes de concorrência chegam sem ECS.

    /// Deposita TODO o porão no storage regional (MF-023).
    pub fn deposit_all(
        &mut self,
        character: CharacterId,
        region: RegionId,
        hold: &mut CargoHold,
        catalog: &ItemCatalog,
    ) -> Result<(usize, u32), MarketError> {
        let drained = hold.drain();
        if drained.is_empty() {
            return Err(MarketError::EmptyStorage);
        }
        let weight: u32 = drained
            .iter()
            .filter_map(|custody| {
                catalog
                    .get(custody.instance.definition)
                    .map(|definition| definition.base_weight * custody.instance.quantity)
            })
            .sum();
        let stacks = drained.len();
        self.storage.entry((character, region)).or_default().extend(
            drained
                .into_iter()
                .map(|custody| custody.with_location(ItemLocation::PortStorage(region))),
        );
        Ok((stacks, weight))
    }

    /// Retira do storage tudo que couber no porão (MF-023). Parcial é
    /// permitido: o que não coube fica guardado.
    pub fn withdraw_all(
        &mut self,
        character: CharacterId,
        region: RegionId,
        hold: &mut CargoHold,
        catalog: &ItemCatalog,
    ) -> Result<usize, MarketError> {
        let Some(storage) = self.storage.get_mut(&(character, region)) else {
            return Err(MarketError::EmptyStorage);
        };
        let mut withdrawn = 0usize;
        let mut index = 0;
        while index < storage.len() {
            let instance = storage[index].instance.clone();
            match hold.insert(catalog, instance) {
                Ok(()) => {
                    storage.remove(index);
                    withdrawn += 1;
                }
                Err(_) => index += 1,
            }
        }
        Ok(withdrawn)
    }

    /// Cria sell order: storage → escrow atômico + listing fee (MF-024/§46).
    /// Retorna o número de protocolo da order.
    pub fn create_order(
        &mut self,
        character: CharacterId,
        region: RegionId,
        item: ItemDefinitionId,
        quantity: u32,
        unit_price: Money,
    ) -> Result<(u32, Money), MarketError> {
        validate_new_order(unit_price, quantity)?;
        let available = self
            .storage
            .get(&(character, region))
            .map(|storage| storage_quantity(storage, item))
            .unwrap_or(0);
        let quantity = quantity.min(available);
        if quantity == 0 {
            return Err(MarketError::NotInStorage);
        }
        validate_new_order(unit_price, quantity)?;

        // Listing fee (§46): sobre o valor anunciado, queima, não reembolsa.
        let fee = self.policy.listing_fee(unit_price, quantity);
        self.debit(character, fee)?;
        self.ledger
            .record(LedgerKind::Burn, fee, "listing fee (§46)");

        // Escrow atômico (MF-024): falha nenhuma chega depois daqui.
        let order_id = MarketOrderId::new();
        let escrowed = take_from_storage(
            self.storage
                .get_mut(&(character, region))
                .expect("checado acima"),
            item,
            quantity,
            ItemLocation::MarketEscrow(order_id),
        );
        let order_num = self.next_order_num;
        self.next_order_num += 1;
        self.order_nums.insert(order_num, order_id);
        self.escrow.insert(order_num, escrowed);
        self.board.push(MarketOrder {
            id: order_id,
            seller: character,
            item,
            quantity,
            unit_price,
            region,
            status: OrderStatus::Open,
            created_at: Utc::now(),
            expires_at: Utc::now(),
            filled_quantity: 0,
        });
        Ok((order_num, fee))
    }

    /// Cancela SUA order: item volta do escrow pro storage; fee não volta.
    pub fn cancel_order(
        &mut self,
        character: CharacterId,
        order_num: u32,
    ) -> Result<(), MarketError> {
        let order_id = self
            .order_nums
            .get(&order_num)
            .copied()
            .ok_or(MarketError::UnknownOrder)?;
        let position = self
            .board
            .iter()
            .position(|order| order.id == order_id)
            .ok_or(MarketError::OrderNotOpen)?;
        if self.board[position].seller != character {
            return Err(MarketError::NotOrderOwner);
        }
        let order = self.board.remove(position);
        if let Some(escrowed) = self.escrow.remove(&order_num) {
            self.storage
                .entry((character, order.region))
                .or_default()
                .extend(
                    escrowed.into_iter().map(|custody| {
                        custody.with_location(ItemLocation::PortStorage(order.region))
                    }),
                );
        }
        Ok(())
    }

    /// Executa uma compra (MF-025/§43-47): ouro sai, seller recebe líquido,
    /// tax queima, item flui escrow → storage do comprador. Tudo-ou-nada
    /// por unidade; apenas uma operação vence a unidade (§70).
    pub fn buy(
        &mut self,
        buyer: CharacterId,
        buyer_region: RegionId,
        order_num: u32,
        quantity: u32,
    ) -> Result<BuyReceipt, MarketError> {
        let order_id = self
            .order_nums
            .get(&order_num)
            .copied()
            .ok_or(MarketError::UnknownOrder)?;
        let position = self
            .board
            .iter()
            .position(|order| order.id == order_id)
            .ok_or(MarketError::OrderNotOpen)?;
        let order = self.board[position].clone();
        if order.region != buyer_region {
            return Err(MarketError::RegionMismatch);
        }
        if order.status != OrderStatus::Open && order.status != OrderStatus::Partial {
            return Err(MarketError::OrderNotOpen);
        }
        let available = order.quantity - order.filled_quantity;
        let quantity = quantity.min(available);
        if quantity == 0 {
            return Err(MarketError::InsufficientQuantity {
                requested: quantity,
                available,
            });
        }
        let total = self.policy.total(order.unit_price, quantity);
        self.debit(buyer, total)?;

        // Dinheiro flui: seller recebe líquido; a taxa queima (§47).
        let net = self.policy.net_proceeds(total);
        let tax = self.policy.transaction_tax(total);
        self.credit(order.seller, net);
        self.ledger.record(LedgerKind::Trade, total, "market trade");
        self.ledger
            .record(LedgerKind::Burn, tax, "transaction tax (§47)");

        // Item flui: escrow → storage do comprador nesta região (MF-025).
        let escrowed = self.escrow.entry(order_num).or_default();
        let moved = take_from_storage(
            escrowed,
            order.item,
            quantity,
            ItemLocation::PortStorage(buyer_region),
        );
        self.storage
            .entry((buyer, buyer_region))
            .or_default()
            .extend(moved);

        // Status (§43: execução por unidade; parcial é permitido).
        let new_filled = order.filled_quantity + quantity;
        let filled_completely = new_filled >= order.quantity;
        let board_order = &mut self.board[position];
        board_order.filled_quantity = new_filled;
        board_order.status = if filled_completely {
            OrderStatus::Filled
        } else {
            OrderStatus::Partial
        };
        if filled_completely {
            self.board.remove(position);
            self.escrow.remove(&order_num);
        }

        Ok(BuyReceipt {
            order_num,
            seller: order.seller,
            item: order.item,
            quantity,
            total,
            net,
            tax,
        })
    }
}

/// Recibo de execução de compra (para log/telemetria do handler).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuyReceipt {
    pub order_num: u32,
    pub seller: CharacterId,
    pub item: ItemDefinitionId,
    pub quantity: u32,
    pub total: Money,
    pub net: Money,
    pub tax: Money,
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
        match market.deposit_all(character, region_id, &mut ship.hold, &dev.catalog) {
            Ok((stacks, weight)) => {
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
            Err(_) => {
                market_result(&mut connection_manager, client_id, false, "porão vazio");
            }
        }
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
        match market.withdraw_all(character, region_id, &mut ship.hold, &dev.catalog) {
            Ok(0) => {
                info!(
                    ship_id = ship.ship_id,
                    "saque recusado: porão sem espaço ou storage vazio"
                );
                market_result(
                    &mut connection_manager,
                    client_id,
                    false,
                    "nada a retirar (storage vazio ou porão cheio)",
                );
            }
            Ok(stacks) => {
                info!(
                    ship_id = ship.ship_id,
                    region = name,
                    stacks,
                    "carga retirada do storage regional"
                );
                market_result(
                    &mut connection_manager,
                    client_id,
                    true,
                    &format!("retirado de {name}"),
                );
            }
            Err(_) => {
                market_result(
                    &mut connection_manager,
                    client_id,
                    false,
                    "storage vazio nesta região",
                );
            }
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
        match market.create_order(
            character,
            region_id,
            message.item,
            message.quantity,
            Money(message.unit_price),
        ) {
            Ok((order_num, fee)) => {
                let item_name = dev
                    .catalog
                    .get(message.item)
                    .map(|definition| definition.display_name.clone())
                    .unwrap_or_default();
                info!(
                    order_num,
                    region = name,
                    item = %item_name,
                    quantity = message.quantity,
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
            Err(error) => {
                warn!(error = %error, "sell order recusada");
                market_result(
                    &mut connection_manager,
                    client_id,
                    false,
                    &error.to_string(),
                );
            }
        }
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
        match market.buy(buyer, buyer_region_id, message.order_num, message.quantity) {
            Ok(receipt) => {
                info!(
                    order_num = receipt.order_num,
                    region = buyer_region_name,
                    quantity = receipt.quantity,
                    total = receipt.total.0,
                    net_proceeds = receipt.net.0,
                    tax = receipt.tax.0,
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
                send_wallet(&mut connection_manager, &market, &ships, receipt.seller);
                market_result(
                    &mut connection_manager,
                    client_id,
                    true,
                    &format!(
                        "comprado: {} un. por {}g",
                        receipt.quantity, receipt.total.0
                    ),
                );
            }
            Err(error) => {
                warn!(error = %error, "compra recusada");
                market_result(
                    &mut connection_manager,
                    client_id,
                    false,
                    &error.to_string(),
                );
            }
        }
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
        match market.cancel_order(character, order_num) {
            Ok(()) => {
                info!(
                    order_num,
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
            Err(error) => {
                warn!(error = %error, "cancelamento recusado");
                market_result(
                    &mut connection_manager,
                    client_id,
                    false,
                    &error.to_string(),
                );
            }
        }
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

/// Carrega o snapshot persistido no boot (MF-027). Sem
/// `MAREFORGE_STATE_PATH`, o mundo nasce limpo — comportamento de dev.
pub fn load_state(mut market: ResMut<ServerMarket>) {
    let Some(path) = std::env::var_os("MAREFORGE_STATE_PATH") else {
        return;
    };
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<MarketSnapshot>(&bytes) {
            Ok(snapshot) => {
                *market = ServerMarket::restore(snapshot);
                info!(
                    path = %path.to_string_lossy(),
                    "estado econômico restaurado do snapshot"
                );
            }
            Err(error) => {
                warn!(error = %error, "snapshot ilegível; começando limpo");
            }
        },
        Err(_) => {
            info!("sem snapshot anterior; começando limpo");
        }
    }
}

/// Salva o snapshot a cada 10s (e tenta no AppExit). Pior caso de perda:
/// o intervalo — aceitável no slice; produção usará Postgres (ADR-0004).
pub fn save_state(
    market: Res<ServerMarket>,
    mut exit: EventReader<AppExit>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    const SAVE_INTERVAL: f32 = 10.0;
    *timer += time.delta_secs();
    let exiting = exit.read().next().is_some();
    if *timer < SAVE_INTERVAL && !exiting {
        return;
    }
    *timer = 0.0;
    let Some(path) = std::env::var_os("MAREFORGE_STATE_PATH") else {
        return;
    };
    let snapshot = market.snapshot();
    match serde_json::to_vec_pretty(&snapshot) {
        Ok(bytes) => {
            let mut temp = std::path::PathBuf::from(&path);
            temp.set_extension("tmp");
            // Escrita atômica via rename: crash no meio não corrompe o arquivo.
            if std::fs::write(&temp, bytes)
                .and_then(|()| std::fs::rename(&temp, &path))
                .is_ok()
            {
                info!(path = %path.to_string_lossy(), "estado econômico persistido");
            } else {
                warn!("falha ao persistir estado econômico");
            }
        }
        Err(error) => warn!(error = %error, "falha ao serializar snapshot"),
    }
}
