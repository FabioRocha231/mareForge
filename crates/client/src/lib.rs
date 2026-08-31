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
    let starts = [
        std::env::current_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf)),
    ];

    starts
        .into_iter()
        .flatten()
        .find_map(|start| asset_root_from(&start))
        .unwrap_or_else(|| PathBuf::from("assets"))
        .to_string_lossy()
        .into_owned()
}

fn asset_root_from(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|directory| directory.join("assets"))
        .find(|assets| assets.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_workspace_assets_from_the_playtest_package() {
        let client = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = client
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let playtest = workspace.join("crates/tools/mareforge-playtest");

        assert_eq!(asset_root_from(&playtest), Some(workspace.join("assets")));
    }
}
