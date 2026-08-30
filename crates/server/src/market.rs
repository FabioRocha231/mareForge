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
use chrono::{DateTime, Utc};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_crafting::{craft_in_storage, CraftError, Recipe, StationKind};
use mareforge_domain_economy::{
    validate_new_order, FeePolicy, Ledger, LedgerKind, MarketError, MarketOrder, Money, OrderStatus,
};
use mareforge_domain_items::{
    put_stack, CargoHold, Custody, ItemCatalog, ItemInstance, ItemLocation,
};
use mareforge_domain_ships::VesselPresence;
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
/// `FileStateStore` persiste este contrato em arquivo; `PostgresStateStore`
/// persiste o mesmo estado nas tabelas do ADR-0004 (persist.rs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSnapshot {
    /// Token de identidade persistente → CharacterId (MF-035): o dono é o
    /// personagem; a conexão/client_num é só transporte da sessão.
    pub identities: HashMap<String, CharacterId>,
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
    identities: HashMap<String, CharacterId>,
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
    /// Âncora de sobrevivência (MF-033/034). `Some(File)` = salvamento
    /// periódico; `Some(Postgres)` = persistência por operação crítica;
    /// `None` = dev puro, mundo descartável.
    store: Option<std::sync::Arc<dyn crate::persist::StateStore>>,
}

impl Default for ServerMarket {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerMarket {
    pub fn new() -> Self {
        Self::with_store(None)
    }

    pub fn with_store(store: Option<std::sync::Arc<dyn crate::persist::StateStore>>) -> Self {
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
            store,
        }
    }

    /// Identidade persistente do jogador (MF-035), cunhando o bootstrap dev
    /// no primeiro toque (§48). O token vem do client e é o ÚNICO vínculo
    /// duradouro — conexão nenhuma é dona de nada.
    pub fn character(&mut self, identity_token: &str) -> CharacterId {
        if let Some(id) = self.identities.get(identity_token) {
            return *id;
        }
        let id = CharacterId::new();
        self.identities.insert(identity_token.to_string(), id);
        self.balances.insert(id, Money(DEV_SEED_GOLD));
        self.ledger.record(
            LedgerKind::Mint,
            Money(DEV_SEED_GOLD),
            "bootstrap dev (§48)",
        );
        info!(character = ?id, gold = DEV_SEED_GOLD, "carteira semeada (dev)");
        self.persist();
        id
    }

    pub fn balance(&self, character: CharacterId) -> Money {
        self.balances.get(&character).copied().unwrap_or(Money(0))
    }

    /// Persiste o estado no store ativo (MF-034: operação crítica chega
    /// atomicamente ao banco; arquivo de dev salva no ritmo periódico).
    /// Chamado ao fim de toda mutação econômica.
    fn persist(&self) {
        if let Some(store) = &self.store {
            let snapshot = self.snapshot();
            if let Err(error) = store.save_market(&snapshot) {
                warn!(error = %error, "falha ao persistir estado econômico");
            }
        }
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

    /// Reconstrói o estado de um snapshot (boot com store). O store volta
    /// como âncora da sessão — persistências seguintes continuam por ele.
    pub fn restore_with_store(
        snapshot: MarketSnapshot,
        store: Option<std::sync::Arc<dyn crate::persist::StateStore>>,
    ) -> Self {
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
            store,
        }
    }

    /// Restore sem store (testes e dev puro).
    pub fn restore(snapshot: MarketSnapshot) -> Self {
        Self::restore_with_store(snapshot, None)
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
        self.persist();
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
        if withdrawn > 0 {
            self.persist();
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
        let now = Utc::now();
        self.board.push(MarketOrder {
            id: order_id,
            seller: character,
            item,
            quantity,
            unit_price,
            region,
            status: OrderStatus::Open,
            created_at: now,
            expires_at: now
                + chrono::Duration::seconds(self.policy.default_order_duration_secs as i64),
            filled_quantity: 0,
        });
        self.persist();
        Ok((order_num, fee))
    }

    /// Quantidade de `item` no storage (personagem, região) — para validar
    /// receitas de oficina contra a riqueza guardada (MF-037).
    pub fn storage_quantity(
        &self,
        character: CharacterId,
        region: RegionId,
        item: ItemDefinitionId,
    ) -> u32 {
        self.storage
            .get(&(character, region))
            .map(|storage| storage_quantity(storage, item))
            .unwrap_or(0)
    }

    /// Consome `quantity` de `item` do storage (insumos de oficina, MF-037).
    /// Fail-closed: sem estoque suficiente é erro e nada se move.
    pub fn consume_from_storage(
        &mut self,
        character: CharacterId,
        region: RegionId,
        item: ItemDefinitionId,
        quantity: u32,
    ) -> Result<(), MarketError> {
        let Some(storage) = self.storage.get_mut(&(character, region)) else {
            return Err(MarketError::NotInStorage);
        };
        if storage_quantity(storage, item) < quantity {
            return Err(MarketError::NotInStorage);
        }
        // Consumido: regrava a localização e descarta as pilhas retiradas.
        let _consumed =
            take_from_storage(storage, item, quantity, ItemLocation::PortStorage(region));
        self.persist();
        Ok(())
    }

    /// Oficina do porto (MF-037): executa a receita sobre o storage regional
    /// — insumos saem do storage, output volta para o storage. O porão não
    /// é insumo automático: quem embarca decide o que embarca (Pilar 2).
    pub fn craft_at_storage(
        &mut self,
        character: CharacterId,
        region: RegionId,
        recipe: &Recipe,
        catalog: &ItemCatalog,
        station: StationKind,
    ) -> Result<ItemInstance, CraftError> {
        let Some(storage) = self.storage.get_mut(&(character, region)) else {
            return Err(CraftError::EmptyStorage);
        };
        let output = craft_in_storage(recipe, storage, catalog, station, region)?;
        self.persist();
        Ok(output)
    }

    /// Retira UMA unidade do item do storage (equipar, MF-039). Fail-closed.
    pub fn take_one_from_storage(
        &mut self,
        character: CharacterId,
        region: RegionId,
        item: ItemDefinitionId,
    ) -> Result<Custody, MarketError> {
        let storage = self
            .storage
            .get_mut(&(character, region))
            .ok_or(MarketError::NotInStorage)?;
        let mut taken = take_from_storage(storage, item, 1, ItemLocation::PortStorage(region));
        match taken.pop() {
            Some(custody) => {
                self.persist();
                Ok(custody)
            }
            None => Err(MarketError::NotInStorage),
        }
    }

    /// Devolve uma custódia ao storage (swap de loadout, MF-039). Storage
    /// não tem limite: nunca falha, nunca destrói.
    pub fn return_to_storage(
        &mut self,
        character: CharacterId,
        region: RegionId,
        custody: Custody,
        catalog: &ItemCatalog,
    ) {
        let max_stack = catalog
            .get(custody.instance.definition)
            .map(|definition| definition.max_stack)
            .unwrap_or(1);
        let storage = self.storage.entry((character, region)).or_default();
        put_stack(
            storage,
            custody.with_location(ItemLocation::PortStorage(region)),
            max_stack,
        );
        self.persist();
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
        if self.board[position].status != OrderStatus::Open
            && self.board[position].status != OrderStatus::Partial
        {
            return Err(MarketError::OrderNotOpen);
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
        self.persist();
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
        .inspect(|_| self.persist())
    }

    /// Expira orders vencidas (MF-041). Escrow volta ao storage do seller na
    /// região da order; o listing fee já queimou na criação e não reembolsa.
    pub fn expire_orders(&mut self, now: DateTime<Utc>) -> usize {
        // ponytail: varredura global; agendamento por região se o board crescer.
        let mut expired = Vec::new();
        for (index, order) in self.board.iter().enumerate() {
            if order.expires_at >= now
                || !matches!(order.status, OrderStatus::Open | OrderStatus::Partial)
            {
                continue;
            }
            let order_num = self
                .order_nums
                .iter()
                .find(|(_, id)| **id == order.id)
                .map(|(num, _)| *num)
                .unwrap_or(0);
            if let Some(escrowed) = self.escrow.remove(&order_num) {
                self.storage
                    .entry((order.seller, order.region))
                    .or_default()
                    .extend(escrowed.into_iter().map(|custody| {
                        custody.with_location(ItemLocation::PortStorage(order.region))
                    }));
            }
            expired.push(index);
        }
        if expired.is_empty() {
            return 0;
        }
        let expired_count = expired.len();
        for index in expired {
            self.board[index].status = OrderStatus::Expired;
        }
        self.persist();
        expired_count
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
        let Some(mut ship) = ships
            .iter_mut()
            .find(|ship| ship.client_id == Some(client_id))
        else {
            continue;
        };
        // MF-036: serviço de porto exige ATRACADO — água protegida não basta.
        let VesselPresence::Docked(region_id) = ship.presence else {
            info!(
                ship_id = ship.ship_id,
                "depósito recusado: atraca primeiro (E)"
            );
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "atraca primeiro (E)",
            );
            continue;
        };
        let name = region_name(&map.0, region_id);
        let character = ship.character;
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
        let Some(mut ship) = ships
            .iter_mut()
            .find(|ship| ship.client_id == Some(client_id))
        else {
            continue;
        };
        let VesselPresence::Docked(region_id) = ship.presence else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "atraca primeiro (E)",
            );
            continue;
        };
        let name = region_name(&map.0, region_id);
        let character = ship.character;
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
        let Some(ship) = ships.iter().find(|ship| ship.client_id == Some(client_id)) else {
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
        let character = ship.character;
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
                let viewers = viewers_of(&ships);
                broadcast_orders(
                    &mut connection_manager,
                    &market,
                    &dev.catalog,
                    &map.0,
                    &viewers,
                );
                send_wallet(&mut connection_manager, &market, &viewers, character);
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
        let Some(ship) = ships.iter().find(|ship| ship.client_id == Some(client_id)) else {
            continue;
        };
        let VesselPresence::Docked(buyer_region_id) = ship.presence else {
            info!(
                ship_id = ship.ship_id,
                "compra recusada: atraca primeiro (E) — mercado é serviço de porto"
            );
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "atraca primeiro (E)",
            );
            continue;
        };
        let buyer_region_name = region_name(&map.0, buyer_region_id);
        let buyer = ship.character;
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
                let viewers = viewers_of(&ships);
                broadcast_orders(
                    &mut connection_manager,
                    &market,
                    &dev.catalog,
                    &map.0,
                    &viewers,
                );
                send_wallet(&mut connection_manager, &market, &viewers, buyer);
                send_wallet(&mut connection_manager, &market, &viewers, receipt.seller);
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
        let Some(ship) = ships.iter().find(|ship| ship.client_id == Some(client_id)) else {
            continue;
        };
        let character = ship.character;
        match market.cancel_order(character, order_num) {
            Ok(()) => {
                info!(
                    order_num,
                    "order cancelada; item devolvido ao storage (fee não reembolsada, §46)"
                );
                let viewers = viewers_of(&ships);
                broadcast_orders(
                    &mut connection_manager,
                    &market,
                    &dev.catalog,
                    &map.0,
                    &viewers,
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

/// Snapshot de orders personalizado por observador (o campo `mine` é do
/// dono — MF-035: personagem vs. vendedor). `viewers` são os pares
/// (sessão, personagem) online — o mercado não conhece o ECS. Enviado no
/// hello e a cada mudança no board.
pub fn broadcast_orders(
    connection_manager: &mut ConnectionManager,
    market: &ServerMarket,
    catalog: &ItemCatalog,
    map: &WorldMap,
    viewers: &[(Option<ClientId>, CharacterId)],
) {
    for (client_id, character) in viewers {
        if let Some(client_id) = client_id {
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                *client_id,
                &OrdersSnapshot {
                    orders: order_lines_for(market, catalog, map, *character),
                },
            );
        }
    }
}

/// Linhas ativas do board para um observador (MF-041: Expired fica no estado
/// do servidor para auditoria, mas nunca vai ao client).
fn order_lines_for(
    market: &ServerMarket,
    catalog: &ItemCatalog,
    map: &WorldMap,
    character: CharacterId,
) -> Vec<OrderLine> {
    market
        .board
        .iter()
        .filter(|order| matches!(order.status, OrderStatus::Open | OrderStatus::Partial))
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
                mine: character == order.seller,
            })
        })
        .collect()
}

/// Envia a carteira atualizada ao dono, se ele estiver online (§31: ouro é
/// da personagem, não do navio — a carteira sobrevive ao respawn).
fn send_wallet(
    connection_manager: &mut ConnectionManager,
    market: &ServerMarket,
    viewers: &[(Option<ClientId>, CharacterId)],
    character: CharacterId,
) {
    for (client_id, owner) in viewers {
        if *owner == character {
            if let Some(client_id) = client_id {
                let _ = connection_manager.send_message::<ReliableChannel, _>(
                    *client_id,
                    &WalletUpdated {
                        gold: market.balance(character).0,
                    },
                );
            }
        }
    }
}

/// Pares (sessão, personagem) dos navios — bridge ECS → funções puras.
pub fn viewers_of(ships: &Query<&ServerShip>) -> Vec<(Option<ClientId>, CharacterId)> {
    ships
        .iter()
        .map(|ship| (ship.client_id, ship.character))
        .collect()
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

/// Carrega o estado econômico do store ativo no boot (MF-027/033). Sem
/// store configurado, o mundo nasce limpo — comportamento de dev puro.
pub fn load_state(store: Res<crate::persist::StoreHandle>, mut market: ResMut<ServerMarket>) {
    let Some(store) = store.0.clone() else {
        return;
    };
    match store.load_market() {
        Ok(Some(snapshot)) => {
            *market = ServerMarket::restore_with_store(snapshot, Some(store));
            info!("estado econômico restaurado do store");
        }
        Ok(None) => info!("mundo econômico novo (store vazio)"),
        Err(error) => warn!(error = %error, "store ilegível; começando limpo"),
    }
}

/// Salvamento periódico (a cada 10s e no AppExit) — só para stores que
/// declaram esse modo (arquivo de dev). Postgres persiste por operação
/// crítica (MF-034); dev puro não persiste.
pub fn save_state(
    store: Res<crate::persist::StoreHandle>,
    market: Res<ServerMarket>,
    mut exit: EventReader<AppExit>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    const SAVE_INTERVAL: f32 = 10.0;
    let Some(store) = store.0.clone() else {
        return;
    };
    if !store.periodic_saving() {
        return;
    }
    *timer += time.delta_secs();
    let exiting = exit.read().next().is_some();
    if *timer < SAVE_INTERVAL && !exiting {
        return;
    }
    *timer = 0.0;
    match store.save_market(&market.snapshot()) {
        Ok(()) => info!("estado econômico persistido"),
        Err(error) => warn!(error = %error, "falha ao persistir estado econômico"),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use mareforge_domain_items::{CargoHold, ItemDefinition, ItemInstance, ItemKind};
    use mareforge_shared::ids::{ItemInstanceId, ShipInstanceId};

    use super::*;

    fn catalog_with_item() -> (ItemCatalog, ItemDefinitionId) {
        let item = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        catalog
            .register(ItemDefinition {
                id: item,
                kind: ItemKind::Resource,
                equipment: None,
                max_stack: 100,
                base_weight: 1,
                tags: Default::default(),
                display_name: String::from("Madeira"),
            })
            .expect("catálogo de teste não registra duplicatas");
        (catalog, item)
    }

    fn put_in_storage(
        market: &mut ServerMarket,
        character: CharacterId,
        region: RegionId,
        item: ItemDefinitionId,
        catalog: &ItemCatalog,
        quantity: u32,
    ) {
        let mut hold = CargoHold::new(ShipInstanceId::new(), 1_000);
        hold.insert(
            catalog,
            ItemInstance::new_resource(ItemInstanceId::new(), item, quantity),
        )
        .expect("teste cabe no porão");
        market
            .deposit_all(character, region, &mut hold, catalog)
            .expect("teste deposita no storage");
    }

    #[test]
    fn create_order_sets_expiry_from_policy_duration() {
        let mut market = ServerMarket::new();
        let character = market.character("seller");
        let region = RegionId::new();
        let (catalog, item) = catalog_with_item();
        put_in_storage(&mut market, character, region, item, &catalog, 10);
        market.policy.default_order_duration_secs = 123;

        let (order_num, _) = market
            .create_order(character, region, item, 10, Money(5))
            .expect("storage tem estoque");
        let snapshot = market.snapshot();
        let order_id = snapshot.order_nums[&order_num];
        let order = snapshot
            .board
            .iter()
            .find(|order| order.id == order_id)
            .expect("order criada");

        assert_eq!(order.expires_at - order.created_at, Duration::seconds(123));
    }

    #[test]
    fn expire_orders_flips_status_and_returns_escrow() {
        let mut market = ServerMarket::new();
        let character = market.character("seller");
        let region = RegionId::new();
        let (catalog, item) = catalog_with_item();
        put_in_storage(&mut market, character, region, item, &catalog, 10);
        let (order_num, _) = market
            .create_order(character, region, item, 10, Money(5))
            .expect("storage tem estoque");
        let order = market.snapshot().board[0].clone();

        assert_eq!(
            market.expire_orders(order.expires_at + Duration::seconds(1)),
            1
        );
        let snapshot = market.snapshot();
        let expired = snapshot
            .board
            .iter()
            .find(|order| order.id == snapshot.order_nums[&order_num])
            .expect("order continua no board para auditoria");
        assert_eq!(expired.status, OrderStatus::Expired);
        assert!(snapshot.escrow.is_empty());
        assert_eq!(market.storage_quantity(character, region, item), 10);
    }

    #[test]
    fn expire_does_not_refund_listing_fee() {
        let mut market = ServerMarket::new();
        let character = market.character("seller");
        let region = RegionId::new();
        let (catalog, item) = catalog_with_item();
        put_in_storage(&mut market, character, region, item, &catalog, 10);
        let balance_before = market.balance(character);
        let burns_before = market.ledger.burned();
        let entries_before = market.ledger.entries().len();

        let (order_num, _) = market
            .create_order(character, region, item, 10, Money(5))
            .expect("storage tem estoque");
        let balance_after_create = market.balance(character);
        let burns_after_create = market.ledger.burned();
        let order = market.snapshot().board[0].clone();
        market.expire_orders(order.expires_at + Duration::seconds(1));

        assert!(
            balance_after_create.0 < balance_before.0,
            "fee queimou ouro"
        );
        assert!(
            burns_after_create.0 > burns_before.0,
            "burn registrado no ledger"
        );
        assert_eq!(market.balance(character), balance_after_create);
        assert_eq!(market.ledger.burned(), burns_after_create);
        assert_eq!(market.ledger.entries().len(), entries_before + 1);
        assert_eq!(market.snapshot().order_nums[&order_num], order.id);
    }

    #[test]
    fn expired_order_cannot_be_cancelled_or_bought() {
        let mut market = ServerMarket::new();
        let character = market.character("seller");
        let region = RegionId::new();
        let (catalog, item) = catalog_with_item();
        put_in_storage(&mut market, character, region, item, &catalog, 10);
        let (order_num, _) = market
            .create_order(character, region, item, 10, Money(5))
            .expect("storage tem estoque");
        let order = market.snapshot().board[0].clone();
        market.expire_orders(order.expires_at + Duration::seconds(1));

        assert_eq!(
            market.cancel_order(character, order_num),
            Err(MarketError::OrderNotOpen)
        );
        assert_eq!(
            market.buy(character, region, order_num, 1),
            Err(MarketError::OrderNotOpen)
        );
    }

    #[test]
    fn order_lines_exclude_expired_orders() {
        let mut market = ServerMarket::new();
        let character = market.character("seller");
        let region = RegionId::new();
        let (catalog, item) = catalog_with_item();
        put_in_storage(&mut market, character, region, item, &catalog, 20);
        market.policy.default_order_duration_secs = 60;
        let (open_num, _) = market
            .create_order(character, region, item, 10, Money(5))
            .expect("primeira order");
        market.policy.default_order_duration_secs = 0;
        let (expired_num, _) = market
            .create_order(character, region, item, 10, Money(5))
            .expect("segunda order");
        let expired = market.snapshot().board[1].clone();
        market.expire_orders(expired.expires_at + Duration::seconds(1));
        let map = WorldMap::vertical_slice();

        let lines = order_lines_for(&market, &catalog, &map, character);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].order_num, open_num);
        assert!(lines.iter().all(|line| line.order_num != expired_num));
    }
}
