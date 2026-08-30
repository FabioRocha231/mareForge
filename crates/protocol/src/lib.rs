//! protocol: tipos de wire do mareForge (PRD §63/§64).
//!
//! Tipos puros com serde — o transporte (lightyear, ADR-0002) escolhe o
//! formato na camada dele; este crate define apenas o **contrato** entre
//! client e servidor. Versionamento no handshake conforme ADR-0011.

use mareforge_domain_combat::weapon::BroadsideSide;
use serde::{Deserialize, Serialize};

/// Versão atual do protocolo. Qualquer mudança incompatível deve incrementar
/// este número (semântica: quebra de contrato = +1).
///
/// v2: combate — FireBroadside, ShipDestroyed e projéteis no snapshot.
pub const PROTOCOL_VERSION: u16 = 2;

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

/// Navio afundou (PRD §21). O servidor retransmite a todos; loot vem na
/// MF-013.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipDestroyed {
    pub ship_id: u32,
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
    fn current_protocol_version_is_two() {
        assert_eq!(PROTOCOL_VERSION, 2);
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
                },
                ShipState {
                    ship_id: 2,
                    x: 0.0,
                    y: 0.0,
                    heading: 3.0,
                    speed: 0.0,
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
}
