//! Guilda Mercante (NPC compradora) e Quadro de Contratos por porto. As
//! regras vivem em `domain-economy` (guild.rs, contract.rs); aqui é casca:
//! valida atracado, move item/ouro pelo `ServerMarket` (mesma carteira e
//! ledger do mercado, logo persistido igual) e empurra snapshots ao client.
//!
//! ponytail: saturação da guilda e contratos vivem só em memória — restart
//! zera preços e contratos ativos; persistir quando virar reclamação.

use std::collections::{HashMap, HashSet};

use bevy::ecs::prelude::*;
use bevy::time::Time;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use marvyr_domain_economy::contract::OFFERS_PER_PORT;
use marvyr_domain_economy::guild::GUILD_BASE_VALUES;
use marvyr_domain_economy::{
    generate_offers, ActiveContract, Contract, ContractKind, GuildBook, LedgerKind, Money, PortSite,
};
use marvyr_domain_items::ItemCatalog;
use marvyr_domain_ships::VesselPresence;
use marvyr_domain_world::WorldMap;
use marvyr_protocol::{
    AbandonContract, AcceptContract, ContractLine, ContractResult, ContractsSnapshot,
    GuildPriceLine, GuildPrices, PortStorageSnapshot, SellToGuild,
};
use marvyr_shared::ids::{CharacterId, ItemDefinitionId, ItemInstanceId, RegionId};
use tracing::info;

use crate::market::{market_result, region_name, send_wallet, ServerMarket};
use crate::net::{port_storage_snapshot, DevItems, ReliableChannel, ServerShip, ServerWorldMap};
use crate::sets::SimulationSet;

/// Intervalo de renovação do Quadro de Contratos.
const BOARD_REFRESH_SECS: f64 = 300.0;

pub struct GuildPlugin;

impl bevy::app::Plugin for GuildPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.init_resource::<ServerGuild>();
        app.add_systems(
            bevy::app::FixedUpdate,
            (handle_sell_to_guild, handle_contract_intents).in_set(SimulationSet::Input),
        );
        app.add_systems(
            bevy::app::FixedUpdate,
            tick_contracts.in_set(SimulationSet::EconomyConsequences),
        );
        app.add_systems(
            bevy::app::FixedUpdate,
            push_guild_state.in_set(SimulationSet::Snapshot),
        );
    }
}

#[derive(Resource, Default)]
pub struct ServerGuild {
    pub book: GuildBook,
    boards: HashMap<RegionId, Vec<Contract>>,
    active: HashMap<CharacterId, ActiveContract>,
    /// Quadro de onde saiu o contrato ativo: abandonar devolve a oferta, senão
    /// aceitar+abandonar em loop esvazia o quadro de todo mundo.
    accepted_at: HashMap<CharacterId, RegionId>,
    next_refresh_secs: f64,
    seed: u64,
    next_id: u32,
}

/// Portos do mapa como (região, sítio) — nome da região = nome do porto.
/// Posição na carta de zonas (MV-066): a distância do contrato é a de
/// viagem, não a do plano onde as zonas moram longe umas das outras.
fn port_sites(map: &WorldMap) -> Vec<(RegionId, PortSite<'static>)> {
    map.regions()
        .iter()
        .filter_map(|region| {
            let port = region.port.as_ref()?;
            let (x, y) = map.chart_position(port.x, port.y);
            Some((
                region.id,
                PortSite {
                    name: region.name,
                    x,
                    y,
                },
            ))
        })
        .collect()
}

impl ServerGuild {
    /// Renova as 3 ofertas de cada porto (as não aceitas somem).
    fn refresh_boards(&mut self, map: &WorldMap) {
        if self.seed == 0 {
            self.seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos() as u64)
                .unwrap_or(1)
                | 1;
        }
        let ports = port_sites(map);
        let sites: Vec<PortSite> = ports.iter().map(|(_, site)| *site).collect();
        for (region, site) in &ports {
            self.seed = self
                .seed
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(1);
            let offers = generate_offers(site, &sites, self.seed, self.next_id);
            self.next_id += offers.len() as u32;
            self.boards.insert(*region, offers);
        }
    }
}

fn contract_line(contract: &Contract, active: Option<(&ActiveContract, f64)>) -> ContractLine {
    ContractLine {
        id: contract.id,
        title: contract.title(),
        reward: contract.reward,
        duration_secs: contract.duration_secs as u32,
        remaining_secs: active
            .map(|(active, now)| active.remaining_secs(now).ceil() as u32)
            .unwrap_or(0),
        progress: active.map(|(active, _)| active.progress()).unwrap_or(0),
        target: contract.target(),
        hunt: matches!(contract.kind, ContractKind::Hunt { .. }),
    }
}

fn catalog_id(catalog: &ItemCatalog, name: &str) -> Option<ItemDefinitionId> {
    catalog
        .items()
        .find(|definition| definition.display_name == name)
        .map(|definition| definition.id)
}

fn send_contract_result(
    connection_manager: &mut ConnectionManager,
    client_id: Option<ClientId>,
    success: bool,
    reason: String,
) {
    if let Some(client_id) = client_id {
        let _ = connection_manager
            .send_message::<ReliableChannel, _>(client_id, &ContractResult { success, reason });
    }
}

/// Vende do storage do porto atracado para a guilda. Fail-closed: item fora
/// da tabela ou sem estoque é recusado e nada se move.
#[allow(clippy::too_many_arguments)]
fn handle_sell_to_guild(
    mut events: EventReader<ServerReceiveMessage<SellToGuild>>,
    mut connection_manager: ResMut<ConnectionManager>,
    mut market: ResMut<ServerMarket>,
    mut guild: ResMut<ServerGuild>,
    dev: Res<DevItems>,
    map: Res<ServerWorldMap>,
    time: Res<Time>,
    ships: Query<&ServerShip>,
) {
    let now = time.elapsed_secs_f64();
    for event in events.read() {
        let client_id = event.from();
        let message = event.message();
        let Some(ship) = ships.iter().find(|ship| ship.client_id == Some(client_id)) else {
            continue;
        };
        let VesselPresence::Docked(region) = ship.presence else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "atraca primeiro (E)",
            );
            continue;
        };
        let Some(name) = dev
            .catalog
            .get(message.item)
            .map(|definition| definition.display_name.clone())
        else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "item desconhecido",
            );
            continue;
        };
        let port = region_name(&map.0, region);
        let available = market.storage_quantity(ship.character, region, message.item);
        let quantity = message.quantity.min(available);
        if quantity == 0 {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                "nada disso no armazem",
            );
            continue;
        }
        let Some(total) = guild.book.quote(port, &name, quantity, now) else {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                &format!("a guilda nao compra {name}"),
            );
            continue;
        };
        let memo = format!("guild purchase {quantity}x {name} @ {port}");
        if let Err(error) =
            market.sell_to_guild(ship.character, region, message.item, quantity, total, memo)
        {
            market_result(
                &mut connection_manager,
                client_id,
                false,
                &error.to_string(),
            );
            continue;
        }
        guild.book.record_sale(port, &name, quantity, now);
        info!(port, item = %name, quantity, gold = total.0, "guilda comprou; item destruído");
        let viewers = crate::market::viewers_of(&ships);
        send_wallet(&mut connection_manager, &market, &viewers, ship.character);
        market_result(
            &mut connection_manager,
            client_id,
            true,
            &format!("Guilda comprou {quantity}x {name} por {}g", total.0),
        );
    }
}

/// Aceitar (só atracado, 1 por vez) e abandonar (em qualquer lugar).
fn handle_contract_intents(
    mut accepts: EventReader<ServerReceiveMessage<AcceptContract>>,
    mut abandons: EventReader<ServerReceiveMessage<AbandonContract>>,
    mut connection_manager: ResMut<ConnectionManager>,
    mut guild: ResMut<ServerGuild>,
    dev: Res<DevItems>,
    time: Res<Time>,
    ships: Query<&ServerShip>,
) {
    let now = time.elapsed_secs_f64();
    for event in accepts.read() {
        let client_id = Some(event.from());
        let Some(ship) = ships.iter().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        let VesselPresence::Docked(region) = ship.presence else {
            send_contract_result(
                &mut connection_manager,
                client_id,
                false,
                "atraca primeiro (E)".into(),
            );
            continue;
        };
        if guild.active.contains_key(&ship.character) {
            send_contract_result(
                &mut connection_manager,
                client_id,
                false,
                "ja existe um contrato ativo".into(),
            );
            continue;
        }
        let id = event.message().id;
        let offer = guild
            .boards
            .get(&region)
            .and_then(|board| board.iter().position(|contract| contract.id == id));
        let Some(index) = offer else {
            send_contract_result(
                &mut connection_manager,
                client_id,
                false,
                "contrato indisponivel".into(),
            );
            continue;
        };
        let contract = guild.boards[&region][index].clone();
        let hold = match &contract.kind {
            ContractKind::Delivery { item, .. } => hold_stacks(ship, &dev.catalog, item),
            ContractKind::Hunt { .. } => Vec::new(),
        };
        let Some(active) = ActiveContract::accept(contract, now, &hold) else {
            send_contract_result(
                &mut connection_manager,
                client_id,
                false,
                "carregue a carga no porao antes de aceitar".into(),
            );
            continue;
        };
        if let Some(board) = guild.boards.get_mut(&region) {
            board.remove(index);
        }
        let reason = format!("aceito: {}", active.contract.title());
        guild.active.insert(ship.character, active);
        guild.accepted_at.insert(ship.character, region);
        send_contract_result(&mut connection_manager, client_id, true, reason);
    }
    for event in abandons.read() {
        let client_id = Some(event.from());
        let Some(ship) = ships.iter().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        let abandoned = guild.active.remove(&ship.character);
        let origin = guild.accepted_at.remove(&ship.character);
        if let (Some(active), Some(region)) = (&abandoned, origin) {
            // Oferta volta ao quadro, sem passar do tamanho de um quadro novo.
            if let Some(board) = guild.boards.get_mut(&region) {
                if board.len() < OFFERS_PER_PORT {
                    board.push(active.contract.clone());
                }
            }
        }
        let abandoned = abandoned.is_some();
        let reason = if abandoned {
            "contrato abandonado"
        } else {
            "nenhum contrato ativo"
        };
        send_contract_result(&mut connection_manager, client_id, abandoned, reason.into());
    }
}

/// Pilhas de `item` no porão do navio: (instância, quantidade).
fn hold_stacks(ship: &ServerShip, catalog: &ItemCatalog, item: &str) -> Vec<(ItemInstanceId, u32)> {
    let Some(item_id) = catalog_id(catalog, item) else {
        return Vec::new();
    };
    ship.hold
        .items()
        .iter()
        .filter(|custody| custody.instance.definition == item_id)
        .map(|custody| (custody.instance.id, custody.instance.quantity))
        .collect()
}

/// Paga a recompensa (faucet auditado, mesma carteira/ledger do mercado).
fn pay_contract(market: &mut ServerMarket, character: CharacterId, contract: &Contract) {
    market.credit(character, Money(contract.reward));
    market.ledger.record(
        LedgerKind::ContractReward,
        Money(contract.reward),
        format!("contract {} reward", contract.id),
    );
    market.persist();
}

/// Renova quadros, conta abates de Caça, expira prazos e conclui entregas
/// de quem está atracado no destino com a carga no porão.
fn tick_contracts(
    mut connection_manager: ResMut<ConnectionManager>,
    mut market: ResMut<ServerMarket>,
    mut guild: ResMut<ServerGuild>,
    dev: Res<DevItems>,
    map: Res<ServerWorldMap>,
    time: Res<Time>,
    mut ships: Query<&mut ServerShip>,
) {
    let now = time.elapsed_secs_f64();
    if now >= guild.next_refresh_secs {
        guild.refresh_boards(&map.0);
        guild.next_refresh_secs = now + BOARD_REFRESH_SECS;
    }
    let viewers: Vec<(Option<ClientId>, CharacterId)> = ships
        .iter()
        .map(|ship| (ship.client_id, ship.character))
        .collect();
    let client_of = |character: CharacterId| {
        viewers
            .iter()
            .find(|(_, owner)| *owner == character)
            .and_then(|(client_id, _)| *client_id)
    };
    let mut completed: Vec<(CharacterId, Contract)> = Vec::new();

    for killer in std::mem::take(&mut market.npc_kills) {
        if let Some(active) = guild.active.get_mut(&killer) {
            if !active.expired(now) && active.record_kill() {
                let active = guild.active.remove(&killer).expect("checado acima");
                guild.accepted_at.remove(&killer);
                completed.push((killer, active.contract));
            }
        }
    }

    let expired: Vec<CharacterId> = guild
        .active
        .iter()
        .filter(|(_, active)| active.expired(now))
        .map(|(character, _)| *character)
        .collect();
    for character in expired {
        guild.active.remove(&character);
        guild.accepted_at.remove(&character);
        send_contract_result(
            &mut connection_manager,
            client_of(character),
            false,
            "contrato expirou".into(),
        );
    }

    for mut ship in &mut ships {
        // Todo tick, em qualquer lugar: a consignação só pode encolher.
        if let Some(active) = guild.active.get_mut(&ship.character) {
            if let ContractKind::Delivery { item, .. } = &active.contract.kind {
                let hold = hold_stacks(&ship, &dev.catalog, item);
                active.observe_hold(&hold);
            }
        }
        let VesselPresence::Docked(region) = ship.presence else {
            continue;
        };
        let Some(active) = guild.active.get(&ship.character) else {
            continue;
        };
        let ContractKind::Delivery { item, quantity, .. } = &active.contract.kind else {
            continue;
        };
        let Some(item_id) = catalog_id(&dev.catalog, item) else {
            continue;
        };
        let hold = hold_stacks(&ship, &dev.catalog, item);
        if !active.delivery_ready(region_name(&map.0, region), &hold) {
            continue;
        }
        let quantity = *quantity;
        // Entregue: a carga some do porão (sink) e o contrato paga.
        if ship.hold.remove(item_id, quantity).is_ok() {
            let active = guild.active.remove(&ship.character).expect("checado acima");
            guild.accepted_at.remove(&ship.character);
            completed.push((ship.character, active.contract));
        }
    }

    for (character, contract) in completed {
        pay_contract(&mut market, character, &contract);
        info!(
            contract = contract.id,
            reward = contract.reward,
            "contrato concluído"
        );
        send_wallet(&mut connection_manager, &market, &viewers, character);
        send_contract_result(
            &mut connection_manager,
            client_of(character),
            true,
            format!("contrato concluido: +{}g", contract.reward),
        );
    }
}

/// Preços da guilda em todos os portos + carga do porão (só leitura).
fn guild_prices(
    guild: &ServerGuild,
    catalog: &ItemCatalog,
    map: &WorldMap,
    ship: &ServerShip,
    now: f64,
) -> GuildPrices {
    let ports = port_sites(map);
    GuildPrices {
        ports: ports.iter().map(|(_, site)| site.name.to_owned()).collect(),
        lines: GUILD_BASE_VALUES
            .iter()
            .filter_map(|(name, _)| {
                Some(GuildPriceLine {
                    item: catalog_id(catalog, name)?,
                    item_name: (*name).to_owned(),
                    prices: ports
                        .iter()
                        .map(|(_, site)| {
                            guild
                                .book
                                .unit_price(site.name, name, now)
                                .map(|price| price.0)
                                .unwrap_or(0)
                        })
                        .collect(),
                })
            })
            .collect(),
        cargo: port_storage_snapshot(catalog, "", ship.hold.items()).lines,
    }
}

fn contracts_snapshot(guild: &ServerGuild, ship: &ServerShip, now: f64) -> ContractsSnapshot {
    let offers = match ship.presence {
        VesselPresence::Docked(region) => guild
            .boards
            .get(&region)
            .map(|board| board.iter().map(|c| contract_line(c, None)).collect())
            .unwrap_or_default(),
        VesselPresence::AtSea => Vec::new(),
    };
    ContractsSnapshot {
        offers,
        active: guild
            .active
            .get(&ship.character)
            .map(|active| contract_line(&active.contract, Some((active, now)))),
    }
}

#[derive(Default)]
struct SentState {
    prices: Option<GuildPrices>,
    storage: Option<PortStorageSnapshot>,
    /// Snapshot com `remaining_secs` zerado: o client faz a contagem.
    contracts: Option<ContractsSnapshot>,
}

/// Envia a cada client só o que mudou: preços (mudança de 1g já conta),
/// storage do porto (depósito, craft, venda) e contratos.
#[allow(clippy::too_many_arguments)]
fn push_guild_state(
    mut connection_manager: ResMut<ConnectionManager>,
    market: Res<ServerMarket>,
    guild: Res<ServerGuild>,
    dev: Res<DevItems>,
    map: Res<ServerWorldMap>,
    time: Res<Time>,
    ships: Query<&ServerShip>,
    mut sent: Local<HashMap<ClientId, SentState>>,
) {
    let now = time.elapsed_secs_f64();
    let online: HashSet<ClientId> = ships.iter().filter_map(|ship| ship.client_id).collect();
    sent.retain(|client_id, _| online.contains(client_id));
    for ship in &ships {
        let Some(client_id) = ship.client_id else {
            continue;
        };
        let state = sent.entry(client_id).or_default();

        let contracts = contracts_snapshot(&guild, ship, now);
        let mut key = contracts.clone();
        if let Some(active) = key.active.as_mut() {
            active.remaining_secs = 0;
        }
        if state.contracts.as_ref() != Some(&key) {
            let _ = connection_manager.send_message::<ReliableChannel, _>(client_id, &contracts);
            state.contracts = Some(key);
        }

        let VesselPresence::Docked(region) = ship.presence else {
            state.prices = None;
            state.storage = None;
            continue;
        };
        let prices = guild_prices(&guild, &dev.catalog, &map.0, ship, now);
        if state.prices.as_ref() != Some(&prices) {
            let _ = connection_manager.send_message::<ReliableChannel, _>(client_id, &prices);
            state.prices = Some(prices);
        }
        let storage = port_storage_snapshot(
            &dev.catalog,
            region_name(&map.0, region),
            market
                .port_storage(ship.character, region)
                .unwrap_or_default(),
        );
        if state.storage.as_ref() != Some(&storage) {
            let _ = connection_manager.send_message::<ReliableChannel, _>(client_id, &storage);
            state.storage = Some(storage);
        }
    }
}
