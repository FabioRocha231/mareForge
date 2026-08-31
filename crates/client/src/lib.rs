pub mod assets;
pub mod crafting;
pub mod market;
pub mod net;
pub mod nodes;
pub mod playtest;
pub mod plugin;
pub mod port_screen;
pub mod ship;
pub mod world;
pub mod zone;

pub use plugin::ClientPlugin;

use std::path::{Path, PathBuf};

use bevy::asset::AssetPlugin;
use bevy::prelude::*;

/// Window app shared by `mareforge-client` and the playtest binary.
pub fn windowed_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: asset_root(),
                ..default()
            })
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Mareforge".into(),
                    resolution: (1280.0, 720.0).into(),
                    ..default()
                }),
                ..default()
            }),
    );
    app.add_plugins(ClientPlugin);
    app
}

fn asset_root() -> String {
    workspace_assets().to_string_lossy().into_owned()
}

fn workspace_assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("mareforge-client lives under the workspace crates directory")
        .join("assets")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_uses_the_compiled_workspace_assets() {
        let client = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = client
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");

        assert_eq!(Path::new(&asset_root()), workspace.join("assets"));
    }
}
