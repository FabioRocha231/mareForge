//! Cenário visual estático do vertical slice. A posição vem do `WorldMap`;
//! esta camada apenas escolhe arte e nunca altera geometria ou risco.

use bevy::asset::LoadState;
use bevy::prelude::*;
use mareforge_domain_world::{WorldMap, ZoneShape};

use crate::assets::{frames, image_failed, layers, GameAssets};

const OCEAN_TILE_SCALE: f32 = 16.0;
const OCEAN_TILE_STEP: f32 = 48.0 * OCEAN_TILE_SCALE;

pub struct WorldVisualPlugin;

impl Plugin for WorldVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_vertical_slice_world);
    }
}

fn atlas_sprite(image: Handle<Image>, layout: Handle<TextureAtlasLayout>, index: usize) -> Sprite {
    Sprite::from_atlas_image(image, TextureAtlas { layout, index })
}

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

    // Cinco tiles ampliados preservam a leitura de água pixelada sem criar uma
    // grade proporcional ao mapa inteiro. O cenário é somente decorativo.
    for x in -2..=2 {
        for y in -2..=2 {
            let sprite = if water_failed {
                Sprite::from_color(Color::srgb(0.03, 0.2, 0.31), Vec2::splat(OCEAN_TILE_STEP))
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
                    translation: Vec3::new(
                        x as f32 * OCEAN_TILE_STEP,
                        y as f32 * OCEAN_TILE_STEP,
                        layers::OCEAN,
                    ),
                    scale: Vec3::splat(OCEAN_TILE_SCALE),
                    ..default()
                },
            ));
        }
    }

    let map = WorldMap::vertical_slice();
    for region in map.regions() {
        let Some(port) = &region.port else { continue };
        spawn_port_landmark(
            &mut commands,
            &assets,
            water_failed,
            fort_failed,
            port.name,
            Vec2::new(port.x, port.y),
        );
    }

    let island = map.zones().iter().find_map(|zone| {
        (zone.name == "Águas da Ilha do Coral Negro").then(|| match zone.shape {
            ZoneShape::Circle { x, y, .. } => Vec2::new(x, y),
        })
    });
    if let Some(position) = island {
        let sprite = if water_failed {
            Sprite::from_color(Color::srgb(0.26, 0.05, 0.08), Vec2::splat(192.0))
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
            );
        }
    } else {
        warn!("WorldMap sem zona da Ilha do Coral Negro; marcador visual omitido");
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

fn spawn_port_landmark(
    commands: &mut Commands,
    assets: &GameAssets,
    water_failed: bool,
    fort_failed: bool,
    name: &'static str,
    position: Vec2,
) {
    let serra = name == "Porto da Serra";
    let primary = if fort_failed {
        let color = if serra {
            Color::srgb(0.45, 0.28, 0.12)
        } else {
            Color::srgb(0.28, 0.32, 0.4)
        };
        Sprite::from_color(color, Vec2::splat(48.0))
    } else {
        let frame = if serra {
            frames::SERRA_CART
        } else {
            frames::MINA_CART
        };
        atlas_sprite(assets.fort.clone(), assets.fort_layout.clone(), frame)
    };
    spawn_landmark(commands, primary, name, position, Vec3::splat(3.0));

    if fort_failed {
        return;
    }
    if serra {
        spawn_visual(
            commands,
            atlas_sprite(
                assets.fort.clone(),
                assets.fort_layout.clone(),
                frames::SERRA_CRATE,
            ),
            position + Vec2::new(30.0, 0.0),
            Vec3::splat(2.0),
        );
    } else {
        spawn_visual(
            commands,
            atlas_sprite(
                assets.fort.clone(),
                assets.fort_layout.clone(),
                frames::MINA_ORE,
            ),
            position + Vec2::new(28.0, 0.0),
            Vec3::splat(2.0),
        );
        if !water_failed {
            spawn_visual(
                commands,
                atlas_sprite(
                    assets.water_and_islands.clone(),
                    assets.water_and_islands_layout.clone(),
                    frames::ORE_NODE,
                ),
                position + Vec2::new(-30.0, 0.0),
                Vec3::splat(0.8),
            );
        }
    }
}

fn spawn_landmark(
    commands: &mut Commands,
    sprite: Sprite,
    name: &'static str,
    position: Vec2,
    scale: Vec3,
) {
    commands
        .spawn((sprite, landmark_transform(position, scale)))
        .with_children(|parent| {
            parent.spawn((
                Text2d::new(name),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.88, 0.72)),
                Transform::from_xyz(0.0, 40.0, layers::LABELS - layers::LAND),
            ));
        });
}

fn spawn_visual(commands: &mut Commands, sprite: Sprite, position: Vec2, scale: Vec3) {
    commands.spawn((sprite, landmark_transform(position, scale)));
}

fn landmark_transform(position: Vec2, scale: Vec3) -> Transform {
    Transform {
        translation: position.extend(layers::LAND),
        scale,
        ..default()
    }
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
