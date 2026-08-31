//! Cenário visual estático do vertical slice. A posição vem do `WorldMap`;
//! esta camada apenas escolhe arte e nunca altera geometria ou risco.

use bevy::prelude::*;
use mareforge_domain_world::{WorldMap, ZoneShape};

use crate::assets::{frames, layers, GameAssets};

const OCEAN_TILE_SCALE: f32 = 16.0;
const OCEAN_TILE_STEP: f32 = 48.0 * OCEAN_TILE_SCALE;

pub struct WorldVisualPlugin;

impl Plugin for WorldVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            spawn_vertical_slice_world.after(crate::assets::load_game_assets),
        );
    }
}

fn atlas_sprite(image: Handle<Image>, layout: Handle<TextureAtlasLayout>, index: usize) -> Sprite {
    Sprite::from_atlas_image(image, TextureAtlas { layout, index })
}

fn spawn_vertical_slice_world(mut commands: Commands, assets: Res<GameAssets>) {
    // Cinco tiles ampliados preservam a leitura de água pixelada sem criar uma
    // grade proporcional ao mapa inteiro. O cenário é somente decorativo.
    for x in -2..=2 {
        for y in -2..=2 {
            commands.spawn((
                atlas_sprite(
                    assets.water_and_islands.clone(),
                    assets.water_and_islands_layout.clone(),
                    frames::OCEAN,
                ),
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
        let frame = if port.name == "Porto da Serra" {
            frames::SERRA_PORT
        } else {
            frames::MINA_PORT
        };
        spawn_landmark(
            &mut commands,
            atlas_sprite(assets.fort.clone(), assets.fort_layout.clone(), frame),
            port.name,
            Vec2::new(port.x, port.y),
            Vec3::splat(3.0),
        );
    }

    let island = map.zones().iter().find_map(|zone| {
        (zone.name == "Águas da Ilha do Coral Negro").then(|| match zone.shape {
            ZoneShape::Circle { x, y, .. } => Vec2::new(x, y),
        })
    });
    if let Some(position) = island {
        spawn_landmark(
            &mut commands,
            atlas_sprite(
                assets.water_and_islands.clone(),
                assets.water_and_islands_layout.clone(),
                frames::ISLAND,
            ),
            "Ilha do Coral Negro",
            position,
            Vec3::splat(5.0),
        );
    } else {
        warn!("WorldMap sem zona da Ilha do Coral Negro; cenário da ilha omitido");
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
        .spawn((
            sprite,
            Transform {
                translation: position.extend(layers::LAND),
                scale,
                ..default()
            },
        ))
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
