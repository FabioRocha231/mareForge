//! Reputação / notoriedade (MF-059): atacar quem é honesto fora das águas
//! protegidas deixa marca. A marca decai sozinha; acumulada, vira cabeça a
//! prêmio — a marinha caça, os portos da coroa fecham e quem afunda o
//! procurado recebe da coroa. Regras puras em cima, sistemas embaixo.
//!
//! ponytail: notoriedade vive só em memória — restart do servidor perdoa
//! todo mundo. Persistir junto do personagem quando a temporada importar.

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_economy::{LedgerKind, Money};
use mareforge_domain_world::RiskTier;
use mareforge_protocol::{
    ReputationUpdate, WorldEvent, WorldEventKind, TIER_HONRADO, TIER_PROCURADO, TIER_SUSPEITO,
};
use mareforge_shared::ids::CharacterId;
use tracing::info;

use crate::net::{
    CombatImpacts, PendingShipDestructions, ReliableChannel, ServerShip, ServerWorldMap,
};

pub const MAX_NOTORIETY: u32 = 1000;
/// Por impacto em caravana ou jogador honesto (águas Frontier).
pub const HIT_GAIN: u32 = 15;
/// Por afundamento de caravana ou jogador honesto (águas Frontier).
pub const SINK_GAIN: u32 = 120;
/// -1 de notoriedade a cada intervalo.
pub const DECAY_EVERY_SECS: f32 = 10.0;
pub const SUSPEITO_AT: u32 = 100;
pub const PROCURADO_AT: u32 = 300;
/// Ouro de recompensa por ponto de notoriedade de um Procurado.
pub const BOUNTY_PER_NOTORIETY: u64 = 2;
/// Quem ataca uma caravana fica marcado pela marinha por este tempo.
pub const CROWN_AGGRESSOR_SECS: f32 = 60.0;
/// Portos da coroa: recusam Procurados. Um porto pirata fica de fora da
/// lista e atraca qualquer um.
pub const CROWN_PORTS: [&str; 2] = ["Porto da Serra", "Porto da Mina"];
pub const CROWN_REFUSAL: &str = "Procurado: os portos da coroa recusam seu navio";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Honrado,
    Suspeito,
    Procurado,
}

impl Tier {
    pub fn of(notoriety: u32) -> Self {
        if notoriety >= PROCURADO_AT {
            Self::Procurado
        } else if notoriety >= SUSPEITO_AT {
            Self::Suspeito
        } else {
            Self::Honrado
        }
    }

    pub fn wire(self) -> u8 {
        match self {
            Self::Honrado => TIER_HONRADO,
            Self::Suspeito => TIER_SUSPEITO,
            Self::Procurado => TIER_PROCURADO,
        }
    }
}

/// Recompensa pela cabeça: só Procurado tem preço.
pub fn bounty_for(notoriety: u32) -> u64 {
    if Tier::of(notoriety) == Tier::Procurado {
        u64::from(notoriety) * BOUNTY_PER_NOTORIETY
    } else {
        0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offense {
    Hit,
    Sink,
}

/// Notoriedade ganha pelo agressor. `victim_is_outlaw` = pirata ou
/// Procurado: caçar fora-da-lei nunca suja o nome. A zona é a da VÍTIMA;
/// Lawless conta metade, Protected/fora do mapa nada (o dano nem entra).
pub fn notoriety_gain(
    offense: Offense,
    victim_zone: Option<RiskTier>,
    victim_is_outlaw: bool,
) -> u32 {
    if victim_is_outlaw {
        return 0;
    }
    let base = match offense {
        Offense::Hit => HIT_GAIN,
        Offense::Sink => SINK_GAIN,
    };
    match victim_zone {
        Some(RiskTier::Frontier) => base,
        Some(RiskTier::Lawless) => base / 2,
        _ => 0,
    }
}

/// Porto da coroa recusa Procurado; qualquer outro porto atraca.
pub fn dock_refusal(port: &str, notoriety: u32) -> Option<&'static str> {
    (Tier::of(notoriety) == Tier::Procurado && CROWN_PORTS.contains(&port)).then_some(CROWN_REFUSAL)
}

#[derive(Resource, Debug, Default)]
pub struct Reputation {
    notoriety: HashMap<CharacterId, u32>,
    /// Segundos restantes de marca de "atacou caravana" (marinha caça).
    crown_aggressors: HashMap<CharacterId, f32>,
    decay_clock: f32,
}

impl Reputation {
    pub fn notoriety(&self, character: CharacterId) -> u32 {
        self.notoriety.get(&character).copied().unwrap_or(0)
    }

    pub fn tier(&self, character: CharacterId) -> Tier {
        Tier::of(self.notoriety(character))
    }

    /// Soma (com teto) e devolve a faixa ANTERIOR para anúncio de subida.
    pub fn add(&mut self, character: CharacterId, amount: u32) -> Tier {
        let before = self.tier(character);
        if amount > 0 {
            let value = self.notoriety.entry(character).or_default();
            *value = (*value + amount).min(MAX_NOTORIETY);
        }
        before
    }

    /// Morte zera a ficha. Devolve o valor anterior.
    pub fn reset(&mut self, character: CharacterId) -> u32 {
        self.crown_aggressors.remove(&character);
        self.notoriety.remove(&character).unwrap_or(0)
    }

    pub fn mark_crown_aggressor(&mut self, character: CharacterId) {
        self.crown_aggressors
            .insert(character, CROWN_AGGRESSOR_SECS);
    }

    /// A marinha caça Procurados e quem atacou caravana há pouco.
    pub fn hunted_by_navy(&self, character: CharacterId) -> bool {
        self.tier(character) == Tier::Procurado || self.crown_aggressors.contains_key(&character)
    }

    /// Avança decaimento e marcas; devolve quem teve a notoriedade mudada.
    pub fn tick(&mut self, dt: f32) -> Vec<CharacterId> {
        self.crown_aggressors.retain(|_, secs| {
            *secs -= dt;
            *secs > 0.0
        });
        self.decay_clock += dt;
        if self.decay_clock < DECAY_EVERY_SECS {
            return Vec::new();
        }
        self.decay_clock -= DECAY_EVERY_SECS;
        let changed: Vec<CharacterId> = self.notoriety.keys().copied().collect();
        for value in self.notoriety.values_mut() {
            *value -= 1;
        }
        self.notoriety.retain(|_, value| *value > 0);
        changed
    }

    pub fn update_for(&self, character: CharacterId) -> ReputationUpdate {
        let notoriety = self.notoriety(character);
        ReputationUpdate {
            notoriety,
            tier: Tier::of(notoriety).wire(),
            bounty: bounty_for(notoriety),
        }
    }
}

pub(crate) fn send_reputation(
    connection_manager: &mut ConnectionManager,
    reputation: &Reputation,
    client_id: Option<ClientId>,
    character: CharacterId,
) {
    if let Some(client_id) = client_id {
        let _ = connection_manager
            .send_message::<ReliableChannel, _>(client_id, &reputation.update_for(character));
    }
}

pub(crate) fn send_event(
    connection_manager: &mut ConnectionManager,
    clients: &[ClientId],
    text: String,
    kind: WorldEventKind,
) {
    if clients.is_empty() {
        return;
    }
    let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
        &WorldEvent { text, kind },
        NetworkTarget::Only(clients.to_vec()),
    );
}

/// Subiu de faixa: o próprio capitão fica sabendo pelo feed.
pub(crate) fn announce_tier_rise(
    connection_manager: &mut ConnectionManager,
    reputation: &Reputation,
    client_id: Option<ClientId>,
    character: CharacterId,
    before: Tier,
) {
    let (Some(client_id), after) = (client_id, reputation.tier(character)) else {
        return;
    };
    if after <= before {
        return;
    }
    let (text, kind) = match after {
        Tier::Procurado => (
            format!(
                "CABECA A PRECO: {}g",
                bounty_for(reputation.notoriety(character))
            ),
            WorldEventKind::Bounty,
        ),
        _ => (
            String::from("A coroa agora o considera SUSPEITO"),
            WorldEventKind::Alert,
        ),
    };
    send_event(connection_manager, &[client_id], text, kind);
}

/// Soma notoriedade e avisa o dono (valor novo + subida de faixa).
pub(crate) fn raise_notoriety(
    connection_manager: &mut ConnectionManager,
    reputation: &mut Reputation,
    client_id: Option<ClientId>,
    character: CharacterId,
    gain: u32,
) {
    if gain == 0 {
        return;
    }
    let before = reputation.add(character, gain);
    send_reputation(connection_manager, reputation, client_id, character);
    announce_tier_rise(connection_manager, reputation, client_id, character, before);
}

fn zone_tier(map: &ServerWorldMap, x: f32, y: f32) -> Option<RiskTier> {
    map.0.zone_at(x, y).ok().map(|zone| zone.tier)
}

/// Impactos jogador→jogador ainda não aplicados (corre antes do dano, com a
/// ficha da vítima de antes do naufrágio).
pub fn track_player_hits(
    mut connection_manager: ResMut<ConnectionManager>,
    impacts: Res<CombatImpacts>,
    map: Res<ServerWorldMap>,
    ships: Query<&ServerShip>,
    mut reputation: ResMut<Reputation>,
) {
    for (_, target_ship_id, _, attacker_ship_id, _) in &impacts.0 {
        let find = |id: u32| ships.iter().find(|ship| ship.ship_id == id);
        let (Some(attacker), Some(victim)) = (find(*attacker_ship_id), find(*target_ship_id))
        else {
            continue;
        };
        if attacker.character == victim.character {
            continue;
        }
        let gain = notoriety_gain(
            Offense::Hit,
            zone_tier(&map, victim.motion.x, victim.motion.y),
            reputation.tier(victim.character) == Tier::Procurado,
        );
        raise_notoriety(
            &mut connection_manager,
            &mut reputation,
            attacker.client_id,
            attacker.character,
            gain,
        );
    }
}

/// Naufrágios de jogador: cobra a cabeça do Procurado (a coroa paga), soma
/// notoriedade a quem afundou honesto e zera a ficha de quem morreu.
pub fn settle_player_sinks(
    mut connection_manager: ResMut<ConnectionManager>,
    pending: Res<PendingShipDestructions>,
    map: Res<ServerWorldMap>,
    ships: Query<&ServerShip>,
    mut market: ResMut<crate::market::ServerMarket>,
    mut reputation: ResMut<Reputation>,
) {
    let viewers: Vec<(Option<ClientId>, CharacterId)> = ships
        .iter()
        .map(|ship| (ship.client_id, ship.character))
        .collect();
    for destruction in &pending.0 {
        let victim = destruction.victim_character;
        let victim_notoriety = reputation.notoriety(victim);
        let killer = destruction
            .exclusive_looter
            .filter(|killer| *killer != victim);
        if let Some(killer) = killer {
            let killer_client = viewers
                .iter()
                .find(|(_, owner)| *owner == killer)
                .and_then(|(client, _)| *client);
            let bounty = bounty_for(victim_notoriety);
            if bounty > 0 {
                claim_bounty(&mut market, killer, bounty);
                crate::market::send_wallet(&mut connection_manager, &market, &viewers, killer);
                if let Some(client) = killer_client {
                    send_event(
                        &mut connection_manager,
                        &[client],
                        format!("Recompensa da coroa: +{bounty}g"),
                        WorldEventKind::Kill,
                    );
                }
            } else {
                let gain = notoriety_gain(
                    Offense::Sink,
                    zone_tier(&map, destruction.victim_x, destruction.victim_y),
                    false,
                );
                if let Some(client) = killer_client {
                    send_event(
                        &mut connection_manager,
                        &[client],
                        String::from("Voce afundou um navio"),
                        WorldEventKind::Kill,
                    );
                }
                raise_notoriety(
                    &mut connection_manager,
                    &mut reputation,
                    killer_client,
                    killer,
                    gain,
                );
            }
        }
        if reputation.reset(victim) > 0 {
            send_reputation(
                &mut connection_manager,
                &reputation,
                destruction.victim_client_id,
                victim,
            );
            if Tier::of(victim_notoriety) == Tier::Procurado {
                if let Some(client) = destruction.victim_client_id {
                    send_event(
                        &mut connection_manager,
                        &[client],
                        String::from("Sua cabeca foi cobrada. Ficha limpa."),
                        WorldEventKind::Bounty,
                    );
                }
            }
        }
    }
}

/// A coroa paga a cabeça: faucet auditado no ledger (`BountyClaim`).
pub(crate) fn claim_bounty(
    market: &mut crate::market::ServerMarket,
    killer: CharacterId,
    bounty: u64,
) {
    market.credit(killer, Money(bounty));
    market.ledger.record(
        LedgerKind::BountyClaim,
        Money(bounty),
        format!("bounty claim {bounty}g"),
    );
    market.persist();
    info!(killer = ?killer, bounty, "cabeca de Procurado cobrada");
}

pub fn decay_notoriety(
    time: Res<Time>,
    mut connection_manager: ResMut<ConnectionManager>,
    ships: Query<&ServerShip>,
    mut reputation: ResMut<Reputation>,
) {
    for character in reputation.tick(time.delta_secs()) {
        if let Some(ship) = ships.iter().find(|ship| ship.character == character) {
            send_reputation(
                &mut connection_manager,
                &reputation,
                ship.client_id,
                character,
            );
        }
    }
}

/// Navio novo (hello, respawn, reconnect): o dono recebe a ficha atual.
/// Dev tooling (PRD §39): `MAREFORGE_DEV_NOTORIETY=N` faz todo capitão
/// novo nascer com N de notoriedade — revisar selo, marca e marinha sem
/// afundar caravana. Não é mecânica de jogo.
pub fn announce_reputation_on_spawn(
    mut connection_manager: ResMut<ConnectionManager>,
    ships: Query<&ServerShip, Added<ServerShip>>,
    mut reputation: ResMut<Reputation>,
) {
    let dev_notoriety: Option<u32> = std::env::var("MAREFORGE_DEV_NOTORIETY")
        .ok()
        .and_then(|value| value.parse().ok());
    for ship in &ships {
        if let Some(value) = dev_notoriety.filter(|_| reputation.notoriety(ship.character) == 0) {
            reputation.add(ship.character, value);
        }
        send_reputation(
            &mut connection_manager,
            &reputation,
            ship.client_id,
            ship.character,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_split_at_100_and_300() {
        assert_eq!(Tier::of(0), Tier::Honrado);
        assert_eq!(Tier::of(99), Tier::Honrado);
        assert_eq!(Tier::of(100), Tier::Suspeito);
        assert_eq!(Tier::of(299), Tier::Suspeito);
        assert_eq!(Tier::of(300), Tier::Procurado);
        assert_eq!(bounty_for(299), 0);
        assert_eq!(bounty_for(320), 640);
    }

    #[test]
    fn gains_are_full_in_frontier_half_in_lawless_and_never_on_outlaws() {
        use Offense::*;
        assert_eq!(notoriety_gain(Hit, Some(RiskTier::Frontier), false), 15);
        assert_eq!(notoriety_gain(Sink, Some(RiskTier::Frontier), false), 120);
        assert_eq!(notoriety_gain(Hit, Some(RiskTier::Lawless), false), 7);
        assert_eq!(notoriety_gain(Sink, Some(RiskTier::Lawless), false), 60);
        assert_eq!(notoriety_gain(Hit, Some(RiskTier::Protected), false), 0);
        assert_eq!(notoriety_gain(Hit, None, false), 0);
        assert_eq!(notoriety_gain(Sink, Some(RiskTier::Frontier), true), 0);
    }

    #[test]
    fn notoriety_caps_decays_and_resets() {
        let mut rep = Reputation::default();
        let captain = CharacterId::new();
        assert_eq!(rep.add(captain, 250), Tier::Honrado);
        assert_eq!(rep.add(captain, 120), Tier::Suspeito);
        assert_eq!(rep.tier(captain), Tier::Procurado);
        rep.add(captain, 5_000);
        assert_eq!(rep.notoriety(captain), MAX_NOTORIETY);

        assert!(rep.tick(DECAY_EVERY_SECS - 0.5).is_empty());
        assert_eq!(rep.tick(1.0), vec![captain]);
        assert_eq!(rep.notoriety(captain), MAX_NOTORIETY - 1);

        assert_eq!(rep.reset(captain), MAX_NOTORIETY - 1);
        assert_eq!(rep.notoriety(captain), 0);
        assert_eq!(rep.update_for(captain).bounty, 0);
    }

    #[test]
    fn decay_forgets_captains_at_zero() {
        let mut rep = Reputation::default();
        let captain = CharacterId::new();
        rep.add(captain, 1);
        rep.tick(DECAY_EVERY_SECS);
        assert_eq!(rep.notoriety(captain), 0);
        assert!(rep.tick(DECAY_EVERY_SECS).is_empty());
    }

    #[test]
    fn crown_ports_refuse_wanted_captains_only() {
        assert_eq!(dock_refusal("Porto da Serra", 300), Some(CROWN_REFUSAL));
        assert_eq!(dock_refusal("Porto da Mina", 999), Some(CROWN_REFUSAL));
        assert_eq!(dock_refusal("Porto da Serra", 299), None);
        // Porto fora da coroa (ex.: porto pirata) atraca qualquer um.
        assert_eq!(dock_refusal("Porto do Coral Negro", 999), None);
    }

    #[test]
    fn navy_hunts_wanted_and_recent_caravan_aggressors() {
        let mut rep = Reputation::default();
        let honest = CharacterId::new();
        let wanted = CharacterId::new();
        let raider = CharacterId::new();
        rep.add(wanted, PROCURADO_AT);
        rep.mark_crown_aggressor(raider);
        assert!(!rep.hunted_by_navy(honest));
        assert!(rep.hunted_by_navy(wanted));
        assert!(rep.hunted_by_navy(raider));
        rep.tick(CROWN_AGGRESSOR_SECS + 1.0);
        assert!(!rep.hunted_by_navy(raider));
    }

    #[test]
    fn claiming_a_bounty_pays_the_killer_from_the_crown() {
        let mut market = crate::market::ServerMarket::new();
        let killer = market.character("hunter");
        let start = market.balance(killer).0;
        claim_bounty(&mut market, killer, 640);
        assert_eq!(market.balance(killer).0, start + 640);
        assert!(market
            .ledger
            .entries()
            .iter()
            .any(|entry| entry.kind == LedgerKind::BountyClaim && entry.amount == Money(640)));
    }
}
