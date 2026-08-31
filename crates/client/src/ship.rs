//! Visual dos navios e projéteis replicados (PRD MF-006/009). Cada estado do
//! snapshot tem uma entidade visual aqui; o client faz lerp suave entre
//! snapshots. Sem predição local — a verdade é o servidor (Pilar 4).
//!
//! AOI (MF-031): entidades que SAEM do snapshot não somem na hora — ficam
//! como last-known-state por [`STALE_VISUAL_TTL`] segundos e só então o
//! visual sai. Wrecks chegam pelo snapshot (protocolo v8), com o mesmo TTL.

use std::collections::HashSet;
use std::time::Instant;

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::ClientReceiveMessage;
use mareforge_domain_ships::ShipKind;
use mareforge_protocol::{ProjectileState, ShipState, WorldSnapshot};

use crate::assets::{frames, image_failed, layers, GameAssets};

/// Quanto tempo um visual sobrevive sem aparecer no snapshot (ADR-0009:
/// last-known-state até expirar). Curto de propósito: é máscara de pop do
/// AOI, não verdade sobre o mundo.
pub const STALE_VISUAL_TTL: f32 = 2.0;

/// Entidade visual de um navio autoritativo. `target` é o último estado
/// autoritativo conhecido (alvo do lerp visual).
#[derive(Component)]
pub struct ShipVisual {
    pub target: ShipState,
    pub last_seen: Instant,
}

const SHIP_HEADING_OFFSET: f32 = -std::f32::consts::FRAC_PI_2;

fn ship_frame_and_scale(kind: ShipKind) -> (usize, Vec3) {
    match kind {
        ShipKind::SmallMerchant => (frames::SMALL_MERCHANT, Vec3::splat(1.15)),
        ShipKind::Patrol => (frames::PATROL, Vec3::splat(1.05)),
        ShipKind::Corsair => (frames::CORSAIR, Vec3::splat(0.92)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_ship_kind_has_a_distinct_visual_frame() {
        let merchant = ship_frame_and_scale(ShipKind::SmallMerchant).0;
        let patrol = ship_frame_and_scale(ShipKind::Patrol).0;
        let corsair = ship_frame_and_scale(ShipKind::Corsair).0;
        assert_ne!(merchant, patrol);
        assert_ne!(merchant, corsair);
        assert_ne!(patrol, corsair);
    }
}

/// Navios que já afundaram: snapshots em voo não podem ressuscitá-los.
#[derive(Resource, Debug, Default)]
pub struct DestroyedShips(pub HashSet<u32>);

#[allow(clippy::too_many_arguments)]
pub fn upsert_ship_visuals(
    mut commands: Commands,
    my_ship: Res<crate::net::MyShip>,
    destroyed: Res<DestroyedShips>,
    mut snapshot_events: EventReader<ClientReceiveMessage<WorldSnapshot>>,
    mut existing: Query<(Entity, &mut ShipVisual)>,
    assets: Res<GameAssets>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Antes do AssignShip não sabemos qual navio é nosso; processar snapshot
    // agora pintaria o próprio navio com a cor errada (cor decide no spawn).
    if my_ship.0.is_none() {
        return;
    }
    // Só o snapshot mais recente importa: eventos antigos nasceram velhos e
    // processar vários no mesmo frame spawnaria duplicatas (spawn é deferido).
    let Some(event) = snapshot_events.read().last() else {
        return;
    };
    for state in &event.message().ships {
        if destroyed.0.contains(&state.ship_id) {
            continue;
        }
        let mut updated = false;
        for (_, mut visual) in existing.iter_mut() {
            if visual.target.ship_id == state.ship_id {
                visual.target = *state;
                visual.last_seen = Instant::now();
                updated = true;
                break;
            }
        }
        if updated {
            continue;
        }

        let (frame, scale) = ship_frame_and_scale(state.kind);
        let mut entity = commands.spawn((
            ShipVisual {
                target: *state,
                last_seen: Instant::now(),
            },
            Transform {
                translation: Vec3::new(state.x, state.y, layers::SHIPS),
                rotation: Quat::from_rotation_z(state.heading + SHIP_HEADING_OFFSET),
                scale,
            },
        ));
        if image_failed(&asset_server, &assets.ships) {
            let color = if state.is_npc {
                Color::srgb(0.85, 0.2, 0.2)
            } else {
                Color::srgb(0.55, 0.62, 0.7)
            };
            entity.insert((
                Mesh2d(meshes.add(Triangle2d::new(
                    Vec2::new(16.0, 0.0),
                    Vec2::new(-10.0, 8.0),
                    Vec2::new(-10.0, -8.0),
                ))),
                MeshMaterial2d(materials.add(color)),
            ));
        } else {
            entity.insert(Sprite::from_atlas_image(
                assets.ships.clone(),
                TextureAtlas {
                    layout: assets.ships_layout.clone(),
                    index: frame,
                },
            ));
        }
        info!(
            ship_id = state.ship_id,
            kind = ?state.kind,
            "navio visível no horizonte"
        );
    }
}

/// Last-known-state (MF-031): visual que parou de aparecer no snapshot
/// permanece por TTL curto e depois sai — sem pop brusco na fronteira do AOI.
pub fn expire_stale_visuals(
    mut commands: Commands,
    ships: Query<(Entity, &ShipVisual)>,
    wrecks: Query<(Entity, &WreckVisual)>,
) {
    for (entity, visual) in &ships {
        if visual.last_seen.elapsed().as_secs_f32() > STALE_VISUAL_TTL {
            commands.entity(entity).despawn();
        }
    }
    for (entity, visual) in &wrecks {
        if visual.last_seen.elapsed().as_secs_f32() > STALE_VISUAL_TTL {
            commands.entity(entity).despawn();
        }
    }
}

pub fn lerp_ship_visuals(time: Res<Time>, mut ships: Query<(&mut Transform, &ShipVisual)>) {
    // Fator por segundo independente de framerate; ~90% da diferença
    // desaparece em ~0.15s, o bastante para disfarçar 30 Hz sem atrasar.
    let factor = 1.0 - (-20.0 * time.delta_secs()).exp();
    for (mut transform, visual) in &mut ships {
        apply_lerp(
            &mut transform,
            &visual.target.x,
            &visual.target.y,
            &visual.target.heading,
            factor,
            SHIP_HEADING_OFFSET,
        );
    }
}

/// Entidade visual de um projétil autoritativo (PRD §20: client interpola).
#[derive(Component)]
pub struct ProjectileVisual {
    pub target: ProjectileState,
}

pub fn upsert_projectile_visuals(
    mut commands: Commands,
    mut snapshot_events: EventReader<ClientReceiveMessage<WorldSnapshot>>,
    mut existing: Query<(Entity, &mut ProjectileVisual)>,
    assets: Res<GameAssets>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Some(event) = snapshot_events.read().last() else {
        return;
    };
    let message = event.message();
    let seen: HashSet<u32> = message
        .projectiles
        .iter()
        .map(|p| p.projectile_id)
        .collect();

    // Projéteis que saíram do snapshot (impacto ou expiração) somem.
    for (entity, visual) in existing.iter() {
        if !seen.contains(&visual.target.projectile_id) {
            commands.entity(entity).despawn();
        }
    }

    for state in &message.projectiles {
        let mut updated = false;
        for (_, mut visual) in existing.iter_mut() {
            if visual.target.projectile_id == state.projectile_id {
                visual.target = *state;
                updated = true;
                break;
            }
        }
        if updated {
            continue;
        }
        let mut entity = commands.spawn((
            ProjectileVisual { target: *state },
            Transform {
                translation: Vec3::new(state.x, state.y, layers::PROJECTILES),
                rotation: Quat::from_rotation_z(state.heading),
                scale: Vec3::splat(0.3),
            },
        ));
        if image_failed(&asset_server, &assets.ships) {
            entity.insert((
                Mesh2d(meshes.add(Circle::new(2.5))),
                MeshMaterial2d(materials.add(Color::srgb(0.15, 0.15, 0.18))),
            ));
        } else {
            entity.insert(Sprite::from_atlas_image(
                assets.ships.clone(),
                TextureAtlas {
                    layout: assets.ships_detail_layout.clone(),
                    index: frames::PROJECTILE,
                },
            ));
        }
    }
}

pub fn lerp_projectile_visuals(
    time: Res<Time>,
    mut projectiles: Query<(&mut Transform, &ProjectileVisual)>,
) {
    // Projéteis voam rápido: lerp mais agressivo que navios.
    let factor = 1.0 - (-40.0 * time.delta_secs()).exp();
    for (mut transform, projectile) in &mut projectiles {
        apply_lerp(
            &mut transform,
            &projectile.target.x,
            &projectile.target.y,
            &projectile.target.heading,
            factor,
            0.0,
        );
    }
}

fn apply_lerp(
    transform: &mut Transform,
    x: &f32,
    y: &f32,
    heading: &f32,
    factor: f32,
    heading_offset: f32,
) {
    transform.translation = transform
        .translation
        .lerp(Vec3::new(*x, *y, transform.translation.z), factor);
    let target_rotation = Quat::from_rotation_z(*heading + heading_offset);
    transform.rotation = transform.rotation.slerp(target_rotation, factor);
}

/// Destroço flutuando no mar (PRD §26): carga esperando um saqueador.
/// Aparece/desaparece pelo snapshot AOI (protocolo v8) com TTL visual.
#[derive(Component)]
pub struct WreckVisual {
    pub wreck_num: u32,
    pub last_seen: Instant,
}

/// Wrecks vindos do snapshot do destinatário (MF-031): upsert de visuais e
/// reconstrução do `KnownWrecks` (o saque do client usa as posições).
#[allow(clippy::too_many_arguments)]
pub fn upsert_wreck_visuals(
    mut commands: Commands,
    mut snapshot_events: EventReader<ClientReceiveMessage<WorldSnapshot>>,
    mut known: ResMut<crate::net::KnownWrecks>,
    mut existing: Query<(Entity, &mut WreckVisual)>,
    assets: Res<GameAssets>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Some(event) = snapshot_events.read().last() else {
        return;
    };
    known.0.clear();
    for wreck in &event.message().wrecks {
        known.0.insert(wreck.wreck_id, Vec2::new(wreck.x, wreck.y));
        let mut updated = false;
        for (_, mut visual) in existing.iter_mut() {
            if visual.wreck_num == wreck.wreck_id {
                visual.last_seen = Instant::now();
                updated = true;
                break;
            }
        }
        if updated {
            continue;
        }
        let mut entity = commands.spawn((
            WreckVisual {
                wreck_num: wreck.wreck_id,
                last_seen: Instant::now(),
            },
            Transform::from_xyz(wreck.x, wreck.y, layers::WRECKS),
        ));
        if image_failed(&asset_server, &assets.wreck) {
            entity.insert((
                Mesh2d(meshes.add(Rectangle::new(14.0, 14.0))),
                MeshMaterial2d(materials.add(Color::srgb(0.38, 0.27, 0.16))),
            ));
        } else {
            entity.insert(Sprite::from_atlas_image(
                assets.wreck.clone(),
                TextureAtlas {
                    layout: assets.water_and_islands_layout.clone(),
                    index: frames::WRECK,
                },
            ));
        }
        info!(wreck_id = wreck.wreck_id, "destroço visível no mar");
    }
}

/// A câmera segue o próprio navio: o mar é grande e o HUD é filho da
/// câmera — navegar sem isso é assistir o casco sumir do quadro.
pub fn follow_camera(
    time: Res<Time>,
    my_ship: Res<crate::net::MyShip>,
    visuals: Query<&ShipVisual>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    let Ok(mut transform) = camera.get_single_mut() else {
        return;
    };
    let Some(my_id) = my_ship.0 else { return };
    let Some(visual) = visuals.iter().find(|visual| visual.target.ship_id == my_id) else {
        return;
    };
    // Perseguição suave: o lerp mais lento que o dos navios dá sensação de
    // câmera de mastro, não de Railcam colada no casco.
    let factor = 1.0 - (-6.0 * time.delta_secs()).exp();
    transform.translation = transform
        .translation
        .lerp(Vec3::new(visual.target.x, visual.target.y, 0.0), factor);
}
