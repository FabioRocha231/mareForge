pub mod crafting;
pub mod market;
pub mod net;
pub mod nodes;
pub mod playtest;
pub mod plugin;
pub mod port_screen;
pub mod ship;
pub mod zone;

pub use plugin::ClientPlugin;

use bevy::prelude::*;

/// Window app shared by `mareforge-client` and the playtest binary.
pub fn windowed_app() -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Mareforge".into(),
            resolution: (1280.0, 720.0).into(),
            ..default()
        }),
        ..default()
    }));
    app.add_plugins(ClientPlugin);
    app
}
