//! Efeitos visuais curtos (MF-057J, MF-058): partículas quadradas no estilo
//! pixel art (espuma, borrifo, lascas) e animações do atlas (fumaça de
//! canhão, fogo de impacto). Nada aqui é verdade de jogo: é polimento que
//! nasce de eventos do snapshot e se despawna sozinho.

use bevy::prelude::*;

use crate::assets::{layers, GameAssets};

/// Malhas e materiais compartilhados pelos efeitos e pela bala.
#[derive(Resource)]
pub struct VfxHandles {
    pub ball_mesh: Handle<Mesh>,
    pub ball_material: Handle<ColorMaterial>,
}

/// Partícula: quadrado que se move, cresce e desbota até morrer.
#[derive(Component, Clone, Copy)]
pub struct Particle {
    pub velocity: Vec2,
    pub drag: f32,
    pub life: f32,
    pub age: f32,
    pub size: (f32, f32),
    pub color: Color,
    pub z: f32,
}

impl Particle {
    /// Espuma de popa: abre devagar na esteira e some em ~1.8 s.
    pub fn foam(velocity: Vec2) -> Self {
        Self {
            velocity,
            drag: 1.2,
            life: 1.8,
            age: 0.0,
            size: (1.6, 4.0),
            color: Color::srgba(0.93, 0.97, 1.0, 0.55),
            z: layers::WAKE,
        }
    }

    /// Borrifo de bala que caiu na água.
    pub fn splash(velocity: Vec2) -> Self {
        Self {
            velocity,
            drag: 4.0,
            life: 0.7,
            age: 0.0,
            size: (2.2, 1.0),
            color: Color::srgba(0.95, 0.98, 1.0, 0.9),
            z: layers::VFX,
        }
    }

    /// Lasca de madeira de casco atingido.
    pub fn debris(velocity: Vec2) -> Self {
        Self {
            velocity,
            drag: 2.5,
            life: 0.9,
            age: 0.0,
            size: (1.8, 1.2),
            color: Color::srgba(0.45, 0.28, 0.14, 1.0),
            z: layers::VFX,
        }
    }
}

/// Animação de atlas que toca uma vez e some.
#[derive(Component)]
pub struct AtlasAnimation {
    first: usize,
    frames: usize,
    age: f32,
    duration: f32,
}

pub fn spawn_particle(commands: &mut Commands, at: Vec2, particle: Particle) {
    commands.spawn((
        Sprite::from_color(particle.color, Vec2::splat(particle.size.0)),
        Transform::from_translation(at.extend(particle.z)),
        particle,
    ));
}

/// Toca `frames` quadros do layout `ship_parts` a partir de `first`.
pub fn spawn_animation(
    commands: &mut Commands,
    assets: &GameAssets,
    first: usize,
    frames: usize,
    at: Vec2,
    duration: f32,
) {
    commands.spawn((
        Sprite::from_atlas_image(
            assets.ships.clone(),
            TextureAtlas {
                layout: assets.ship_parts.clone(),
                index: first,
            },
        ),
        Transform::from_translation(at.extend(layers::VFX + 0.5)).with_scale(Vec3::splat(0.8)),
        AtlasAnimation {
            first,
            frames,
            age: 0.0,
            duration,
        },
    ));
}

fn setup_vfx_handles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(VfxHandles {
        ball_mesh: meshes.add(Circle::new(1.6)),
        ball_material: materials.add(Color::srgb(0.1, 0.1, 0.12)),
    });
}

fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (entity, mut p, mut transform, mut sprite) in &mut particles {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(entity).despawn();
            continue;
        }
        let k = p.age / p.life;
        transform.translation += (p.velocity * dt).extend(0.0);
        let damping = (-p.drag * dt).exp();
        p.velocity *= damping;
        sprite.custom_size = Some(Vec2::splat(p.size.0 + (p.size.1 - p.size.0) * k));
        sprite.color = p.color.with_alpha(p.color.alpha() * (1.0 - k));
    }
}

fn update_atlas_animations(
    mut commands: Commands,
    time: Res<Time>,
    mut anims: Query<(Entity, &mut AtlasAnimation, &mut Sprite, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut anim, mut sprite, mut transform) in &mut anims {
        anim.age += dt;
        if anim.age >= anim.duration {
            commands.entity(entity).despawn();
            continue;
        }
        let frame = (anim.age / anim.duration * anim.frames as f32) as usize;
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = anim.first + frame.min(anim.frames - 1);
        }
        // Fumaça sobe e se espalha um pouco com o vento.
        transform.translation += Vec3::new(1.5, 2.5, 0.0) * dt;
    }
}

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_vfx_handles)
            .add_systems(Update, (update_particles, update_atlas_animations));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particles_fade_out_and_despawn() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, update_particles);
        let entity = app
            .world_mut()
            .spawn((
                Sprite::default(),
                Transform::default(),
                Particle {
                    life: 0.0,
                    ..Particle::foam(Vec2::X)
                },
            ))
            .id();
        app.update();
        app.update();
        assert!(app.world().get_entity(entity).is_err());
    }
}
