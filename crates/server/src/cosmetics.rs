//! Cosméticos no servidor (MV-066). Só aparência: o que o capitão possui
//! vem do store (concedido pela administração), o que ele usa vai no
//! `ShipState` para todo mundo ver. Nenhum stat passa por aqui — `stats`
//! é calculado só do casco e do loadout.

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use marvyr_domain_ships::{
    cosmetic_by_code, cosmetic_code, CosmeticSlot, ShipCosmetics, VesselPresence, COSMETICS,
};
use marvyr_protocol::{CosmeticsSnapshot, WearCosmetic};
use marvyr_shared::ids::CharacterId;
use tracing::{info, warn};

use crate::net::{ReliableChannel, ServerShip};
use crate::persist::{CosmeticsRecord, StoreHandle};
use crate::sets::SimulationSet;

/// O visual de um capitão: o que possui e o que está usando.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaptainLook {
    pub owned: Vec<u8>,
    pub worn: ShipCosmetics,
    /// Conexão que já recebeu o snapshot (reconexão recarrega do store e
    /// pega concessões novas).
    client: Option<ClientId>,
    /// Última troca aceita (s de simulação): cada troca grava no banco.
    last_wear: Option<f64>,
}

/// Uma troca por segundo por capitão: a UI não precisa de mais, e cada troca
/// é uma escrita no banco dentro do tick.
const WEAR_COOLDOWN_SECS: f64 = 1.0;

#[derive(Resource, Default)]
pub struct CaptainCosmetics(pub HashMap<CharacterId, CaptainLook>);

impl CaptainCosmetics {
    pub fn worn(&self, character: CharacterId) -> ShipCosmetics {
        self.0
            .get(&character)
            .map(|look| look.worn)
            .unwrap_or_default()
    }
}

pub fn install(app: &mut App) {
    app.init_resource::<CaptainCosmetics>();
    app.add_systems(
        FixedUpdate,
        (load_on_connect, handle_wear)
            .chain()
            .in_set(SimulationSet::Input),
    );
}

/// Dev: `MARVYR_DEV_COSMETICS=1` (fora de produção) libera o catálogo todo.
fn dev_grants_all() -> bool {
    std::env::var_os("MARVYR_DEV_COSMETICS").is_some()
        && !std::env::var("MARVYR_ENV").is_ok_and(|env| env == "production")
}

/// Registro do store em códigos. Id desconhecido (catálogo mudou) é
/// ignorado; o que está em uso sem estar concedido (revogado) cai.
fn look_from_record(record: &CosmeticsRecord, grant_all: bool) -> CaptainLook {
    let owned: Vec<u8> = if grant_all {
        (1..=COSMETICS.len() as u8).collect()
    } else {
        record
            .owned
            .iter()
            .filter_map(|id| cosmetic_code(id))
            .collect()
    };
    let mut worn = ShipCosmetics::default();
    for (slot, id) in [
        (CosmeticSlot::Sail, &record.sail),
        (CosmeticSlot::Flag, &record.flag),
    ] {
        let code = id.as_deref().and_then(cosmetic_code).unwrap_or(0);
        if worn.wear(&owned, slot, code).is_err() {
            warn!(?slot, "cosmético em uso não é mais do capitão: removido");
        }
    }
    CaptainLook {
        owned,
        worn,
        client: None,
        last_wear: None,
    }
}

fn snapshot(look: &CaptainLook) -> CosmeticsSnapshot {
    CosmeticsSnapshot {
        owned: look.owned.clone(),
        sail: look.worn.sail,
        flag: look.worn.flag,
    }
}

/// Capitão recém-conectado: carrega do store e manda o snapshot.
fn load_on_connect(
    ships: Query<&ServerShip>,
    store: Res<StoreHandle>,
    mut looks: ResMut<CaptainCosmetics>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    for ship in &ships {
        let Some(client_id) = ship.client_id else {
            continue;
        };
        if looks
            .0
            .get(&ship.character)
            .is_some_and(|look| look.client == Some(client_id))
        {
            continue;
        }
        let record = match &store.0 {
            Some(store) => store
                .load_cosmetics(ship.character)
                .unwrap_or_else(|error| {
                    warn!(%error, "cosméticos não carregaram: capitão sem visual");
                    CosmeticsRecord::default()
                }),
            None => CosmeticsRecord::default(),
        };
        let mut look = look_from_record(&record, dev_grants_all());
        look.client = Some(client_id);
        let _ = connection_manager.send_message::<ReliableChannel, _>(client_id, &snapshot(&look));
        looks.0.insert(ship.character, look);
    }
}

/// Troca de visual: só atracado (é no porto que se pinta o navio) e só o
/// que o capitão possui. A resposta é sempre o snapshot verdadeiro.
fn handle_wear(
    time: Res<Time>,
    mut events: EventReader<ServerReceiveMessage<WearCosmetic>>,
    ships: Query<&ServerShip>,
    store: Res<StoreHandle>,
    mut looks: ResMut<CaptainCosmetics>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    for event in events.read() {
        let client_id = event.from();
        let wear = *event.message();
        let Some(ship) = ships.iter().find(|s| s.client_id == Some(client_id)) else {
            continue;
        };
        let Some(look) = looks.0.get_mut(&ship.character) else {
            continue;
        };
        let slot = match wear.slot {
            0 => CosmeticSlot::Sail,
            1 => CosmeticSlot::Flag,
            _ => continue,
        };
        let now = time.elapsed_secs_f64();
        let is_docked = matches!(ship.presence, VesselPresence::Docked(_));
        let is_rested = look
            .last_wear
            .map_or(true, |last| now - last >= WEAR_COOLDOWN_SECS);
        let before = look.worn;
        match (is_docked && is_rested).then(|| look.worn.wear(&look.owned, slot, wear.code)) {
            Some(Ok(())) if look.worn != before => {
                look.last_wear = Some(now);
                info!(
                    ship_id = ship.ship_id,
                    ?slot,
                    code = wear.code,
                    "visual trocado"
                );
                if let Some(store) = &store.0 {
                    let id = |code: u8| cosmetic_by_code(code).map(|cosmetic| cosmetic.id);
                    if let Err(error) = store.save_cosmetic_choice(
                        ship.character,
                        id(look.worn.sail),
                        id(look.worn.flag),
                    ) {
                        warn!(%error, "visual não foi gravado");
                    }
                }
            }
            Some(Ok(())) => {}
            Some(Err(error)) => warn!(ship_id = ship.ship_id, %error, "troca de visual recusada"),
            None => warn!(
                ship_id = ship.ship_id,
                "troca de visual fora do porto ou rápida demais"
            ),
        }
        let _ = connection_manager.send_message::<ReliableChannel, _>(client_id, &snapshot(look));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_becomes_codes_and_drops_what_was_revoked() {
        let record = CosmeticsRecord {
            owned: vec!["sail-gold".into(), "nada-disso".into()],
            sail: Some("sail-gold".into()),
            // Revogada: estava em uso, não está mais concedida.
            flag: Some("flag-linen".into()),
        };
        let look = look_from_record(&record, false);
        let gold = cosmetic_code("sail-gold").unwrap();
        assert_eq!(look.owned, vec![gold]);
        assert_eq!(
            look.worn,
            ShipCosmetics {
                sail: gold,
                flag: 0
            }
        );
        let all = look_from_record(&CosmeticsRecord::default(), true);
        assert_eq!(all.owned.len(), COSMETICS.len());
        assert_eq!(all.worn, ShipCosmetics::default());
    }
}
