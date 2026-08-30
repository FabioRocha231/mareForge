//! SystemSet canônico do tick de simulação (MF-054, ADR-0008).
//!
//! `simulate_world` virou só registro: cada bloco virou um sistema dentro
//! de um destes sets, e os sets rodam nesta ordem via `.chain()` (Bevy 0.15).
//! Mantém o limite prático de ~15 SystemParams do Bevy saudável para A2:
//! cada sistema individual cabe em uma tupla de até 8 parâmetros.

use bevy::prelude::*;

/// Ordem canônica dos sub-sistemas do tick (MF-054).
///
/// Os sets rodam em sequência no `FixedUpdate` via `.chain()`:
/// input chega → mundo se move → zonas resolvem → combate → destruição →
/// consequências econômicas → telemetria → snapshot → persistência.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimulationSet {
    /// Mensagens dos clients: input de movimento, dock/undock, tiro, loot,
    /// gather, craft, storage.
    Input,
    /// Avança física dos navios (motion step + bateria). Atracados: speed=0.
    Movement,
    /// Detecta cruzamento de fronteira entre zonas; envia ZoneChanged;
    /// incrementa `metrics.zone_transitions`.
    Zones,
    /// Avança projéteis, detecta colisões, expira os fora do ar. Coleta
    /// impactos em `CombatImpacts` para o próximo set.
    Combat,
    /// Aplica dano; resolve destruição (wreck + loot + dev respawn); conta
    /// métricas de gameplay (pvp_engagements, ship_losses_by_kind, trip
    /// encerrada em sink).
    Destruction,
    /// Efeitos de mercado derivados dos eventos do tick (vendido, escrow
    /// devolvido por expiração).
    EconomyConsequences,
    /// Telemetria periódica de mundo (world_status 5s; init_trip_started_at).
    Telemetry,
    /// Envia `WorldSnapshot` aos clients na cadência do `SnapshotClock`.
    Snapshot,
    /// Persiste estado derivado (wrecks) após o tick, fora do hot loop.
    Persistence,
}
