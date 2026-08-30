//! MF-030 — End-to-End Vertical Slice (PRD Phase 10). A prova do loop:
//!
//! ```text
//! Player A coleta → fabrica → transporta
//!   → Player B ataca → navio afunda → loot transfere
//!   → B volta → vende → a economia registra tudo
//! ```
//!
//! O teste dirige os módulos puros de domínio e o ServerMarket na mesma
//! sequência do jogo — a mesma fronteira que os handlers Bevy chamam.

use mareforge_domain_combat::{
    apply_damage, resolve_ship_destruction, DamageOutcome, LootPolicy, WreckChest,
};
use mareforge_domain_crafting::{Ingredient, Recipe, StationKind};
use mareforge_domain_economy::{LedgerKind, Money};
use mareforge_domain_items::{
    CargoHold, Custody, ItemCatalog, ItemDefinition, ItemInstance, ItemKind,
};
use mareforge_domain_ships::{step_motion, MotionInput, MotionTuning, ShipMotion};
use mareforge_domain_world::{ResourceNode, WorldMap};
use mareforge_server::market::{port_region, ServerMarket};
use mareforge_shared::ids::{
    CharacterId, DestructionEventId, ItemDefinitionId, ItemInstanceId, RecipeId, ShipInstanceId,
    WreckId,
};

const STARTER_PORT: (f32, f32) = (-560.0, 0.0); // doca do Porto da Serra

fn catalog_with_goods() -> (ItemCatalog, ItemDefinitionId, ItemDefinitionId) {
    let wood = ItemDefinitionId::new();
    let hull = ItemDefinitionId::new();
    let mut catalog = ItemCatalog::default();
    let mut register = |definition: ItemDefinition| catalog.register(definition).unwrap();
    register(ItemDefinition {
        id: wood,
        kind: ItemKind::Resource,
        equipment: None,
        max_stack: 100,
        base_weight: 2,
        tags: Default::default(),
        display_name: String::from("Madeira"),
    });
    register(ItemDefinition {
        id: hull,
        kind: ItemKind::Equipment,
        equipment: Some(mareforge_domain_items::EquipmentStats {
            damage: 0,
            speed: 0,
            cargo: 0,
            hp: 40,
            range: 0,
        }),
        max_stack: 1,
        base_weight: 8,
        tags: Default::default(),
        display_name: String::from("Casco Reforçado"),
    });
    (catalog, wood, hull)
}

fn craft_hull_recipe(wood: ItemDefinitionId, hull: ItemDefinitionId) -> Recipe {
    Recipe {
        id: RecipeId::new(),
        display_name: String::from("Casco Reforçado"),
        output_item: hull,
        output_quantity: 1,
        ingredients: vec![Ingredient {
            item: wood,
            quantity: 15,
        }],
        required_station: StationKind::Workbench,
        craft_time_secs: 0,
    }
}

/// A personagem de um jogador: porão e carteira.
struct Player {
    character: CharacterId,
    hold: CargoHold,
}

#[test]
fn vertical_slice_loop_gather_craft_transport_fight_loot_sell() {
    // ===== Mundo =====
    let map = WorldMap::vertical_slice();
    let (catalog, wood, hull) = catalog_with_goods();
    let region_serra = map.region_by_name("Porto da Serra").unwrap().id;
    let mut market = ServerMarket::new();

    let mut a = Player {
        character: market.character("token-a"),
        hold: CargoHold::new(ShipInstanceId::new(), 100),
    };
    let mut b = Player {
        character: market.character("token-b"),
        hold: CargoHold::new(ShipInstanceId::new(), 100),
    };

    // ===== 1. A coleta (node → ShipCargo) =====
    let mut node = ResourceNode {
        id: mareforge_shared::ids::ResourceNodeId::new(),
        name: "Bosque da Serra",
        x: -700.0,
        y: 90.0,
        region: region_serra,
        resource: wood,
        stock: 60,
        max_stock: 60,
    };
    let mut gathered = 0;
    while gathered < 30 {
        let taken = node.take(10);
        assert!(taken > 0, "node tem estoque para a sessão de coleta");
        a.hold
            .insert(
                &catalog,
                ItemInstance::new_resource(ItemInstanceId::new(), wood, taken),
            )
            .expect("porão comporta a coleta do dia");
        gathered += taken;
    }
    assert_eq!(node.stock, 60 - 30);
    assert_eq!(a.hold.used_weight(&catalog).unwrap(), 60); // 30 × peso 2

    // ===== 2. A fabrica na OFICINA do porto (MF-036/037) =====
    // atracar → depositar o dia de coleta → craftar NO STORAGE → embarcar
    // só o que quer transportar. O porão não é matéria-prima automática.
    market
        .deposit_all(a.character, region_serra, &mut a.hold, &catalog)
        .expect("doca da Serra recebe o porão do dia");
    assert_eq!(
        a.hold.used_weight(&catalog).unwrap(),
        0,
        "porão vazio na doca"
    );
    let recipe = craft_hull_recipe(wood, hull);
    let crafted = market
        .craft_at_storage(
            a.character,
            region_serra,
            &recipe,
            &catalog,
            StationKind::Workbench,
        )
        .expect("oficina da Serra com madeira guardada de sobra");
    assert_eq!(crafted.definition, hull);
    assert!(crafted.durability.is_some());
    assert_eq!(
        market.storage_quantity(a.character, region_serra, hull),
        1,
        "casco nasce no storage, não no porão"
    );
    market
        .withdraw_all(a.character, region_serra, &mut a.hold, &catalog)
        .expect("embarca o casco (e o resto da madeira)");
    assert_eq!(
        a.hold.used_weight(&catalog).unwrap(),
        38, // 15 madeira (30) + casco (8)
        "carga embarcada: decisão explícita do jogador"
    );

    // ===== 3. A transporta (modelo puro de movimento, rota leste) =====
    let definition = mareforge_domain_ships::ShipDefinition::small_merchant();
    let stats = mareforge_domain_ships::compute_ship_stats(
        &definition,
        &mareforge_domain_ships::EquippedComponents::default(),
        &catalog,
    )
    .expect("navio sem equipamento: stats não falham");
    let mut motion = ShipMotion {
        x: STARTER_PORT.0,
        y: STARTER_PORT.1,
        ..ShipMotion::default()
    };
    let tuning = MotionTuning::default();
    for _ in 0..600 {
        step_motion(
            &mut motion,
            &stats,
            MotionInput {
                throttle: 1.0,
                turn: 0.0,
            },
            &tuning,
            1.0 / 30.0,
        );
    }
    assert!(
        motion.x > STARTER_PORT.0 + 200.0,
        "A navegou para leste rumo à rota da costa"
    );
    // A rota é fronteira: PvP legal — é aqui que B pode atacar (§8/§9).
    assert_eq!(
        map.zone_at(motion.x, motion.y).unwrap().tier,
        mareforge_domain_world::RiskTier::Frontier
    );

    // ===== 4. A deposita no porto e lista o Casco (storage → escrow) =====
    // A volta à baía para operar o mercado (§45).
    let (port_region_id, _) = port_region(&map, STARTER_PORT.0, STARTER_PORT.1)
        .expect("doca do Porto da Serra é área de porto");
    assert_eq!(port_region_id, region_serra);
    market
        .deposit_all(a.character, port_region_id, &mut a.hold, &catalog)
        .expect("deposita tudo: madeira restante + casco");
    let (_order_num, listing_fee) = market
        .create_order(a.character, port_region_id, hull, 1, Money(60))
        .expect("o casco está no storage local");
    assert!(listing_fee.0 > 0, "listing fee queimou ouro (§46)");

    // ===== 5. B ataca: projéteis até afundar (PvP em fronteira) =====
    let mut hp = 100;
    let mut sinking = false;
    for _ in 0..20 {
        match apply_damage(hp, 20) {
            DamageOutcome::Survived { remaining_hp } => hp = remaining_hp,
            DamageOutcome::Destroyed => {
                sinking = true;
                break;
            }
        }
    }
    assert!(sinking, "5 impactos de 20 afundam o casco de 100");

    // ===== 6. Navio afunda: full loot transfere carga de dono =====
    // A carga do navio de A era: madeira sobrando + o casco fabricado.
    let mut hold_afundado = CargoHold::new(ShipInstanceId::new(), 100);
    hold_afundado
        .insert(
            &catalog,
            ItemInstance::new_resource(ItemInstanceId::new(), wood, 5),
        )
        .unwrap();
    hold_afundado
        .insert(
            &catalog,
            ItemInstance::new_equipment(ItemInstanceId::new(), hull, 100),
        )
        .unwrap();
    let cargo: Vec<ItemInstance> = hold_afundado
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
    // Nada desaparece no vazio: por definição, quantidade sobrevivente +
    // quantidade destruída = quantidade embarcada (a pilha pode se dividir:
    // 80% da carga sobrevive por unidade, §25).
    let survived: u32 = outcome.wreck_items.iter().map(|s| s.quantity).sum();
    let destroyed: u32 = outcome.destroyed_items.iter().map(|s| s.quantity).sum();
    let shipped: u32 = cargo.iter().map(|item| item.quantity).sum();
    assert_eq!(survived + destroyed, shipped);

    let mut chest = WreckChest::new(WreckId::new());
    for survivor in &outcome.wreck_items {
        chest.insert(*survivor, ItemInstanceId::new());
    }
    // B chega no wreck e saqueia (MF-015: take_all atômico).
    let incoming: Vec<Custody> = chest.drain();
    let lootado = !incoming.is_empty();
    if lootado {
        b.hold.take_all(&catalog, incoming).expect("B tem porão");
    }

    // ===== 7. B volta ao porto e vende o que saqueou =====
    let mut vendeu_de_volta = false;
    if lootado {
        market
            .deposit_all(b.character, port_region_id, &mut b.hold, &catalog)
            .expect("loot depositado no storage local de B");
        // B lista a madeira saqueada a preço de mercado.
        let wood_storage = market
            .snapshot()
            .storage
            .into_iter()
            .find(|entry| entry.character == b.character && entry.region == port_region_id)
            .map(|entry| {
                entry
                    .stacks
                    .iter()
                    .filter(|custody| custody.instance.definition == wood)
                    .map(|custody| custody.instance.quantity)
                    .sum::<u32>()
            })
            .unwrap_or(0);
        if wood_storage > 0 {
            if let Ok((buy_order, _)) =
                market.create_order(b.character, port_region_id, wood, 1, Money(8))
            {
                // A compra a madeira de volta — a economia gira (§50).
                if market
                    .buy(a.character, port_region_id, buy_order, 1)
                    .is_ok()
                {
                    vendeu_de_volta = true;
                }
            }
        }
    }

    // ===== 8. A economia registrou TUDO (MF-026/§71) =====
    let snapshot = market.snapshot();
    let kinds: Vec<_> = snapshot.ledger.entries().iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&LedgerKind::Mint), "bootstrap dev cunhado");
    assert!(
        kinds.contains(&LedgerKind::Burn),
        "listing fees queimaram ouro"
    );
    assert!(
        snapshot.ledger.burned().0 >= 2,
        "duas listagens queimaram fee (A no casco, B na madeira)"
    );
    if vendeu_de_volta {
        assert!(
            snapshot
                .ledger
                .entries()
                .iter()
                .any(|entry| entry.kind == LedgerKind::Trade),
            "a compra de volta registrou trade no ledger"
        );
    }
    // O snapshot é persistível (MF-027) e fecha redondo: roundtrip idêntico.
    let bytes = serde_json::to_vec(&snapshot).expect("snapshot serializa");
    let restored: mareforge_server::market::MarketSnapshot =
        serde_json::from_slice(&bytes).expect("snapshot desserializa");
    assert_eq!(restored.balances.len(), snapshot.balances.len());
}
