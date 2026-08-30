//! Integração do PostgresStateStore (MF-034, ADR-0004/0010).
//!
//! Roda contra um PostgreSQL real (Docker serve):
//!
//! ```text
//! docker run --rm -d --name mareforge-pg -p 54329:5432 \
//!   -e POSTGRES_PASSWORD=mareforge postgres:16-alpine
//! MAREFORGE_TEST_DATABASE_URL=postgres://postgres:mareforge@localhost:54329/postgres \
//!   cargo test -p mareforge-server --test postgres
//! ```
//!
//! Sem a variável, o teste pula com aviso — CI sem banco não quebra, mas a
//! verificação do adapter também não acontece (reporte honesto > falso verde).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Utc;
use mareforge_domain_economy::{Ledger, LedgerKind, MarketOrder, Money, OrderStatus};
use mareforge_domain_items::{
    CargoHold, Custody, ItemCatalog, ItemDefinition, ItemInstance, ItemKind, ItemLocation,
};
use mareforge_domain_ships::ShipKind;
use mareforge_server::market::{MarketSnapshot, ServerMarket};
use mareforge_server::persist::{PostgresStateStore, ShipRecord, StateStore};
use mareforge_shared::ids::{
    CharacterId, ItemDefinitionId, ItemInstanceId, MarketOrderId, RegionId, ShipInstanceId,
};

/// Os três testes compartilham UM banco (o estado é global por natureza):
/// mutex serializa e o reset limpa o estado antes de cada cenário.
fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Limpa todas as tabelas (o ledger é append-only no jogo; no TESTE o
/// cenário começa limpo para as asserções serem exatas).
fn reset_database(url: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime de teste");
    runtime.block_on(async {
        let pool = sqlx::PgPool::connect(url).await.expect("pool de reset");
        sqlx::query(
            "TRUNCATE accounts, characters, ship_instances, item_instances, \
             ledger_entries, market_orders, wallets CASCADE",
        )
        .execute(&pool)
        .await
        .expect("reset do banco de teste");
        pool.close().await;
    });
}

fn store_or_skip() -> Option<(Arc<PostgresStateStore>, String)> {
    match std::env::var("MAREFORGE_TEST_DATABASE_URL") {
        Ok(url) => {
            // Conectar PRIMEIRO (roda as migrations), resetar DEPOIS.
            let store = match PostgresStateStore::connect(&url) {
                Ok(store) => store,
                Err(error) => panic!("banco de teste configurado mas não abriu: {error}"),
            };
            reset_database(&url);
            Some((Arc::new(store), url))
        }
        Err(_) => {
            eprintln!(
                "PULANDO: defina MAREFORGE_TEST_DATABASE_URL para testar o PostgresStateStore"
            );
            None
        }
    }
}

fn sample_snapshot() -> (MarketSnapshot, CharacterId) {
    let character = CharacterId::new();
    let region = RegionId::new();
    let item = ItemDefinitionId::new();
    let order_id = MarketOrderId::new();
    let order_num = 0u32;

    let mut ledger = Ledger::default();
    ledger.record(LedgerKind::Mint, Money(1_000), "bootstrap dev (§48)");
    ledger.record(LedgerKind::Burn, Money(1), "listing fee (§46)");

    let mut balances = HashMap::new();
    balances.insert(character, Money(999));
    let mut identities = HashMap::new();
    identities.insert("token-alfa".to_string(), character);

    let stack = Custody {
        instance: ItemInstance::new_resource(ItemInstanceId::new(), item, 30),
        location: ItemLocation::PortStorage(region),
    };
    let escrowed = Custody {
        instance: ItemInstance::new_resource(ItemInstanceId::new(), item, 5),
        location: ItemLocation::MarketEscrow(order_id),
    };

    let snapshot = MarketSnapshot {
        identities,
        balances,
        storage: vec![mareforge_server::market::StorageEntry {
            character,
            region,
            stacks: vec![stack],
        }],
        escrow: vec![mareforge_server::market::EscrowEntry {
            order_num,
            stacks: vec![escrowed],
        }],
        board: vec![MarketOrder {
            id: order_id,
            seller: character,
            item,
            quantity: 5,
            unit_price: Money(8),
            region,
            status: OrderStatus::Open,
            created_at: Utc::now(),
            expires_at: Utc::now(),
            filled_quantity: 0,
        }],
        order_nums: HashMap::from([(order_num, order_id)]),
        next_order_num: 1,
        ledger,
    };
    (snapshot, character)
}

fn quantity_of(snapshot: &MarketSnapshot, character: CharacterId) -> u32 {
    snapshot
        .storage
        .iter()
        .filter(|entry| entry.character == character)
        .flat_map(|entry| entry.stacks.iter())
        .map(|custody| custody.instance.quantity)
        .sum()
}

/// MF-034: o estado econômico completo sobrevive ao banco — salvo em uma
/// transação, lido de volta idêntico (carteiras, storage, escrow, orders,
/// ledger).
#[test]
fn market_state_roundtrips_through_postgres() {
    let _guard = test_lock();
    let Some((store, _url)) = store_or_skip() else {
        return;
    };
    let (snapshot, character) = sample_snapshot();

    store
        .save_market(&snapshot)
        .expect("save_market transacional");
    let restored = store
        .load_market()
        .expect("load_market")
        .expect("estado após save");

    assert_eq!(restored.identities.get("token-alfa"), Some(&character));
    assert_eq!(restored.balances.get(&character), Some(&Money(999)));
    assert_eq!(quantity_of(&restored, character), 30);
    assert_eq!(restored.board.len(), 1);
    assert_eq!(restored.board[0].unit_price, Money(8));
    assert_eq!(restored.order_nums.len(), 1);
    assert_eq!(restored.escrow.len(), 1);
    assert_eq!(restored.escrow[0].stacks[0].instance.quantity, 5);
    assert_eq!(restored.ledger.entries().len(), 2);
    assert_eq!(restored.ledger.burned(), Money(1));
    assert_eq!(restored.next_order_num, 1);
}

/// A unidade atômica é o estado inteiro (ADR-0010 no Alpha single-writer):
/// salvar duas vezes seguidas não duplica linhas nem entra em conflito.
#[test]
fn repeated_saves_stay_consistent() {
    let _guard = test_lock();
    let Some((store, _url)) = store_or_skip() else {
        return;
    };
    let (snapshot, character) = sample_snapshot();
    store.save_market(&snapshot).expect("primeiro save");
    store.save_market(&snapshot).expect("segundo save");
    let restored = store
        .load_market()
        .expect("load_market")
        .expect("estado após saves");
    assert_eq!(quantity_of(&restored, character), 30, "carga não duplica");
    assert_eq!(restored.board.len(), 1, "order não duplica entre saves");
    assert_eq!(restored.ledger.entries().len(), 2, "ledger é append-only");
}

/// MF-035/034: o navio do personagem sobrevive — casco, HP, posição e a
/// carga embarcada (item_instances com location ShipCargo).
#[test]
fn ship_record_roundtrips_through_postgres() {
    let _guard = test_lock();
    let Some((store, _url)) = store_or_skip() else {
        return;
    };
    let character = CharacterId::new();
    // No fluxo real o personagem já existe no banco (market.character →
    // save_market). O teste reproduz a ordem: identidade primeiro, navio
    // depois — a FK de ship_instances é o fail-closed do banco.
    let mut ledger = Ledger::default();
    ledger.record(LedgerKind::Mint, Money(1_000), "bootstrap dev (§48)");
    let seed = MarketSnapshot {
        identities: HashMap::from([("token-navio".to_string(), character)]),
        balances: HashMap::from([(character, Money(1_000))]),
        storage: Vec::new(),
        escrow: Vec::new(),
        board: Vec::new(),
        order_nums: HashMap::new(),
        next_order_num: 0,
        ledger,
    };
    store.save_market(&seed).expect("identidade no banco");

    let ship_instance = ShipInstanceId::new();
    let item = ItemDefinitionId::new();
    let sail_item = ItemDefinitionId::new();
    let record = ShipRecord {
        ship_instance,
        character,
        kind: ShipKind::Corsair,
        hp: 55,
        x: -300.5,
        y: 42.25,
        heading: 1.25,
        cargo: vec![Custody {
            instance: ItemInstance::new_resource(ItemInstanceId::new(), item, 12),
            location: ItemLocation::ShipCargo(ship_instance),
        }],
        equipped: vec![Custody {
            instance: ItemInstance::new_equipment(ItemInstanceId::new(), sail_item, 100),
            location: ItemLocation::Equipped {
                ship: ship_instance,
                slot: mareforge_domain_items::EquipmentSlot::Sail,
            },
        }],
    };

    store.save_ship(&record).expect("save_ship");
    let restored = store
        .load_ship(character)
        .expect("load_ship")
        .expect("navio persistido");

    assert_eq!(restored.ship_instance, ship_instance);
    assert_eq!(restored.character, character);
    assert_eq!(restored.kind, ShipKind::Corsair);
    assert_eq!(restored.hp, 55);
    assert_eq!(restored.cargo.len(), 1);
    assert_eq!(restored.cargo[0].instance.quantity, 12);
    assert_eq!(restored.equipped.len(), 1, "loadout persistiu");
    assert_eq!(
        restored.equipped[0].location,
        ItemLocation::Equipped {
            ship: ship_instance,
            slot: mareforge_domain_items::EquipmentSlot::Sail,
        }
    );
    assert_eq!(
        restored.cargo[0].location,
        ItemLocation::ShipCargo(ship_instance)
    );
}

/// MF-041: um Expired persistido no banco volta com o status preservado e
/// sem escrow, porque o servidor já devolveu o item ao storage do seller.
#[test]
fn expired_order_roundtrips_with_escrow_returned() {
    let _guard = test_lock();
    let Some((store, _url)) = store_or_skip() else {
        return;
    };
    let region = RegionId::new();
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

    let mut market = ServerMarket::with_store(Some(store.clone()));
    let character = market.character("token-expired");
    let mut hold = CargoHold::new(ShipInstanceId::new(), 1_000);
    hold.insert(
        &catalog,
        ItemInstance::new_resource(ItemInstanceId::new(), item, 10),
    )
    .expect("teste cabe no porão");
    market
        .deposit_all(character, region, &mut hold, &catalog)
        .expect("teste deposita no storage");
    market.policy.default_order_duration_secs = 60;
    market
        .create_order(character, region, item, 10, Money(8))
        .expect("storage tem estoque");
    let order = market.snapshot().board[0].clone();
    market.expire_orders(order.expires_at + chrono::Duration::seconds(1));

    let restored = store
        .load_market()
        .expect("load_market")
        .expect("estado após expire");
    let restored_order = restored.board[0].clone();
    assert_eq!(restored_order.status, OrderStatus::Expired);
    assert!(restored.escrow.is_empty(), "escrow devolvido ao storage");
    let storage_quantity = restored
        .storage
        .iter()
        .filter(|entry| entry.character == character && entry.region == region)
        .flat_map(|entry| entry.stacks.iter())
        .filter(|custody| custody.instance.definition == item)
        .map(|custody| custody.instance.quantity)
        .sum::<u32>();
    assert_eq!(storage_quantity, 10);
}

/// MF-027 cont.: o snapshot de wrecks sobrevive ao banco. Dois wrecks
/// ativos (com e sem exclusive_looter) roundtrippam com todos os campos,
/// e `delete_wreck` remove pontualmente.
#[test]
fn wreck_snapshot_roundtrips_through_postgres() {
    let _guard = test_lock();
    let Some((store, url)) = store_or_skip() else {
        return;
    };
    reset_database(&url);

    let killer = CharacterId::new();
    let records = vec![
        mareforge_server::persist::WreckRecord {
            wreck_num: 0,
            wreck_id: mareforge_shared::ids::WreckId::new(),
            x: 120.0,
            y: -45.0,
            exclusive_looter: Some(killer),
            spawned_at_secs: 12.5,
        },
        mareforge_server::persist::WreckRecord {
            wreck_num: 1,
            wreck_id: mareforge_shared::ids::WreckId::new(),
            x: -300.0,
            y: 80.0,
            exclusive_looter: None,
            spawned_at_secs: 90.25,
        },
    ];

    store
        .save_wreck_snapshot(&records)
        .expect("save_wreck_snapshot grava dois wrecks");

    let restored = store
        .load_wreck_snapshot()
        .expect("load_wreck_snapshot lê o snapshot");
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].wreck_num, 0);
    assert_eq!(restored[0].x, 120.0);
    assert_eq!(restored[0].y, -45.0);
    assert_eq!(restored[0].exclusive_looter, Some(killer));
    assert!((restored[0].spawned_at_secs - 12.5).abs() < 1e-6);
    assert_eq!(restored[1].wreck_num, 1);
    assert_eq!(restored[1].x, -300.0);
    assert_eq!(restored[1].exclusive_looter, None);

    store.delete_wreck(0).expect("delete_wreck");
    let remaining = store.load_wreck_snapshot().expect("load após delete");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].wreck_num, 1);
}
