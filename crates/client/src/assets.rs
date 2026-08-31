//! Central visual asset manifest (MF-056A). Systems consume `GameAssets`;
//! asset loading stays in this module instead of spreading `asset_server.load`
//! across visual systems.

use bevy::asset::{AssetServer, Handle, LoadState};
use bevy::prelude::*;
use bevy::sprite::TextureAtlasLayout;

const SHIP_SHEET: &str = "external/scallywag/ships/ships-tiles.png";
const WATER_AND_ISLANDS_SHEET: &str = "external/scallywag/water-islands/water-island-tiles.png";
const FORT_SHEET: &str = "external/scallywag/fort/fort-tiles.png";

#[derive(Resource)]
pub struct GameAssets {
    pub ships: Handle<Image>,
    pub ships_layout: Handle<TextureAtlasLayout>,
    pub water_and_islands: Handle<Image>,
    pub water_and_islands_layout: Handle<TextureAtlasLayout>,
    pub fort: Handle<Image>,
    pub fort_layout: Handle<TextureAtlasLayout>,
    pub small_merchant: Handle<Image>,
    pub patrol: Handle<Image>,
    pub corsair: Handle<Image>,
    pub wreck: Handle<Image>,
    pub wood_node: Handle<Image>,
    pub ore_node: Handle<Image>,
    pub coral_node: Handle<Image>,
    pub projectile: Handle<Image>,
    pub port: Handle<Image>,
}

pub struct AssetManifestPlugin;

impl Plugin for AssetManifestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_game_assets)
            .add_systems(Update, report_asset_load_result);
    }
}

fn load_game_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let ships = asset_server.load(SHIP_SHEET);
    let water_and_islands = asset_server.load(WATER_AND_ISLANDS_SHEET);
    let fort = asset_server.load(FORT_SHEET);
    let ships_layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(48, 48),
        15,
        14,
        None,
        None,
    ));
    let water_and_islands_layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(48, 48),
        8,
        3,
        None,
        None,
    ));
    let fort_layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(16, 16),
        27,
        16,
        None,
        None,
    ));
    commands.insert_resource(GameAssets {
        ships: ships.clone(),
        ships_layout,
        water_and_islands: water_and_islands.clone(),
        water_and_islands_layout,
        fort: fort.clone(),
        fort_layout,
        small_merchant: ships.clone(),
        patrol: ships.clone(),
        corsair: ships.clone(),
        wreck: water_and_islands.clone(),
        wood_node: water_and_islands.clone(),
        ore_node: water_and_islands.clone(),
        coral_node: water_and_islands,
        projectile: ships,
        port: fort,
    });
}

fn report_asset_load_result(
    asset_server: Res<AssetServer>,
    assets: Res<GameAssets>,
    mut reported: Local<bool>,
) {
    if *reported {
        return;
    }

    let sheets = [
        ("ships", &assets.ships),
        ("water-and-islands", &assets.water_and_islands),
        ("fort", &assets.fort),
    ];
    for (pack, sheet) in sheets {
        if let Some(LoadState::Failed(error)) = asset_server.get_load_state(sheet.id()) {
            warn!(pack, error = %error, "visual asset sheet failed to load; placeholder renderer remains active");
            *reported = true;
            return;
        }
    }

    if sheets.iter().all(|(_, sheet)| {
        matches!(
            asset_server.get_load_state(sheet.id()),
            Some(LoadState::Loaded)
        )
    }) {
        info!(
            "CC0 Scallywag visual assets loaded; placeholder renderer remains active until MF-056B"
        );
        *reported = true;
    }
}
