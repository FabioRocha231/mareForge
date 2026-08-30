pub mod aoi;
pub mod crafting;
pub mod loadout;
pub mod market;
pub mod net;
pub mod nodes;
pub mod npc;
pub mod persist;
mod playtest;
pub mod plugin;
pub mod sets;

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
    app.add_plugins(MinimalPlugins)
        .add_plugins(TerminalCtrlCHandlerPlugin)
        .add_plugins(ServerPlugin)
        .add_plugins(net::ServerNetPlugin);

    if playtest {
        playtest::install(&mut app);
        tracing::info!("playtest session recorder enabled");
    }

    app.run();
}
