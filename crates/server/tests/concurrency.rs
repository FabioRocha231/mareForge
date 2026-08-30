//! Testes de concorrência (PRD §70, MF-028): duplo clique, retry, duplo
//! loot, duplo craft e disconnect. O servidor é autoritativo e
//! single-threaded por tick — a ameaça real é a INTENÇÃO DUPLICADA chegando
//! no mesmo tick. Cada teste prova que apenas uma operação vence.

use chrono::Utc;
use mareforge_domain_combat::{resolve_ship_destruction, LootPolicy, SurvivorItem, WreckChest};
use mareforge_domain_crafting::{craft, Ingredient, Recipe, StationKind};
use mareforge_domain_economy::{LedgerKind, MarketError, Money};

// Money não implementa Add/Sub: comparação via tuple interno.
use mareforge_domain_items::{CargoHold, ItemCatalog, ItemDefinition, ItemInstance, ItemKind};
use mareforge_server::market::ServerMarket;
use mareforge_shared::ids::{
    CharacterId, DestructionEventId, ItemDefinitionId, ItemInstanceId, RecipeId, RegionId,
    ShipInstanceId, WreckId,
};

fn wood_definition() -> (ItemCatalog, ItemDefinitionId) {
    let id = ItemDefinitionId::new();
    let mut catalog = ItemCatalog::default();
    catalog
        .register(ItemDefinition {
            id,
            kind: ItemKind::Resource,
            equipment: None,
            max_stack: 100,
            base_weight: 2,
            tags: Default::default(),
            display_name: String::from("Madeira"),
        })
        .unwrap();
    (catalog, id)
}

/// Mundo mínimo: catálogo + dois personagens com 20 de madeira depositada.
struct World {
    market: ServerMarket,
    catalog: ItemCatalog,
    wood: ItemDefinitionId,
    region: RegionId,
    a: CharacterId,
    b: CharacterId,
}

impl World {
    fn new() -> Self {
        let (catalog, wood) = wood_definition();
        let mut market = ServerMarket::new();
        let a = market.character(1);
        let b = market.character(2);
        let region = RegionId::new();

        for character in [a, b] {
            let mut hold = CargoHold::new(ShipInstanceId::new(), 100);
            hold.insert(
                &catalog,
                ItemInstance::new_resource(ItemInstanceId::new(), wood, 20),
            )
            .unwrap();
            market
                .deposit_all(character, region, &mut hold, &catalog)
                .unwrap();
        }

        Self {
            market,
            catalog,
            wood,
            region,
            a,
            b,
        }
    }
}

/// §70 double buy: dois compradores (ou o mesmo duas vezes) tentam a última
/// unidade. Apenas uma compra vence; a segunda é recusada sem efeito.
#[test]
fn double_buy_of_last_unit_wins_once() {
    let mut world = World::new();

    // A lista UMA unidade a 10g. Listing fee: 1% de 10g, teto = 1g (§46).
    let (order_num, fee) = world
        .market
        .create_order(world.a, world.region, world.wood, 1, Money(10))
        .unwrap();
    assert_eq!(fee, Money(1));
    let saldo_a_apos_listagem = world.market.balance(world.a);

    // Primeiro comprador vence.
    assert!(world
        .market
        .buy(world.b, world.region, order_num, 1)
        .is_ok());

    // Duplo clique do MESMO comprador: a order já saiu do board — o
    // segundo intent bate em OrderNotOpen e nada mais é pago.
    assert_eq!(
        world.market.buy(world.b, world.region, order_num, 1),
        Err(MarketError::OrderNotOpen)
    );

    // Ninguém pagou duas vezes e o vendedor recebeu líquido uma só vez.
    assert_eq!(world.market.balance(world.b).0, 1_000 - 10);
    assert_eq!(
        world.market.balance(world.a).0,
        saldo_a_apos_listagem.0 + 10 - 1
    );
}

/// §70 double loot: dois eventos de loot no mesmo tick. O baú drena uma
/// vez; o segundo encontra o vazio e não duplica item.
#[test]
fn double_loot_transfers_cargo_once() {
    let (catalog, wood) = wood_definition();
    let mut chest = WreckChest::new(WreckId::new());
    chest.insert(
        SurvivorItem {
            definition: wood,
            quantity: 5,
            durability: None,
        },
        ItemInstanceId::new(),
    );

    let mut hold = CargoHold::new(ShipInstanceId::new(), 100);

    // Primeiro LootWreck do tick: vence e drena o baú.
    let incoming = chest.drain();
    assert_eq!(incoming.len(), 1);
    hold.take_all(&catalog, incoming).unwrap();

    // Segundo LootWreck do tick (despawn do Bevy ainda é deferido):
    // baú vazio — nada a transferir, e é aqui que o guard barra o duplo.
    assert!(chest.is_empty());
    assert!(chest.drain().is_empty());
    assert_eq!(hold.items().len(), 1);
}

/// §70 double craft: dois cliques consumindo os MESMOS ingredientes. O
/// primeiro vence; o segundo falha sem produzir item duplicado.
#[test]
fn double_craft_consumes_ingredients_once() {
    let (catalog, wood) = wood_definition();
    let recipe = Recipe {
        id: RecipeId::new(),
        display_name: String::from("teste"),
        output_item: wood,
        output_quantity: 1,
        ingredients: vec![Ingredient {
            item: wood,
            quantity: 15,
        }],
        required_station: StationKind::Workbench,
        craft_time_secs: 0,
    };

    let mut hold = CargoHold::new(ShipInstanceId::new(), 100);
    hold.insert(
        &catalog,
        ItemInstance::new_resource(ItemInstanceId::new(), wood, 15),
    )
    .unwrap();

    assert!(craft(&recipe, &mut hold, &catalog, StationKind::Workbench).is_ok());
    assert!(craft(&recipe, &mut hold, &catalog, StationKind::Workbench).is_err());
    // Exatamente 1 unidade produzida (15 consumidas, 1 gerada).
    let total: u32 = hold
        .items()
        .iter()
        .map(|custody| custody.instance.quantity)
        .sum();
    assert_eq!(total, 1);
}

/// §70 retry: reenviar a listagem depois de sucesso não duplica escrow —
/// sem estoque restante, a segunda falha com NotInStorage.
#[test]
fn retry_of_sell_order_does_not_duplicate_escrow() {
    let mut world = World::new();
    assert!(world
        .market
        .create_order(world.a, world.region, world.wood, 20, Money(5))
        .is_ok());
    assert_eq!(
        world
            .market
            .create_order(world.a, world.region, world.wood, 20, Money(5)),
        Err(MarketError::NotInStorage)
    );
}

/// §70 disconnect: o navio afunda com o client, mas a CARTEIRA é da
/// personagem (§31) e sobrevive; reconectar com o mesmo client_num
/// encontra a mesma identidade e saldo.
#[test]
fn disconnect_then_reconnect_keeps_wallet() {
    let mut market = ServerMarket::new();
    let character = market.character(42);
    assert_eq!(market.balance(character), Money(1_000));

    let reconnected = market.character(42);
    assert_eq!(reconnected, character);
    assert_eq!(market.balance(reconnected), Money(1_000));
}

/// Full loot + mercado deixam rastro econômico completo no ledger.
#[test]
fn loot_and_market_record_economic_traces() {
    let mut world = World::new();
    let (order_num, _) = world
        .market
        .create_order(world.a, world.region, world.wood, 10, Money(10))
        .unwrap();
    world
        .market
        .buy(world.b, world.region, order_num, 10)
        .unwrap();

    let snapshot = world.market.snapshot();
    assert!(snapshot
        .ledger
        .entries()
        .iter()
        .any(|entry| entry.kind == LedgerKind::Trade && entry.amount == Money(100)));
    assert!(snapshot.ledger.burned().0 > 0);

    // E a resolução de destruição (full loot) continua determinística:
    // 10 de madeira no porão geram sobreviventes no wreck (~80%).
    let mut hold = CargoHold::new(ShipInstanceId::new(), 100);
    hold.insert(
        &world.catalog,
        ItemInstance::new_resource(ItemInstanceId::new(), world.wood, 10),
    )
    .unwrap();
    let cargo: Vec<_> = hold
        .items()
        .iter()
        .map(|custody| custody.instance.clone())
        .collect();
    let outcome = resolve_ship_destruction(
        DestructionEventId::new(),
        &[],
        &cargo,
        &LootPolicy::default(),
    );
    assert!(!outcome.wreck_items.is_empty());
}

/// Silencia avisos de import condicional (Utc entra via MarketOrder serde).
#[allow(dead_code)]
fn _touch(_: Utc) {}
