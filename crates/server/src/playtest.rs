//! Playtest session recorder (MF-055).
//!
//! Observabilidade de desenvolvimento, não economia autoritativa: em uma
//! execução `--playtest`, identifica a sessão por um UUID e grava um resumo
//! JSON em `playtest-results/` quando o processo encerra de forma ordenada
//! (SIGTERM/SIGINT). Nada aqui toca no estado persistido do jogo.

use std::time::Instant;

use bevy::app::{App, AppExit, TerminalCtrlCHandlerPlugin};
use bevy::ecs::event::EventReader;
use bevy::prelude::{IntoSystemConfigs, Res, Resource, Update};
use mareforge_domain_economy::Ledger;
use serde::Serialize;
use uuid::Uuid;

use crate::net::Metrics;

/// Identidade de sessão gerada no boot quando `--playtest` está ativo.
#[derive(Resource, Debug, Clone, Copy)]
struct PlaytestSessionId(Uuid);

/// Instante de boot (relógio de parede) usado para `session_duration`.
#[derive(Resource)]
struct PlaytestBoot(Instant);

/// Resumo gravado em disco no encerramento. Os campos casam com o spec
/// MF-055; `ship_losses_by_kind` segue a ordem de `ShipKind as usize`
/// (SmallMerchant=0, Patrol=1, Corsair=2).
#[derive(Serialize)]
struct PlaytestReport {
    session_duration: f64,
    players_seen: usize,
    trips: u64,
    completed_routes: u64,
    average_trip_duration: f64,
    cargo_value_at_risk: u64,
    cargo_value_coverage: f32,
    pvp_engagements: u64,
    ship_losses_by_kind: [u64; 3],
    wrecks_looted: u64,
    items_gathered: u64,
    items_crafted: u64,
    items_destroyed: u64,
    gold_minted: u64,
    gold_burned: u64,
    market_volume: u64,
    npc_bounty_gold_minted: u64,
}

/// Monta o resumo a partir do `Metrics` e do `Ledger` já existentes. Não
/// adiciona nenhuma medição nova: valores que não existem em nenhuma fonte
/// autoritativa ficariam em zero.
fn build_report(session_duration: f64, metrics: &Metrics, ledger: &Ledger) -> PlaytestReport {
    let completed_routes = metrics.completed_routes.values().sum::<u64>();
    let average_trip_duration = if metrics.trip_count == 0 {
        0.0
    } else {
        metrics.trip_total_secs / metrics.trip_count as f64
    };
    PlaytestReport {
        session_duration,
        // TODO(MF-055): a contagem de jogadores distintos não existe na
        // telemetria atual; manter zero sem instrumentar gameplay novo.
        players_seen: 0,
        trips: metrics.trip_count,
        completed_routes,
        average_trip_duration,
        cargo_value_at_risk: metrics.cargo_value_at_risk_total,
        cargo_value_coverage: metrics.cargo_value_coverage_pct,
        pvp_engagements: metrics.pvp_engagements,
        ship_losses_by_kind: metrics.ship_losses_by_kind,
        wrecks_looted: metrics.wrecks_looted,
        items_gathered: metrics.items_gathered,
        items_crafted: metrics.items_crafted,
        items_destroyed: metrics.items_destroyed,
        gold_minted: ledger.minted().0,
        gold_burned: ledger.burned().0,
        market_volume: ledger.market_volume().0,
        npc_bounty_gold_minted: metrics.npc_bounty_gold_minted,
    }
}

/// Grava o resumo em `playtest-results/session-{id}.json`.
fn write_report(report: &PlaytestReport, session_id: Uuid) -> std::io::Result<()> {
    std::fs::create_dir_all("playtest-results")?;
    let path = format!("playtest-results/session-{session_id}.json");
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    std::fs::write(path, json)
}

/// Instala os recursos e o dump no encerramento. Chamado apenas no modo
/// `--playtest`, então o caminho normal nunca cria pasta nem arquivo.
pub(crate) fn install(app: &mut App) {
    install_sigterm_handler();
    app.insert_resource(PlaytestSessionId(Uuid::new_v4()));
    app.insert_resource(PlaytestBoot(Instant::now()));
    app.add_systems(
        Update,
        dump_on_exit.after(TerminalCtrlCHandlerPlugin::exit_on_flag),
    );
}

/// Escreve o resumo quando o app recebe `AppExit` (SIGINT/Ctrl+C via
/// `TerminalCtrlCHandlerPlugin`, SIGTERM via handler próprio, ou `TickLimit`).
fn dump_on_exit(
    mut exits: EventReader<AppExit>,
    session: Res<PlaytestSessionId>,
    boot: Res<PlaytestBoot>,
    metrics: Res<Metrics>,
    market: Res<crate::market::ServerMarket>,
) {
    if exits.read().next().is_none() {
        return;
    }
    let report = build_report(boot.0.elapsed().as_secs_f64(), &metrics, &market.ledger);
    match write_report(&report, session.0) {
        Ok(()) => tracing::info!(session = %session.0, "playtest report written"),
        Err(error) => tracing::error!(error = %error, "failed to write playtest report"),
    }
}

/// SIGTERM roteado para o mesmo flag do Ctrl+C, para que o encerramento
/// ordenado dispare o dump antes do processo sair.
fn install_sigterm_handler() {
    let handler: extern "C" fn(libc::c_int) = handle_sigterm;
    // SAFETY: assinatura e semântica de `libc::signal` para um handler global
    // que apenas faz um store atômico (o mesmo padrão usado pelo
    // TerminalCtrlCHandlerPlugin no SIGINT).
    unsafe {
        libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
    }
}

extern "C" fn handle_sigterm(_sig: libc::c_int) {
    TerminalCtrlCHandlerPlugin::gracefully_exit();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::TradeRouteKey;
    use mareforge_shared::ids::RegionId;

    #[test]
    fn report_builder_emits_zero_fields_for_empty_metrics() {
        let report = build_report(0.0, &Metrics::default(), &Ledger::default());

        assert_eq!(report.session_duration, 0.0);
        assert_eq!(report.players_seen, 0);
        assert_eq!(report.trips, 0);
        assert_eq!(report.completed_routes, 0);
        assert_eq!(report.average_trip_duration, 0.0);
        assert_eq!(report.cargo_value_at_risk, 0);
        assert_eq!(report.pvp_engagements, 0);
        assert_eq!(report.ship_losses_by_kind, [0, 0, 0]);
        assert_eq!(report.wrecks_looted, 0);
        assert_eq!(report.items_gathered, 0);
        assert_eq!(report.items_crafted, 0);
        assert_eq!(report.items_destroyed, 0);
        assert_eq!(report.gold_minted, 0);
        assert_eq!(report.gold_burned, 0);
        assert_eq!(report.market_volume, 0);
        assert_eq!(report.npc_bounty_gold_minted, 0);
    }

    #[test]
    fn report_builder_guards_div_by_zero() {
        let metrics = Metrics {
            trip_total_secs: 123.0,
            cargo_value_at_risk_total: 500,
            ..Metrics::default()
        };

        let report = build_report(10.0, &metrics, &Ledger::default());

        assert_eq!(report.average_trip_duration, 0.0);
        assert_eq!(report.cargo_value_at_risk, 500);
    }

    #[test]
    fn report_builder_sums_completed_routes() {
        let mut metrics = Metrics {
            trip_total_secs: 100.0,
            trip_count: 10,
            ..Metrics::default()
        };
        metrics.completed_routes.insert(
            TradeRouteKey {
                origin: RegionId::new(),
                destination: RegionId::new(),
            },
            3,
        );
        metrics.completed_routes.insert(
            TradeRouteKey {
                origin: RegionId::new(),
                destination: RegionId::new(),
            },
            5,
        );

        let report = build_report(0.0, &metrics, &Ledger::default());

        assert_eq!(report.completed_routes, 8);
        assert_eq!(report.average_trip_duration, 10.0);
    }
}
