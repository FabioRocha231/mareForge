//! Quadro de Contratos por porto: gerador puro e determinístico (seed) e o
//! progresso de um contrato ativo. Recompensa é faucet auditado
//! (`LedgerKind::ContractReward`); itens entregues são consumidos (sink).

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
}

impl ActiveContract {
    pub fn accept(contract: Contract, now_secs: f64) -> Self {
        Self {
            deadline_secs: now_secs + contract.duration_secs,
            contract,
            kills: 0,
        }
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

    /// Entrega pronta: atracado no destino com ≥N no PORÃO.
    pub fn delivery_ready(&self, docked_port: &str, item_in_hold: impl Fn(&str) -> u32) -> bool {
        match &self.contract.kind {
            ContractKind::Delivery {
                item, quantity, to, ..
            } => docked_port == to && item_in_hold(item) >= *quantity,
            ContractKind::Hunt { .. } => false,
        }
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
        let mut active = ActiveContract::accept(hunt, 100.0);
        assert_eq!(active.remaining_secs(130.0), 30.0);
        assert!(!active.expired(159.0));
        assert!(active.expired(160.0));
        assert!(!active.record_kill());
        assert!(active.record_kill());

        let delivery = ActiveContract::accept(
            Contract {
                id: 2,
                kind: ContractKind::Delivery {
                    item: String::from("Madeira"),
                    quantity: 10,
                    from: String::from("Porto da Serra"),
                    to: String::from("Porto da Mina"),
                },
                reward: 100,
                duration_secs: 60.0,
            },
            0.0,
        );
        assert!(delivery.delivery_ready("Porto da Mina", |_| 10));
        assert!(!delivery.delivery_ready("Porto da Mina", |_| 9));
        assert!(!delivery.delivery_ready("Porto da Serra", |_| 99));
    }
}
