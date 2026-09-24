//! Renome no servidor (MV-067). Quem faz algo no mar dispara
//! [`RenownEarned`]; aqui o total sobe, o capitão recebe o `RenownUpdate` e
//! o store grava em lote (um feito por unidade coletada não pode virar uma
//! escrita no banco).

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use marvyr_domain_economy::renown::{level_of, progress};
use marvyr_protocol::RenownUpdate;
use marvyr_shared::ids::CharacterId;
use tracing::{info, warn};

use crate::net::{ReliableChannel, ServerShip};
use crate::persist::StoreHandle;
use crate::sets::SimulationSet;

/// Um feito que rende Renome. `reason` é PT-BR (o client traduz).
#[derive(Event, Debug, Clone, Copy)]
pub struct RenownEarned {
    pub character: CharacterId,
    pub amount: u32,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Default)]
struct Captain {
    total: u64,
    /// Conexão que já recebeu o total (reconexão recarrega do store).
    client: Option<ClientId>,
    is_dirty: bool,
}

#[derive(Resource, Default)]
pub struct CaptainRenown {
    captains: HashMap<CharacterId, Captain>,
    save_clock: f32,
}

impl CaptainRenown {
    pub fn total(&self, character: CharacterId) -> u64 {
        self.captains.get(&character).map_or(0, |c| c.total)
    }

    pub fn level(&self, character: CharacterId) -> u32 {
        level_of(self.total(character))
    }

    /// Soma o feito e marca para gravar. Devolve o total novo e se subiu
    /// de nível.
    fn credit(&mut self, character: CharacterId, amount: u32) -> (u64, bool) {
        let captain = self.captains.entry(character).or_default();
        let before = level_of(captain.total);
        captain.total = captain.total.saturating_add(u64::from(amount));
        captain.is_dirty |= amount > 0;
        (captain.total, level_of(captain.total) > before)
    }
}

/// ponytail: grava a cada 5 s — crash perde no máximo isso de Renome.
const SAVE_EVERY: f32 = 5.0;

pub fn install(app: &mut App) {
    app.init_resource::<CaptainRenown>()
        .add_event::<RenownEarned>();
    // Depois das consequências do tick, onde os feitos são disparados.
    app.add_systems(
        FixedUpdate,
        (load_on_connect, apply_earned)
            .chain()
            .in_set(SimulationSet::Telemetry),
    );
    app.add_systems(FixedUpdate, save_renown.in_set(SimulationSet::Persistence));
}

fn update(total: u64, gained: u32, reason: &str) -> RenownUpdate {
    let p = progress(total);
    RenownUpdate {
        total,
        level: p.level,
        into: p.into,
        span: p.span,
        gained,
        reason: reason.to_owned(),
    }
}

fn load_on_connect(
    ships: Query<&ServerShip>,
    store: Res<StoreHandle>,
    mut renown: ResMut<CaptainRenown>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    for ship in &ships {
        let Some(client_id) = ship.client_id else {
            continue;
        };
        let known = renown.captains.get(&ship.character);
        if known.is_some_and(|c| c.client == Some(client_id)) {
            continue;
        }
        // Reconexão sem gravar ainda: a memória é mais nova que o banco.
        let total = match known {
            Some(captain) if captain.is_dirty => captain.total,
            _ => store
                .0
                .as_ref()
                .map(|store| {
                    store.load_renown(ship.character).unwrap_or_else(|error| {
                        warn!(%error, "renome não carregou: começa do zero na sessão");
                        0
                    })
                })
                .unwrap_or(0),
        };
        let captain = renown.captains.entry(ship.character).or_default();
        captain.total = total;
        captain.client = Some(client_id);
        let _ =
            connection_manager.send_message::<ReliableChannel, _>(client_id, &update(total, 0, ""));
    }
}

fn apply_earned(
    mut events: EventReader<RenownEarned>,
    ships: Query<&ServerShip>,
    mut renown: ResMut<CaptainRenown>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    for earned in events.read() {
        if earned.amount == 0 {
            continue;
        }
        let (total, leveled) = renown.credit(earned.character, earned.amount);
        if leveled {
            info!(character = ?earned.character, level = level_of(total), "capitão subiu de Renome");
        }
        let client = ships
            .iter()
            .find(|ship| ship.character == earned.character)
            .and_then(|ship| ship.client_id);
        if let Some(client_id) = client {
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &update(total, earned.amount, earned.reason),
            );
        }
    }
}

fn save_renown(time: Res<Time>, store: Res<StoreHandle>, mut renown: ResMut<CaptainRenown>) {
    renown.save_clock += time.delta_secs();
    if renown.save_clock < SAVE_EVERY {
        return;
    }
    renown.save_clock = 0.0;
    let dirty: Vec<(CharacterId, u64)> = renown
        .captains
        .iter()
        .filter(|(_, captain)| captain.is_dirty)
        .map(|(character, captain)| (*character, captain.total))
        .collect();
    if dirty.is_empty() {
        return;
    }
    if let Some(store) = &store.0 {
        if let Err(error) = store.save_renown(&dirty) {
            warn!(%error, "renome não foi gravado; tenta de novo no próximo ciclo");
            return;
        }
    }
    for (character, _) in dirty {
        if let Some(captain) = renown.captains.get_mut(&character) {
            captain.is_dirty = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use marvyr_domain_economy::renown::threshold;

    #[test]
    fn credit_adds_up_marks_dirty_and_reports_level_ups() {
        let mut renown = CaptainRenown::default();
        let character = CharacterId::new();
        assert_eq!(renown.credit(character, 0), (0, false));
        assert!(!renown.captains[&character].is_dirty);
        let (total, leveled) = renown.credit(character, 30);
        assert_eq!((total, leveled), (30, false));
        let to_next = u32::try_from(threshold(2) - 30).unwrap();
        assert_eq!(renown.credit(character, to_next), (threshold(2), true));
        assert_eq!(renown.level(character), 2);
        assert!(renown.captains[&character].is_dirty);
    }
}
