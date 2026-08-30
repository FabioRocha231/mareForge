use bevy::app::TerminalCtrlCHandlerPlugin;
use bevy::prelude::*;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,mareforge_server=debug")),
        )
        .init();

    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(TerminalCtrlCHandlerPlugin)
        .add_plugins(mareforge_server::ServerPlugin)
        .add_plugins(mareforge_server::net::ServerNetPlugin)
        .run();
}
