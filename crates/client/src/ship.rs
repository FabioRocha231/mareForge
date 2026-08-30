//! Visual dos navios replicados (PRD MF-006). Cada `ShipState` do snapshot
//! tem uma entidade visual aqui; o client faz lerp suave entre snapshots.
//! Sem predição local — a verdade é o servidor (Pilar 4).

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::ClientReceiveMessage;
use mareforge_protocol::{ShipState, WorldSnapshot};

/// Entidade visual de um navio autoritativo. `target` é o último estado
/// autoritativo conhecido (alvo do lerp visual).
#[derive(Component)]
pub struct ShipVisual {
    pub target: ShipState,
}

pub fn upsert_ship_visuals(
    mut commands: Commands,
    my_ship: Res<crate::net::MyShip>,
    mut snapshot_events: EventReader<ClientReceiveMessage<WorldSnapshot>>,
    mut existing: Query<(Entity, &mut ShipVisual)>,
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
        let mut updated = false;
        for (_, mut visual) in existing.iter_mut() {
            if visual.target.ship_id == state.ship_id {
                visual.target = *state;
                updated = true;
                break;
            }
        }
        if updated {
            continue;
        }

        let is_mine = my_ship.0 == Some(state.ship_id);
        let hull = meshes.add(Mesh::from(Triangle2d::new(
            Vec2::new(16.0, 0.0),
            Vec2::new(-10.0, 8.0),
            Vec2::new(-10.0, -8.0),
        )));
        // Meu navio é madeira; os demais, bruma cinza-azulada.
        let color = if is_mine {
            Color::srgb(0.85, 0.72, 0.45)
        } else {
            Color::srgb(0.55, 0.62, 0.7)
        };
        let material = materials.add(color);
        commands.spawn((
            ShipVisual { target: *state },
            Mesh2d(hull),
            MeshMaterial2d(material),
            Transform::from_xyz(state.x, state.y, 0.0),
        ));
        info!(
            ship_id = state.ship_id,
            mine = is_mine,
            "navio visível no horizonte"
        );
    }
}

pub fn lerp_ship_visuals(time: Res<Time>, mut ships: Query<(&mut Transform, &ShipVisual)>) {
    // Fator por segundo independente de framerate; ~90% da diferença
    // desaparece em ~0.15s, o bastante para disfarçar 30 Hz sem atrasar.
    let factor = 1.0 - (-20.0 * time.delta_secs()).exp();
    for (mut transform, visual) in &mut ships {
        let target = &visual.target;
        let target_pos = Vec3::new(target.x, target.y, 0.0);
        transform.translation = transform.translation.lerp(target_pos, factor);
        let target_rotation = Quat::from_rotation_z(target.heading);
        transform.rotation = transform.rotation.slerp(target_rotation, factor);
    }
}
