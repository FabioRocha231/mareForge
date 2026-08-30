//! protocol: tipos de wire do mareForge (PRD §63/§64).
//!
//! Tipos puros com serde — o transporte (lightyear, ADR-0002) escolhe o
//! formato na camada dele; este crate define apenas o **contrato** entre
//! client e servidor. Versionamento no handshake conforme ADR-0011.

use mareforge_domain_combat::weapon::BroadsideSide;
use mareforge_domain_world::RiskTier;
use serde::{Deserialize, Serialize};

/// Versão atual do protocolo. Qualquer mudança incompatível deve incrementar
/// este número (semântica: quebra de contrato = +1).
///
/// v2: combate — FireBroadside, ShipDestroyed e projéteis no snapshot.
/// v3: economia do naufrágio — cargo_weight no ShipState, ciclo de vida de
///     Wreck (WreckSpawned/WreckRemoved) e loot (LootWreck/LootResult).
/// v4: geografia de risco (MF-017) — ZoneChanged com tier e nome da zona;
///     o servidor é quem calcula a zona real (PRD §10), a UI representa.
/// v5: economia de recursos (MF-018/019) — nós visíveis (NodesSnapshot no
///     hello, NodeUpdated em toda mudança), coleta GatherNode/GatherResult.
pub const PROTOCOL_VERSION: u16 = 5;

/// Primeira mensagem do client após conectar (ADR-0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientHello {
    pub protocol_version: u16,
}

impl ClientHello {
    pub fn current() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
        }
    }
}

/// Resposta do servidor. Conexão com versão incompatível é rejeitada
/// (`accepted == false`) e encerrada em seguida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerWelcome {
    pub protocol_version: u16,
    pub accepted: bool,
}

/// O servidor atribui um navio ao jogador aceito (janela com visão própria).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignShip {
    pub ship_id: u32,
}

/// Intenção de navegação do jogador (PRD §63: ShipInput).
/// Campos espelham `MotionInput` de `domain-ships`; o servidor é quem
/// valida e aplica (Pilar 4).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShipInput {
    /// [0, 1] — velas.
    pub throttle: f32,
    /// [-1, 1] — leme; positivo = bombordo (anti-horário).
    pub turn: f32,
}

/// Estado autoritativo de um navio no tick do snapshot (PRD §64: ShipState).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShipState {
    pub ship_id: u32,
    pub x: f32,
    pub y: f32,
    /// Radianos, 0 = +X, anti-horário (convenção de `domain-ships`).
    pub heading: f32,
    /// m/s.
    pub speed: f32,
    /// Peso atual da carga (unidades de peso; o limite é do navio).
    pub cargo_weight: u32,
}

/// Comando de disparo de bordo do jogador (PRD §19). Confiável: é um clique,
/// perder um tiro é perder gameplay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FireBroadside {
    pub side: BroadsideSide,
}

/// Estado autoritativo de um projétil no tick do snapshot (PRD §20).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectileState {
    pub projectile_id: u32,
    pub x: f32,
    pub y: f32,
    /// Direção de voo em radianos.
    pub heading: f32,
}

/// Navio afundou (PRD §21). O servidor retransmite a todos; a resolução de
/// loot acontece do lado do servidor (MF-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipDestroyed {
    pub ship_id: u32,
}

/// Wreck surgiu no mar com a carga que sobreviveu ao naufrágio (PRD §26).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WreckSpawned {
    pub wreck_id: u32,
    pub x: f32,
    pub y: f32,
    /// Quantidade de pilhas de itens dentro do baú (para UI).
    pub stack_count: u32,
}

/// Wreck desapareceu (saqueado até esvaziar ou expirado — PRD §26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WreckRemoved {
    pub wreck_id: u32,
}

/// Jogador quer saquear um wreck (PRD §27: precisa estar nele, com porão).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootWreck {
    pub wreck_id: u32,
}

/// Resultado da tentativa de saque (MF-015: atômico, capacity-aware).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootResult {
    pub wreck_id: u32,
    pub success: bool,
}

/// A zona real do navio mudou (PRD §10, MF-017). O servidor define a zona;
/// o client apenas representa. Enviado por canal confiável para o dono do
/// navio — no spawn (estado inicial) e a cada travessia de fronteira.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneChanged {
    pub ship_id: u32,
    pub tier: RiskTier,
    /// Nome declarado da zona no mapa do servidor (ex.: "Rota da Costa").
    pub zone_name: String,
}

/// Estado visível de um nó de recurso (PRD §64: ResourceNodeUpdated,
/// MF-018). Nós são server-authoritative: o client desenha e pede.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeState {
    /// Id numérico do nó (protocolo usa u32, como wrecks e navios).
    pub node_id: u32,
    pub x: f32,
    pub y: f32,
    /// Nome do recurso para a UI (display name do catálogo do servidor).
    pub resource_name: String,
    /// Unidades disponíveis agora (0 = esgotado, aguardando respawn).
    pub stock: u32,
    pub max_stock: u32,
}

/// Todos os nós do mundo, enviados no handshake — depois, só deltas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodesSnapshot {
    pub nodes: Vec<NodeState>,
}

/// Um nó mudou (coleta de outro jogador ou respawn). Mesmo formato do
/// estado: o client substitui o que sabe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeUpdated {
    pub node: NodeState,
}

/// Jogador quer coletar um nó (PRD MF-019: perto, com estoque e porão).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatherNode {
    pub node_id: u32,
}

/// Resultado da tentativa de coleta (MF-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatherResult {
    pub node_id: u32,
    pub success: bool,
    /// Unidades efetivamente coletadas (0 quando falha).
    pub gathered: u32,
}

/// Snapshot do mundo visível; enviado por tick (PRD §64/§66 — AOI corta o
/// escopo por proximidade quando entrar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub tick: u64,
    pub ships: Vec<ShipState>,
    pub projectiles: Vec<ProjectileState>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_protocol_version_is_five() {
        assert_eq!(PROTOCOL_VERSION, 5);
        assert_eq!(ClientHello::current().protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn client_hello_roundtrips() {
        let message = ClientHello::current();
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: ClientHello = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn server_welcome_rejects_old_version() {
        let message = ServerWelcome {
            protocol_version: 99,
            accepted: false,
        };
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: ServerWelcome = bincode::deserialize(&bytes).unwrap();
        assert!(!decoded.accepted);
        assert_ne!(decoded.protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn ship_input_roundtrips_and_clamps_are_server_side() {
        let message = ShipInput {
            throttle: 0.75,
            turn: -0.5,
        };
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: ShipInput = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn world_snapshot_roundtrips_with_ships_and_projectiles() {
        let message = WorldSnapshot {
            tick: 42,
            ships: vec![
                ShipState {
                    ship_id: 1,
                    x: 12.5,
                    y: -3.25,
                    heading: 0.1,
                    speed: 4.0,
                    cargo_weight: 36,
                },
                ShipState {
                    ship_id: 2,
                    x: 0.0,
                    y: 0.0,
                    heading: 3.0,
                    speed: 0.0,
                    cargo_weight: 0,
                },
            ],
            projectiles: vec![ProjectileState {
                projectile_id: 7,
                x: 1.0,
                y: 2.0,
                heading: 1.5,
            }],
        };
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: WorldSnapshot = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn wreck_lifecycle_messages_roundtrip() {
        let spawned = WreckSpawned {
            wreck_id: 9,
            x: -4.0,
            y: 8.5,
            stack_count: 2,
        };
        let bytes = bincode::serialize(&spawned).unwrap();
        assert_eq!(
            bincode::deserialize::<WreckSpawned>(&bytes).unwrap(),
            spawned
        );

        let removed = WreckRemoved { wreck_id: 9 };
        let bytes = bincode::serialize(&removed).unwrap();
        assert_eq!(
            bincode::deserialize::<WreckRemoved>(&bytes).unwrap(),
            removed
        );

        let loot = LootWreck { wreck_id: 9 };
        let bytes = bincode::serialize(&loot).unwrap();
        assert_eq!(bincode::deserialize::<LootWreck>(&bytes).unwrap(), loot);

        let result = LootResult {
            wreck_id: 9,
            success: true,
        };
        let bytes = bincode::serialize(&result).unwrap();
        assert_eq!(bincode::deserialize::<LootResult>(&bytes).unwrap(), result);
    }

    #[test]
    fn fire_broadside_roundtrips_both_sides() {
        for side in [
            mareforge_domain_combat::weapon::BroadsideSide::Port,
            mareforge_domain_combat::weapon::BroadsideSide::Starboard,
        ] {
            let message = FireBroadside { side };
            let bytes = bincode::serialize(&message).unwrap();
            let decoded: FireBroadside = bincode::deserialize(&bytes).unwrap();
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn ship_destroyed_roundtrips() {
        let message = ShipDestroyed { ship_id: 3 };
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: ShipDestroyed = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn zone_changed_roundtrips_with_tier_and_name() {
        for tier in [
            mareforge_domain_world::RiskTier::Protected,
            mareforge_domain_world::RiskTier::Frontier,
            mareforge_domain_world::RiskTier::Lawless,
        ] {
            let message = ZoneChanged {
                ship_id: 7,
                tier,
                zone_name: String::from("Rota da Costa"),
            };
            let bytes = bincode::serialize(&message).unwrap();
            let decoded: ZoneChanged = bincode::deserialize(&bytes).unwrap();
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn node_messages_roundtrip() {
        let state = NodeState {
            node_id: 4,
            x: -430.0,
            y: 20.0,
            resource_name: String::from("Madeira"),
            stock: 50,
            max_stock: 60,
        };
        let snapshot = NodesSnapshot {
            nodes: vec![
                state.clone(),
                NodeState {
                    node_id: 5,
                    x: 0.0,
                    y: 900.0,
                    resource_name: String::from("Coral Negro"),
                    stock: 0,
                    max_stock: 30,
                },
            ],
        };
        let bytes = bincode::serialize(&snapshot).unwrap();
        let decoded: NodesSnapshot = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, snapshot);
        assert_eq!(decoded.nodes[1].stock, 0);

        let updated = NodeUpdated { node: state };
        let bytes = bincode::serialize(&updated).unwrap();
        assert_eq!(
            bincode::deserialize::<NodeUpdated>(&bytes).unwrap(),
            updated
        );

        let gather = GatherNode { node_id: 4 };
        let bytes = bincode::serialize(&gather).unwrap();
        assert_eq!(bincode::deserialize::<GatherNode>(&bytes).unwrap(), gather);

        let result = GatherResult {
            node_id: 4,
            success: true,
            gathered: 10,
        };
        let bytes = bincode::serialize(&result).unwrap();
        assert_eq!(
            bincode::deserialize::<GatherResult>(&bytes).unwrap(),
            result
        );
    }
}
