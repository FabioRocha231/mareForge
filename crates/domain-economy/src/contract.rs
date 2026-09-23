//! Quadro de Contratos por porto: gerador puro e determinístico (seed) e o
//! progresso de um contrato ativo. Recompensa é faucet auditado
//! (`LedgerKind::ContractReward`); itens entregues são consumidos (sink).

use marvyr_shared::ItemInstanceId;

use crate::guild::{guild_value, GUILD_BASE_VALUES};

pub const OFFERS_PER_PORT: usize = 3;
pub const DELIVERY_DURATION_SECS: f64 = 15.0 * 60.0;
pub const HUNT_DURATION_SECS: f64 = 10.0 * 60.0;
pub const HUNT_REWARD_PER_KILL: u64 = 150;
/// Valor-alvo de um lote de entrega: define N pela base do item.
const DELIVERY_LOT_VALUE: u64 = 250;
/// Ouro de bônus por unidade de distância entre os portos.
const DISTANCE_BONUS_PER_UNIT: f64 = 0.1;

#[derive(Debug, Clone, PartialEq)]
pub enum ContractKind {
    /// Leve `quantity` x `item` de `from` para `to` (no porão, não storage).
    Delivery {
        item: String,
        quantity: u32,
        from: String,
        to: String,
    },
    /// Afunde `kills` navios NPC.
    Hunt { kills: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    pub id: u32,
    pub kind: ContractKind,
    pub reward: u64,
    pub duration_secs: f64,
}

impl Contract {
    /// Texto de exibição (ASCII fora dos nomes de item/porto).
    pub fn title(&self) -> String {
        match &self.kind {
            ContractKind::Delivery {
                item,
                quantity,
                from,
                to,
            } => format!("Entrega: {quantity}x {item} de {from} para {to}"),
            ContractKind::Hunt { kills } => format!("Caca: afunde {kills} navio(s) pirata(s)"),
        }
    }

    pub fn target(&self) -> u32 {
        match self.kind {
            ContractKind::Delivery { quantity, .. } => quantity,
            ContractKind::Hunt { kills } => kills,
        }
    }
}

/// Um porto para o gerador: nome e posição (bônus de distância).
#[derive(Debug, Clone, Copy)]
pub struct PortSite<'a> {
    pub name: &'a str,
    pub x: f32,
    pub y: f32,
}

/// xorshift64 — determinístico, sem dependência.
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Gera as ofertas do porto `here`. Mesmo seed → mesmas ofertas (testes).
/// `first_id` é o primeiro id livre; ids crescem de 1 em 1.
pub fn generate_offers(
    here: &PortSite,
    ports: &[PortSite],
    seed: u64,
    first_id: u32,
) -> Vec<Contract> {
    let mut state = seed | 1;
    let destinations: Vec<&PortSite> = ports.iter().filter(|port| port.name != here.name).collect();
    (0..OFFERS_PER_PORT)
        .map(|index| {
            let id = first_id + index as u32;
            let roll = next(&mut state);
            if destinations.is_empty() || roll.is_multiple_of(3) {
                let kills = 1 + (next(&mut state) % 3) as u32;
                return Contract {
                    id,
                    kind: ContractKind::Hunt { kills },
                    reward: HUNT_REWARD_PER_KILL * u64::from(kills),
                    duration_secs: HUNT_DURATION_SECS,
                };
            }
            let to = destinations[(next(&mut state) % destinations.len() as u64) as usize];
            let (item, base) =
                GUILD_BASE_VALUES[(next(&mut state) % GUILD_BASE_VALUES.len() as u64) as usize];
            let quantity = (DELIVERY_LOT_VALUE / base).clamp(1, 30) as u32;
            let value = guild_value(to.name, item).expect("item vem da tabela da guilda");
            let distance = ((to.x - here.x).hypot(to.y - here.y)) as f64;
            Contract {
                id,
                kind: ContractKind::Delivery {
                    item: item.to_owned(),
                    quantity,
                    from: here.name.to_owned(),
                    to: to.name.to_owned(),
                },
                reward: (value * f64::from(quantity) * 1.5 + distance * DISTANCE_BONUS_PER_UNIT)
                    .round() as u64,
                duration_secs: DELIVERY_DURATION_SECS,
            }
        })
        .collect()
}

/// Contrato aceito por um jogador (1 por vez).
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveContract {
    pub contract: Contract,
    pub deadline_secs: f64,
    /// Abates contados (Caça).
    pub kills: u32,
    /// Entrega: pilhas do item que estavam no porão no aceite, com a
    /// quantidade de então. Só elas contam na chegada — comprar ou coletar
    /// no destino não cumpre o contrato.
    pub consignment: Vec<(ItemInstanceId, u32)>,
}

impl ActiveContract {
    /// `hold` = pilhas do item da entrega no porão agora. Entrega sem a
    /// carga completa a bordo é recusada (`None`); Caça ignora o porão.
    pub fn accept(
        contract: Contract,
        now_secs: f64,
        hold: &[(ItemInstanceId, u32)],
    ) -> Option<Self> {
        if let ContractKind::Delivery { quantity, .. } = contract.kind {
            if hold.iter().map(|(_, qty)| qty).sum::<u32>() < quantity {
                return None;
            }
        }
        let consignment = match contract.kind {
            ContractKind::Delivery { .. } => hold.to_vec(),
            ContractKind::Hunt { .. } => Vec::new(),
        };
        Some(Self {
            deadline_secs: now_secs + contract.duration_secs,
            contract,
            kills: 0,
            consignment,
        })
    }

    pub fn remaining_secs(&self, now_secs: f64) -> f64 {
        (self.deadline_secs - now_secs).max(0.0)
    }

    pub fn expired(&self, now_secs: f64) -> bool {
        now_secs >= self.deadline_secs
    }

    /// Conta um abate; `true` quando a Caça fica completa.
    pub fn record_kill(&mut self) -> bool {
        match self.contract.kind {
            ContractKind::Hunt { kills } => {
                self.kills += 1;
                self.kills >= kills
            }
            ContractKind::Delivery { .. } => false,
        }
    }

    /// Consignação só encolhe: pilha que desceu (depositada, vendida,
    /// saqueada) não volta a contar se for reabastecida depois — senão dava
    /// para zarpar com 1 e completar no destino numa pilha de mesmo id.
    pub fn observe_hold(&mut self, hold: &[(ItemInstanceId, u32)]) {
        for (id, counted) in &mut self.consignment {
            let now = hold
                .iter()
                .find(|(held, _)| held == id)
                .map_or(0, |(_, qty)| *qty);
            *counted = (*counted).min(now);
        }
    }

    /// Entrega pronta: atracado no destino com ≥N da carga consignada no
    /// porão. Pilha que cresceu depois do aceite conta só até o que tinha.
    pub fn delivery_ready(&self, docked_port: &str, hold: &[(ItemInstanceId, u32)]) -> bool {
        let ContractKind::Delivery { quantity, to, .. } = &self.contract.kind else {
            return false;
        };
        let carried: u32 = self
            .consignment
            .iter()
            .map(|(id, at_accept)| {
                hold.iter()
                    .find(|(held, _)| held == id)
                    .map_or(0, |(_, now)| (*now).min(*at_accept))
            })
            .sum();
        docked_port == to && carried >= *quantity
    }

    pub fn progress(&self) -> u32 {
        match self.contract.kind {
            ContractKind::Hunt { .. } => self.kills,
            ContractKind::Delivery { .. } => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports() -> Vec<PortSite<'static>> {
        vec![
            PortSite {
                name: "Porto da Serra",
                x: -600.0,
                y: 0.0,
            },
            PortSite {
                name: "Porto da Mina",
                x: 600.0,
                y: 0.0,
            },
        ]
    }

    #[test]
    fn generator_is_deterministic_and_well_formed() {
        let ports = ports();
        let a = generate_offers(&ports[0], &ports, 42, 10);
        let b = generate_offers(&ports[0], &ports, 42, 10);
        assert_eq!(a, b);
        assert_eq!(a.len(), OFFERS_PER_PORT);
        assert_eq!(a.iter().map(|c| c.id).collect::<Vec<_>>(), vec![10, 11, 12]);
        for contract in &a {
            assert!(contract.reward > 0);
            if let ContractKind::Delivery { from, to, .. } = &contract.kind {
                assert_eq!(from, "Porto da Serra");
                assert_eq!(to, "Porto da Mina");
            }
        }
        // Seeds diferentes variam o quadro (em algum ponto de 20 tentativas).
        assert!((0..20).any(|seed| generate_offers(&ports[0], &ports, seed, 10) != a));
    }

    #[test]
    fn delivery_reward_uses_destination_value_and_distance() {
        let ports = ports();
        let delivery = (0..200)
            .flat_map(|seed| generate_offers(&ports[0], &ports, seed, 0))
            .find(|c| matches!(&c.kind, ContractKind::Delivery { item, .. } if item == "Madeira"))
            .expect("alguma seed gera entrega de Madeira");
        // 25 Madeira × 16g (Mina paga 1.6x) × 1.5 + 1200 × 0.1 = 720.
        assert_eq!(delivery.target(), 25);
        assert_eq!(delivery.reward, 720);
    }

    #[test]
    fn single_port_only_offers_hunts() {
        let ports = ports();
        let offers = generate_offers(&ports[0], &ports[..1], 7, 0);
        assert!(offers
            .iter()
            .all(|c| matches!(c.kind, ContractKind::Hunt { .. })));
    }

    #[test]
    fn active_contract_expires_counts_kills_and_checks_delivery() {
        let hunt = Contract {
            id: 1,
            kind: ContractKind::Hunt { kills: 2 },
            reward: 300,
            duration_secs: 60.0,
        };
        let mut active = ActiveContract::accept(hunt, 100.0, &[]).unwrap();
        assert_eq!(active.remaining_secs(130.0), 30.0);
        assert!(!active.expired(159.0));
        assert!(active.expired(160.0));
        assert!(!active.record_kill());
        assert!(active.record_kill());

        let (a, b, local) = (
            ItemInstanceId::new(),
            ItemInstanceId::new(),
            ItemInstanceId::new(),
        );
        let madeira = Contract {
            id: 2,
            kind: ContractKind::Delivery {
                item: String::from("Madeira"),
                quantity: 10,
                from: String::from("Porto da Serra"),
                to: String::from("Porto da Mina"),
            },
            reward: 100,
            duration_secs: 60.0,
        };
        // Sem a carga a bordo não dá para aceitar.
        assert!(ActiveContract::accept(madeira.clone(), 0.0, &[(a, 9)]).is_none());
        let delivery = ActiveContract::accept(madeira, 0.0, &[(a, 6), (b, 4)]).unwrap();
        assert!(delivery.delivery_ready("Porto da Mina", &[(a, 6), (b, 4)]));
        assert!(!delivery.delivery_ready("Porto da Mina", &[(a, 6), (b, 3)]));
        assert!(!delivery.delivery_ready("Porto da Serra", &[(a, 6), (b, 4)]));
        // Comprado/coletado no destino não conta; pilha que cresceu, só até o aceite.
        assert!(!delivery.delivery_ready("Porto da Mina", &[(a, 6), (local, 50)]));
        assert!(!delivery.delivery_ready("Porto da Mina", &[(a, 50), (b, 3)]));
        // Zarpou com a pilha reduzida e reabasteceu no destino: não conta.
        let mut delivery = delivery;
        delivery.observe_hold(&[(a, 1), (b, 4)]);
        assert!(!delivery.delivery_ready("Porto da Mina", &[(a, 6), (b, 4)]));
    }
}
