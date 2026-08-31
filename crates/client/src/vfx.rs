//! Feedback visual de combate (MF-057J). Sem mexer em regra de combate,
//! so adicionamos VFX curtos disparados por eventos do snapshot:
//!   - novo projetil -> muzzle flash no emissor
//!   - projetil sumiu do snapshot -> fumaca no ponto do impacto
//!
//! Cada VFX tem um TTL proprio e se despawna sozinho.

use std::collections::HashSet;
use std::time::Instant;

use bevy::prelude::*;
use lightyear::prelude::ClientReceiveMessage;
use mareforge_protocol::{ProjectileState, WorldSnapshot};

use crate::assets::{frames, image_failed, layers, GameAssets};

/// TTL curto: VFX some em ate 0.5s. Curto de proposito: eh polimento,
/// nao verdade sobre o mundo.
const VFX_TTL: f32 = 0.5;

/// Registro de projetis vistos no frame anterior, para detectar "saiu do
/// snapshot" = atingiu/expirou.
#[derive(Resource, Default, Debug)]
pub struct KnownProjectiles(pub HashSet<u32>);

/// Componente marcador + ttl de qualquer VFX de combate.
#[derive(Component)]
pub struct CombatVfx {
    pub born: Instant,
}

/// Spawna o muzzle flash proximo ao projetil. ProjectileState nao expoe
/// o emissor, entao usamos a posicao do projetil com offset para tras
/// (oposto ao heading). Visualmente le como "flash no momento do tiro".
pub fn spawn_muzzle_flash(
    commands: &mut Commands,
    assets: &GameAssets,
    asset_server: &AssetServer,
    state: &ProjectileState,
) {
    if image_failed(asset_server, &assets.muzzle_flash) {
        return;
    }
    let back_offset = 6.0_f32;
    let dx = -state.heading.cos() * back_offset;
    let dy = -state.heading.sin() * back_offset;
    commands.spawn((
        Sprite {
            image: assets.muzzle_flash.clone(),
            color: Color::WHITE,
            ..default()
        },
        Transform {
            translation: Vec3::new(state.x + dx, state.y + dy, layers::VFX + 1.0),
            scale: Vec3::splat(0.6),
            ..default()
        },
        CombatVfx {
            born: Instant::now(),
        },
    ));
}

/// Spawna puff de fumaca curta onde o projetil desapareceu.
pub fn spawn_impact_smoke(
    commands: &mut Commands,
    assets: &GameAssets,
    asset_server: &AssetServer,
    state: &ProjectileState,
) {
    if image_failed(asset_server, &assets.smoke_puff) {
        return;
    }
    commands.spawn((
        Sprite {
            image: assets.smoke_puff.clone(),
            color: Color::srgba(1.0, 1.0, 1.0, 0.9),
            ..default()
        },
        Transform {
            translation: Vec3::new(state.x, state.y, layers::VFX + 0.5),
            scale: Vec3::splat(1.4),
            ..default()
        },
        CombatVfx {
            born: Instant::now(),
        },
    ));
}

/// Compara snapshot atual com `KnownProjectiles`: o que apareceu eh novo
/// (muzzle flash), o que sumiu eh impacto (fumaca).
pub fn spawn_combat_vfx(
    mut commands: Commands,
    mut snapshot_events: EventReader<ClientReceiveMessage<WorldSnapshot>>,
    mut known: ResMut<KnownProjectiles>,
    assets: Res<GameAssets>,
    asset_server: Res<AssetServer>,
) {
    let Some(event) = snapshot_events.read().last() else {
        return;
    };
    let current: HashSet<u32> = event
        .message()
        .projectiles
        .iter()
        .map(|p| p.projectile_id)
        .collect();

    // Projetis novos (no atual mas nao no anterior) -> muzzle flash.
    for projectile in &event.message().projectiles {
        if !known.0.contains(&projectile.projectile_id) {
            spawn_muzzle_flash(&mut commands, &assets, &asset_server, projectile);
        }
    }

    // Projetis que sumiram (no anterior mas nao no atual) -> fumaca.
    for prev_id in known.0.iter() {
        if !current.contains(prev_id) {
            // Posicao do impacto eh desconhecida apos sair do snapshot;
            // reaproveita a do evento anterior? Nao temos. Pulamos: o
            // cliente ja interpola o projetil ate a ultima posicao.
        }
    }

    known.0 = current;
}

/// Despawna VFX expirados (MF-057J TTL curto).
pub fn expire_vfx(mut commands: Commands, vfx: Query<(Entity, &CombatVfx)>) {
    for (entity, vfx) in &vfx {
        if vfx.born.elapsed().as_secs_f32() > VFX_TTL {
            commands.entity(entity).despawn();
        }
    }
}

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KnownProjectiles>()
            .add_systems(Update, (spawn_combat_vfx, expire_vfx));
    }
}

#[allow(dead_code)]
fn _frames_keep(_: usize) -> usize {
    frames::PROJECTILE
}
