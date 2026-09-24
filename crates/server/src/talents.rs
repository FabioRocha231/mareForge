//! Rosa dos Ventos no servidor (MV-067). As regras da árvore vivem em
//! `domain-ships::talents`; aqui: carrega e grava o que o capitão aprendeu,
//! valida os pontos contra o nível de Renome e aplica o bônus nos stats do
//! navio.
//!
//! Aplicar: quem recalcula stats (spawn, restore, troca de equipamento)
//! grava os stats base em `ServerShip.stats`. `apply_to_ships` percebe que
//! eles não são os que ele deixou lá e reaplica o bônus por cima — nenhum
//! desses caminhos precisa saber que a árvore existe.

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use marvyr_domain_economy::{LedgerKind, Money};
use marvyr_domain_ships::talents::{can_allocate, points_for_level, respec_cost, TalentBonus};
use marvyr_domain_ships::{compute_ship_stats, ShipStats, VesselPresence};
use marvyr_protocol::{ActionKind, AllocateTalent, RespecTalents, TalentsSnapshot};
use marvyr_shared::ids::CharacterId;
use tracing::{info, warn};

use crate::market::ServerMarket;
use crate::net::{DevItems, ReliableChannel, ServerShip};
use crate::persist::StoreHandle;
use crate::renown::CaptainRenown;
use crate::seafaring::send_action;
use crate::sets::SimulationSet;

#[derive(Debug, Clone, Default)]
struct Learned {
    allocated: Vec<String>,
    /// Conexão que já recebeu o snapshot (reconexão recarrega).
    client: Option<ClientId>,
}

#[derive(Resource, Default)]
pub struct CaptainTalents {
    captains: HashMap<CharacterId, Learned>,
}

impl CaptainTalents {
    pub fn bonus(&self, character: CharacterId) -> TalentBonus {
        self.captains
            .get(&character)
            .map_or_else(TalentBonus::default, |learned| {
                TalentBonus::of(&learned.allocated)
            })
    }
}

/// O que `apply_to_ships` deixou no navio da última vez.
#[derive(Component)]
struct TalentsApplied {
    bonus: TalentBonus,
    stats: ShipStats,
}

pub fn install(app: &mut App) {
    app.init_resource::<CaptainTalents>();
    app.add_systems(
        FixedUpdate,
        (
            load_on_connect,
            handle_allocate,
            handle_respec,
            apply_to_ships,
        )
            .chain()
            .in_set(SimulationSet::Input),
    );
}

fn send_snapshot(
    connection_manager: &mut ConnectionManager,
    client_id: ClientId,
    allocated: &[String],
) {
    let _ = connection_manager.send_message::<ReliableChannel, _>(
        client_id,
        &TalentsSnapshot {
            allocated: allocated.to_vec(),
        },
    );
}

fn load_on_connect(
    ships: Query<&ServerShip>,
    store: Res<StoreHandle>,
    mut talents: ResMut<CaptainTalents>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    for ship in &ships {
        let Some(client_id) = ship.client_id else {
            continue;
        };
        if talents
            .captains
            .get(&ship.character)
            .is_some_and(|learned| learned.client == Some(client_id))
        {
            continue;
        }
        // Toda mudança grava na hora: o banco é a verdade na reconexão.
        let allocated = match store.0.as_ref() {
            Some(store) => store.load_talents(ship.character).unwrap_or_else(|error| {
                warn!(%error, "talentos não carregaram: sessão sem bônus");
                Vec::new()
            }),
            None => talents
                .captains
                .get(&ship.character)
                .map(|learned| learned.allocated.clone())
                .unwrap_or_default(),
        };
        send_snapshot(&mut connection_manager, client_id, &allocated);
        talents.captains.insert(
            ship.character,
            Learned {
                allocated,
                client: Some(client_id),
            },
        );
    }
}

fn save(store: &StoreHandle, character: CharacterId, allocated: &[String]) -> Result<(), String> {
    match store.0.as_ref() {
        Some(store) => store.save_talents(character, allocated),
        None => Ok(()),
    }
}

// System Bevy: params são injeção de dependência, não assinatura.
#[allow(clippy::too_many_arguments)]
fn handle_allocate(
    mut events: EventReader<ServerReceiveMessage<AllocateTalent>>,
    ships: Query<&ServerShip>,
    renown: Res<CaptainRenown>,
    store: Res<StoreHandle>,
    mut talents: ResMut<CaptainTalents>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    for event in events.read() {
        let client_id = event.from();
        let Some(ship) = ships.iter().find(|ship| ship.client_id == Some(client_id)) else {
            continue;
        };
        let character = ship.character;
        let node = &event.message().node;
        let learned = talents.captains.entry(character).or_default();
        let points = points_for_level(renown.level(character));
        if let Err(error) = can_allocate(&learned.allocated, node, points) {
            send_action(
                &mut connection_manager,
                client_id,
                ActionKind::Talent,
                false,
                error.reason(),
            );
            continue;
        }
        learned.allocated.push(node.clone());
        if let Err(error) = save(&store, character, &learned.allocated) {
            learned.allocated.pop();
            warn!(%error, "talento não foi gravado");
            send_action(
                &mut connection_manager,
                client_id,
                ActionKind::Talent,
                false,
                "Não deu para gravar agora; tente de novo.",
            );
            continue;
        }
        info!(?character, node, "talento aprendido");
        send_snapshot(&mut connection_manager, client_id, &learned.allocated);
    }
}

// System Bevy: params são injeção de dependência, não assinatura.
#[allow(clippy::too_many_arguments)]
fn handle_respec(
    mut events: EventReader<ServerReceiveMessage<RespecTalents>>,
    ships: Query<&ServerShip>,
    store: Res<StoreHandle>,
    mut market: ResMut<ServerMarket>,
    mut talents: ResMut<CaptainTalents>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    for event in events.read() {
        let client_id = event.from();
        let Some(ship) = ships.iter().find(|ship| ship.client_id == Some(client_id)) else {
            continue;
        };
        let character = ship.character;
        let refuse = |connection_manager: &mut ConnectionManager, reason: String| {
            send_action(
                connection_manager,
                client_id,
                ActionKind::Talent,
                false,
                reason,
            );
        };
        if !matches!(ship.presence, VesselPresence::Docked(_)) {
            refuse(
                &mut connection_manager,
                "Redistribuir só no porto: atraque primeiro.".into(),
            );
            continue;
        }
        let learned = talents.captains.entry(character).or_default();
        if learned.allocated.is_empty() {
            refuse(
                &mut connection_manager,
                "Nenhum talento para esquecer.".into(),
            );
            continue;
        }
        let cost = Money(respec_cost(learned.allocated.len()));
        if market.balance(character).0 < cost.0 {
            refuse(
                &mut connection_manager,
                format!("Ouro insuficiente: redistribuir custa {}g.", cost.0),
            );
            continue;
        }
        // Grava antes de cobrar: se o banco falhar, ninguém paga por nada.
        if let Err(error) = save(&store, character, &[]) {
            warn!(%error, "respec não foi gravado");
            refuse(
                &mut connection_manager,
                "Não deu para gravar agora; tente de novo.".into(),
            );
            continue;
        }
        market
            .debit(character, cost)
            .expect("saldo conferido acima, no mesmo tick");
        market.ledger.record(
            LedgerKind::Burn,
            cost,
            format!("respec talentos ship {}", ship.ship_id),
        );
        market.persist();
        learned.allocated.clear();
        info!(?character, cost = cost.0, "talentos redistribuídos");
        crate::market::send_wallet(
            &mut connection_manager,
            &market,
            &[(Some(client_id), character)],
            character,
        );
        send_snapshot(&mut connection_manager, client_id, &[]);
        send_action(
            &mut connection_manager,
            client_id,
            ActionKind::Talent,
            true,
            format!("Rosa dos Ventos zerada (-{}g). Pontos de volta.", cost.0),
        );
    }
}

fn apply_to_ships(
    mut commands: Commands,
    talents: Res<CaptainTalents>,
    dev_ships: Res<crate::crafting::DevShips>,
    dev: Res<DevItems>,
    mut ships: Query<(Entity, &mut ServerShip, Option<&TalentsApplied>)>,
) {
    for (entity, mut ship, applied) in &mut ships {
        let bonus = talents.bonus(ship.character);
        if applied.is_some_and(|applied| applied.bonus == bonus && applied.stats == ship.stats) {
            continue;
        }
        let Ok(base) = compute_ship_stats(
            dev_ships.definition(ship.kind),
            &ship.loadout.components(),
            &dev.catalog,
        ) else {
            continue;
        };
        let stats = bonus.apply(&base);
        // Navio novo nasce de casco cheio; depois, casco maior não cura.
        let is_fresh = applied.is_none() && ship.hp >= ship.stats.max_hp;
        ship.hp = if is_fresh {
            stats.max_hp
        } else {
            ship.hp.min(stats.max_hp)
        };
        ship.hold.set_capacity(stats.cargo_capacity);
        ship.stats = stats.clone();
        commands
            .entity(entity)
            .insert(TalentsApplied { bonus, stats });
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use marvyr_domain_ships::ShipKind;
    use marvyr_domain_world::WorldMap;

    use super::*;
    use crate::crafting::DevShips;
    use crate::net::{spawn_ship_for, ShipIdCounter, DEFAULT_WORLD_SEED};

    fn learn(app: &mut App, character: CharacterId, allocated: &[&str]) {
        app.world_mut()
            .resource_mut::<CaptainTalents>()
            .captains
            .insert(
                character,
                Learned {
                    allocated: allocated.iter().map(|id| id.to_string()).collect(),
                    client: None,
                },
            );
    }

    fn ship(app: &mut App) -> (ShipStats, u32) {
        let world = app.world_mut();
        let mut ships = world.query::<&ServerShip>();
        let ship = ships.single(world);
        (ship.stats.clone(), ship.hp)
    }

    #[test]
    fn bonus_rides_on_top_of_base_stats_and_survives_recalc() {
        let mut app = App::new();
        app.insert_resource(DevItems::new())
            .insert_resource(DevShips::new())
            .init_resource::<CaptainTalents>()
            .add_systems(Update, apply_to_ships);
        let character = CharacterId::new();
        learn(&mut app, character, &["com.olho", "com.estiva"]);
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, dev: Res<DevItems>, ships: Res<DevShips>| {
                    let map = WorldMap::from_seed(DEFAULT_WORLD_SEED);
                    spawn_ship_for(
                        &mut commands,
                        &mut ShipIdCounter(1),
                        &dev,
                        &ships,
                        &map,
                        ShipKind::SmallMerchant,
                        None,
                        character,
                        Vec::new(),
                    );
                },
            )
            .expect("spawn");
        app.update();
        let base = DevShips::new()
            .definition(ShipKind::SmallMerchant)
            .cargo_capacity;
        let (boosted, hp) = ship(&mut app);
        assert_eq!(boosted.cargo_capacity, base * 105 / 100);
        assert_eq!(hp, boosted.max_hp, "nasce de casco cheio");

        // Troca de equipamento grava stats base: o bônus volta por cima.
        {
            let world = app.world_mut();
            let mut ships = world.query::<&mut ServerShip>();
            let mut ship = ships.single_mut(world);
            ship.stats.cargo_capacity = base;
            ship.hp = 10;
        }
        learn(
            &mut app,
            character,
            &["com.olho", "com.estiva", "nav.leme", "nav.costado"],
        );
        app.update();
        let (again, hp) = ship(&mut app);
        assert_eq!(again.cargo_capacity, base * 105 / 100);
        assert!(again.max_hp > boosted.max_hp);
        assert_eq!(hp, 10, "casco maior não cura");
    }
}
