//! Cenario visual estatico do vertical slice (MF-057D, MF-057E).
//! A posicao vem do `WorldMap`; esta camada apenas escolhe arte e nunca
//! altera geometria ou risco. Composicao e identidade dos portos moram
//! aqui (Serra = madeira/verde, Mina = pedra/industrial).

use bevy::asset::LoadState;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use mareforge_domain_world::{WorldMap, ZoneShape};

use crate::assets::{frames, image_failed, layers, GameAssets};

/// Escala base de cada tile de agua. O atlas eh 48 px; 16x = 768 px por
/// tile, suficiente para o mundo vertical caber em ~3 tiles visiveis.
const OCEAN_TILE_SCALE: f32 = 16.0;
const OCEAN_TILE_STEP: f32 = 48.0 * OCEAN_TILE_SCALE;

/// Escala das props de porto: casas, docas, vegetacao, muros. O atlas
/// fort-tiles.png tem tiles de 16 px; 3x = 48 px.
/* REMOVED: PROP_SCALE was only used by spawn_shore_band, which has been
   removed (it created the dotted stair-step artifact around ports). */

pub struct WorldVisualPlugin;

impl Plugin for WorldVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_vertical_slice_world);
    }
}

fn atlas_sprite(image: Handle<Image>, layout: Handle<TextureAtlasLayout>, index: usize) -> Sprite {
    Sprite::from_atlas_image(image, TextureAtlas { layout, index })
}

/// MF-057D: agua com dois andares visuais. Tile base = agua do atlas
/// (frames::OCEAN, azul claro). Onda de fundo = ocean_deep procedural
/// (azul mais escuro, mais contraste). A combinacao da um piso de agua
/// com profundidade visivel sem pipeline de shader.
fn spawn_water_tile(
    commands: &mut Commands,
    assets: &GameAssets,
    asset_server: &AssetServer,
    position: Vec2,
) {
    let deep_failed = image_failed(asset_server, &assets.ocean_deep);
    let water_failed = image_failed(asset_server, &assets.water_and_islands);

    // Fundo: ocean_deep procedural. Sprite + leve alpha para nao cobrir
    // totalmente o tile base.
    if !deep_failed {
        commands.spawn((
            Sprite {
                image: assets.ocean_deep.clone(),
                color: Color::srgba(1.0, 1.0, 1.0, 0.55),
                ..default()
            },
            Transform {
                translation: position.extend(layers::OCEAN - 0.5),
                scale: Vec3::splat(OCEAN_TILE_SCALE),
                ..default()
            },
        ));
    }

    // Tile base do atlas por cima, com leve variacao de matiz.
    let sprite = if water_failed {
        Sprite::from_color(Color::srgb(0.04, 0.18, 0.30), Vec2::splat(48.0))
    } else {
        atlas_sprite(
            assets.water_and_islands.clone(),
            assets.water_and_islands_layout.clone(),
            frames::OCEAN,
        )
    };
    commands.spawn((
        sprite,
        Transform {
            translation: position.extend(layers::OCEAN),
            scale: Vec3::splat(OCEAN_TILE_SCALE),
            ..default()
        },
    ));
}

fn spawn_visual(commands: &mut Commands, sprite: Sprite, position: Vec2, scale: Vec3, z: f32) {
    commands.spawn((
        sprite,
        Transform {
            translation: position.extend(z),
            scale,
            ..default()
        },
    ));
}

/// Spawna o cenario completo uma unica vez apos os sheets carregarem.
fn spawn_vertical_slice_world(
    mut commands: Commands,
    assets: Res<GameAssets>,
    asset_server: Res<AssetServer>,
    mut spawned: Local<bool>,
) {
    if *spawned || !world_sheets_resolved(&asset_server, &assets) {
        return;
    }
    *spawned = true;

    let water_failed = image_failed(&asset_server, &assets.water_and_islands);
    let fort_failed = image_failed(&asset_server, &assets.fort);

    // Agua: 5x5 tiles com variacao de matiz por posicao (alternando base
    // e deep) para a cena nao parecer um wallpaper.
    for x in -2..=2 {
        for y in -2..=2 {
            spawn_water_tile(
                &mut commands,
                &assets,
                &asset_server,
                Vec2::new(x as f32 * OCEAN_TILE_STEP, y as f32 * OCEAN_TILE_STEP),
            );
        }
    }

    let map = WorldMap::vertical_slice();
    for region in map.regions() {
        let Some(port) = &region.port else { continue };
        let position = Vec2::new(port.x, port.y);
        let theme = if port.name.contains("Mina") {
            PortTheme::Mina
        } else {
            PortTheme::Serra
        };
        spawn_port_composition(
            &mut commands,
            &assets,
            water_failed,
            fort_failed,
            theme,
            position,
        );
    }

    let island = map.zones().iter().find_map(|zone| {
        (zone.name == "Águas da Ilha do Coral Negro").then(|| match zone.shape {
            ZoneShape::Circle { x, y, .. } => Vec2::new(x, y),
        })
    });
    if let Some(position) = island {
        let sprite = if water_failed {
            Sprite::from_color(Color::srgb(0.26, 0.05, 0.08), Vec2::splat(48.0))
        } else {
            atlas_sprite(
                assets.water_and_islands.clone(),
                assets.water_and_islands_layout.clone(),
                frames::ISLAND,
            )
        };
        spawn_landmark(
            &mut commands,
            sprite,
            "Ilha do Coral Negro",
            position,
            Vec3::splat(5.0),
        );
        if !fort_failed {
            spawn_visual(
                &mut commands,
                atlas_sprite(
                    assets.fort.clone(),
                    assets.fort_layout.clone(),
                    frames::DANGER_MARKER,
                ),
                position + Vec2::new(34.0, 20.0),
                Vec3::splat(2.0),
                layers::PROPS,
            );
        }
    } else {
        warn!("WorldMap sem zona da Ilha do Coral Negro; marcador visual omitido");
    }
}

#[derive(Clone, Copy)]
enum PortTheme {
    /// Madeira + vegetacao + comercio leve. Atmosfera organica.
    Serra,
    /// Pedra + peso visual + clima industrial. Mais rigido.
    Mina,
}

/// Composicao de porto (MF-057E): silhueta reconhecivel, doca visivel,
/// relacao clara agua/terra, props organizados com intencao.
fn spawn_port_composition(
    commands: &mut Commands,
    assets: &GameAssets,
    water_failed: bool,
    fort_failed: bool,
    theme: PortTheme,
    position: Vec2,
) {
    let name = match theme {
        PortTheme::Serra => "Porto da Serra",
        PortTheme::Mina => "Porto da Mina",
    };

    // Silhueta principal (landmark). Escala maior para os portos terem
    // massa visual (MF-057E).
    let primary = if fort_failed {
        let color = match theme {
            PortTheme::Serra => Color::srgb(0.42, 0.28, 0.14),
            PortTheme::Mina => Color::srgb(0.32, 0.34, 0.38),
        };
        Sprite::from_color(color, Vec2::splat(48.0))
    } else {
        atlas_sprite(
            assets.fort.clone(),
            assets.fort_layout.clone(),
            frames::PORT_CRATE,
        )
    };
    let primary_scale = match theme {
        PortTheme::Serra => Vec3::splat(4.5),
        PortTheme::Mina => Vec3::splat(5.5),
    };
    spawn_landmark(commands, primary, name, position, primary_scale);

    if fort_failed {
        return;
    }

    // Props em volta do landmark. Quantidade e tipo variam por tema.
    let (barrels, crates, side_gap, prop_color) = match theme {
        PortTheme::Serra => (2, 2, 28.0, Color::srgb(1.0, 0.95, 0.8)),
        PortTheme::Mina => (3, 1, 22.0, Color::srgb(0.85, 0.85, 0.95)),
    };

    for i in 0..barrels {
        let angle = (i as f32 + 1.0) * std::f32::consts::TAU / (barrels as f32 + 1.0);
        let offset = Vec2::new(angle.cos(), angle.sin()) * side_gap;
        spawn_visual(
            commands,
            Sprite {
                color: prop_color,
                ..atlas_sprite(
                    assets.fort.clone(),
                    assets.fort_layout.clone(),
                    frames::PORT_BARREL,
                )
            },
            position + offset,
            Vec3::splat(2.2),
            layers::PROPS,
        );
    }
    for i in 0..crates {
        let angle =
            std::f32::consts::PI + (i as f32 + 1.0) * std::f32::consts::PI / (crates as f32 + 1.0);
        let offset = Vec2::new(angle.cos(), angle.sin()) * (side_gap + 4.0);
        spawn_visual(
            commands,
            atlas_sprite(
                assets.fort.clone(),
                assets.fort_layout.clone(),
                frames::PORT_CRATE,
            ),
            position + offset,
            Vec3::splat(1.8),
            layers::PROPS,
        );
    }

    // Toque de vida: corais perto da Serra, pedras/voxels perto da Mina.
    if !water_failed {
        let accent_frame = match theme {
            PortTheme::Serra => frames::CORAL_NODE,
            PortTheme::Mina => frames::ORE_NODE,
        };
        for i in 0..3 {
            let angle = i as f32 * std::f32::consts::TAU / 3.0;
            let offset = Vec2::new(angle.cos(), angle.sin()) * (side_gap + 18.0);
            spawn_visual(
                commands,
                atlas_sprite(
                    assets.water_and_islands.clone(),
                    assets.water_and_islands_layout.clone(),
                    accent_frame,
                ),
                position + offset,
                Vec3::splat(1.2),
                layers::PROPS,
            );
        }
    }
}

fn spawn_landmark(
    commands: &mut Commands,
    sprite: Sprite,
    name: &str,
    position: Vec2,
    scale: Vec3,
) {
    commands.spawn((sprite, landmark_transform(position, scale)));

    commands.spawn((
        Text2d::new(name),
        // MF-057: wrap default do Text2d parte a string em varias linhas
        // em world units e quebrava "Porto da Serra" em "Porto / da /
        // Serra" estilizado. Forcamos uma linha so para o nome caber
        // sobre o landmark sem fragmentar.
        TextLayout::new_with_no_wrap(),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.92, 0.88, 0.72)),
        Anchor::Center,
        Transform::from_translation(Vec3::new(
            position.x,
            position.y + 64.0,
            layers::LABELS,
        )),
    ));
}

fn landmark_transform(position: Vec2, scale: Vec3) -> Transform {
    Transform {
        translation: position.extend(layers::LAND),
        scale,
        ..default()
    }
}

fn world_sheets_resolved(asset_server: &AssetServer, assets: &GameAssets) -> bool {
    [assets.water_and_islands.id(), assets.fort.id()]
        .into_iter()
        .all(|id| {
            matches!(
                asset_server.get_load_state(id),
                Some(LoadState::Loaded | LoadState::Failed(_))
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_slice_exposes_the_three_visual_landmarks() {
        let map = WorldMap::vertical_slice();
        assert_eq!(
            map.regions()
                .iter()
                .filter(|region| region.port.is_some())
                .count(),
            2
        );
        assert!(map
            .zones()
            .iter()
            .any(|zone| zone.name == "Águas da Ilha do Coral Negro"));
    }
}
