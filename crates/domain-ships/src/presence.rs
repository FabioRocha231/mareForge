//! Presença da embarcação (MF-036): o navio está no mar ou atracado — e
//! `Docked` é um ESTADO explícito, não a impressão de estar dentro da baía.
//! Águas protegidas continuam sendo água segura; só a doca dá acesso a
//! serviços (storage, craft, mercado, construção).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use mareforge_shared::ids::RegionId;

/// Estado de presença da embarcação. No slice, PortId == RegionId (PRD §6:
/// um porto por região) — a régua muda quando o mundo pedir portos múltiplos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VesselPresence {
    AtSea,
    /// Atracado no porto da região: movimento/tiro/coleta/saque desligados;
    /// serviços de porto ligados.
    Docked(RegionId),
}

/// Política de atracação (tuning é conteúdo, §32/§39).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockPolicy {
    /// Velocidade máxima (m/s) para lançar as amarras.
    pub max_dock_speed: f32,
}

impl Default for DockPolicy {
    fn default() -> Self {
        Self {
            max_dock_speed: 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum DockError {
    #[error("fora da área de qualquer porto")]
    NotInPort,
    #[error("veloz demais para atracar: {speed:.1} m/s (limite {limit:.1})")]
    TooFast { speed: f32, limit: f32 },
    #[error("navio já está atracado")]
    AlreadyDocked,
    #[error("navio não está atracado")]
    NotDocked,
}

/// Tenta atracar. Puro e fail-closed: já atracado, fora do porto ou veloz
/// demais são erros — nada de re-atracar silenciosamente.
pub fn dock(
    presence: &VesselPresence,
    speed: f32,
    port_region: Option<RegionId>,
    policy: &DockPolicy,
) -> Result<VesselPresence, DockError> {
    match presence {
        VesselPresence::Docked(_) => Err(DockError::AlreadyDocked),
        VesselPresence::AtSea => {
            let region = port_region.ok_or(DockError::NotInPort)?;
            if speed > policy.max_dock_speed {
                return Err(DockError::TooFast {
                    speed,
                    limit: policy.max_dock_speed,
                });
            }
            Ok(VesselPresence::Docked(region))
        }
    }
}

/// Desatraca. O navio permanece no ponto onde atracou (que É o ponto de
/// saída da doca), com o HP e a carga que já tinha — nada de cura grátis.
pub fn undock(presence: &VesselPresence) -> Result<VesselPresence, DockError> {
    match presence {
        VesselPresence::Docked(_) => Ok(VesselPresence::AtSea),
        VesselPresence::AtSea => Err(DockError::NotDocked),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region() -> RegionId {
        RegionId::new()
    }

    #[test]
    fn dock_stops_the_ship_at_the_port() {
        let policy = DockPolicy::default();
        let region = region();
        let presence = dock(&VesselPresence::AtSea, 3.0, Some(region), &policy).expect("atracou");
        assert_eq!(presence, VesselPresence::Docked(region));
    }

    #[test]
    fn dock_refuses_a_fast_hull() {
        let policy = DockPolicy::default();
        let error = dock(&VesselPresence::AtSea, 12.0, Some(region()), &policy).unwrap_err();
        assert_eq!(
            error,
            DockError::TooFast {
                speed: 12.0,
                limit: 5.0
            }
        );
    }

    #[test]
    fn dock_outside_any_port_fails_closed() {
        let policy = DockPolicy::default();
        let error = dock(&VesselPresence::AtSea, 0.0, None, &policy).unwrap_err();
        assert_eq!(error, DockError::NotInPort);
    }

    #[test]
    fn docking_twice_is_an_error_not_a_noop() {
        let policy = DockPolicy::default();
        let region = region();
        let presence = VesselPresence::Docked(region);
        assert_eq!(
            dock(&presence, 0.0, Some(region), &policy),
            Err(DockError::AlreadyDocked)
        );
    }

    #[test]
    fn undock_returns_to_sea_and_keeps_everything() {
        let region = region();
        let presence = undock(&VesselPresence::Docked(region)).expect("desatracou");
        assert_eq!(presence, VesselPresence::AtSea);
    }

    #[test]
    fn undock_at_sea_is_an_error() {
        assert_eq!(undock(&VesselPresence::AtSea), Err(DockError::NotDocked));
    }
}
