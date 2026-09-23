pub mod aoi;
pub mod crafting;
pub mod guild;
pub mod loadout;
pub mod market;
pub mod net;
pub mod nodes;
pub mod npc;
pub mod persist;
mod playtest;
pub mod plugin;
pub mod portals;
pub mod reputation;
pub mod sets;
pub mod weather;

pub use plugin::ServerPlugin;

use bevy::app::TerminalCtrlCHandlerPlugin;
use bevy::prelude::*;
use tracing_subscriber::EnvFilter;

/// Headless server app shared by `mareforge-server` and the playtest child.
pub fn run_headless() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,mareforge_server=debug")),
        )
        .init();

    let playtest = std::env::args().any(|arg| arg == "--playtest");

    let mut app = App::new();
    // Sem `run_loop` o MinimalPlugins gira o loop sem pausa (130%+ de CPU
    // ocioso). 60 Hz de frame folga o FixedUpdate de 30 Hz e a rede.
    app.add_plugins(
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(
            std::time::Duration::from_secs_f64(1.0 / 60.0),
        )),
    )
    .add_plugins(TerminalCtrlCHandlerPlugin)
    .add_plugins(ServerPlugin)
    .add_plugins(net::ServerNetPlugin);

    if playtest {
        playtest::install(&mut app);
        tracing::info!("playtest session recorder enabled");
    }

    app.run();
}
