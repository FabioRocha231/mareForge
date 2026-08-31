//! Central visual asset manifest (MF-056A). Systems consume `GameAssets`;
//! asset loading stays in this module instead of spreading `asset_server.load`
//! across visual systems.

use bevy::asset::{AssetServer, Handle, LoadState};
use bevy::prelude::*;
use bevy::sprite::TextureAtlasLayout;

const SHIP_SHEET: &str = "external/scallywag/ships/ships-tiles.png";
const WATER_AND_ISLANDS_SHEET: &str = "external/scallywag/water-islands/water-island-tiles.png";
const FORT_SHEET: &str = "external/scallywag/fort/fort-tiles.png";

/// Ordem de desenho do mundo 2D. Sistemas visuais usam estes valores em vez
/// de espalhar profundidades numéricas que podem inverter a cena por acaso.
pub mod layers {
    pub const OCEAN: f32 = -10.0;
    pub const LAND: f32 = -9.0;
    pub const PROPS: f32 = -8.0;
    pub const RESOURCES: f32 = -7.0;
    pub const WRECKS: f32 = -6.0;
    pub const SHIPS: f32 = -5.0;
    pub const PROJECTILES: f32 = -4.0;
    pub const LABELS: f32 = 5.0;
    pub const HUD: f32 = 10.0;
    pub const OVERLAY: f32 = 20.0;
}

/// Frames dos sheets CC0. Os três navios usam recortes completos do atlas.
pub mod frames {
    pub const SMALL_MERCHANT: usize = 0;
    pub const PATROL: usize = 1;
    pub const CORSAIR: usize = 2;
    pub const OCEAN: usize = 3;
    pub const ISLAND: usize = 0;
    // `water-island-tiles.png`: 16..18 são somente rochas.
    pub const ORE_NODE: usize = 16;
    // `fort-tiles.png`: carga terrestre para os marcos dos portos.
    pub const WOOD_NODE: usize = 18;
    // `water-island-tiles.png`: casco/destroço marrom verificado visualmente.
    pub const WRECK: usize = 17;
    // `water-island-tiles.png`: vegetação e madeira; recebe tinta coral.
    pub const CORAL_NODE: usize = 19;
    pub const PROJECTILE: usize = 157;
    pub const PORT_CRATE: usize = 31;
    pub const PORT_BARREL: usize = 153;
    pub const DANGER_MARKER: usize = 101;
}

#[derive(Resource)]
pub struct GameAssets {
    pub ships: Handle<Image>,
    pub ships_layout: Handle<TextureAtlasLayout>,
    pub ships_detail_layout: Handle<TextureAtlasLayout>,
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

pub fn image_failed(asset_server: &AssetServer, image: &Handle<Image>) -> bool {
    matches!(
        asset_server.get_load_state(image.id()),
        Some(LoadState::Failed(_))
    )
}

impl Plugin for AssetManifestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_game_assets)
            .add_systems(Update, report_asset_load_result);
    }
}

pub(crate) fn load_game_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let ships = asset_server.load(SHIP_SHEET);
    let water_and_islands = asset_server.load(WATER_AND_ISLANDS_SHEET);
    let fort = asset_server.load(FORT_SHEET);
    let mut ships_layout = TextureAtlasLayout::new_empty(UVec2::new(720, 672));
    // Os cascos não ocupam uma grade regular. Estes retângulos seguem os
    // limites reais de três embarcações completas no sheet.
    ships_layout.add_texture(URect::new(33, 0, 63, 159));
    ships_layout.add_texture(URect::new(306, 0, 350, 178));
    ships_layout.add_texture(URect::new(497, 0, 543, 256));
    let ships_layout = layouts.add(ships_layout);
    let ships_detail_layout = layouts.add(TextureAtlasLayout::from_grid(
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
        ships_detail_layout,
        water_and_islands: water_and_islands.clone(),
        water_and_islands_layout,
        fort: fort.clone(),
        fort_layout,
        small_merchant: ships.clone(),
        patrol: ships.clone(),
        corsair: ships.clone(),
        wreck: water_and_islands.clone(),
        wood_node: fort.clone(),
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
            warn!(pack, error = %error, "visual asset sheet failed to load; affected entities use geometric fallback");
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
        info!("CC0 Scallywag visual assets loaded");
        *reported = true;
    }
}
