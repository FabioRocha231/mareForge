//! Controlador local do navio do jogador (PRD MF-003). O movimento é
//! calculado exclusivamente pelo modelo puro de `domain-ships` — este módulo
//! só traduz teclado em intenção e estado em visual. Quando a réplica
//! autoritativa entrar (MF-006), este controlador vira predição de client.

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use mareforge_domain_items::ItemCatalog;
use mareforge_domain_ships::{
    compute_ship_stats, step_motion, EquippedComponents, MotionInput, MotionTuning, ShipDefinition,
    ShipKind, ShipMotion, ShipStats, SlotKind, SlotSpec,
};
use mareforge_shared::ids::ShipDefinitionId;

/// Navio controlado localmente neste client.
#[derive(Component)]
pub struct LocalShip {
    pub stats: ShipStats,
    pub motion: ShipMotion,
    pub tuning: MotionTuning,
}

pub fn spawn_local_ship(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let definition = placeholder_small_merchant();
    let stats = compute_ship_stats(
        &definition,
        &EquippedComponents::default(),
        &ItemCatalog::default(),
    )
    .expect("stats de navio sem equipamento não podem falhar");

    // Placeholder visual (PRD MF-003): triângulo de madeira apontando para
    // +X, mesma convenção de heading do modelo de movimento.
    let hull = meshes.add(Mesh::from(Triangle2d::new(
        Vec2::new(16.0, 0.0),
        Vec2::new(-10.0, 8.0),
        Vec2::new(-10.0, -8.0),
    )));
    let wood = materials.add(Color::srgb(0.85, 0.72, 0.45));

    commands.spawn((
        LocalShip {
            stats,
            motion: ShipMotion::default(),
            tuning: MotionTuning::default(),
        },
        Mesh2d(hull),
        MeshMaterial2d(wood),
        Transform::default(),
    ));
}

/// Definição provisória do SmallMerchant até o catálogo de navios existir.
fn placeholder_small_merchant() -> ShipDefinition {
    ShipDefinition {
        id: ShipDefinitionId::new(),
        kind: ShipKind::SmallMerchant,
        display_name: String::from("Small Merchant"),
        slots: vec![
            SlotSpec {
                kind: SlotKind::Hull,
                accepts_tag: None,
            },
            SlotSpec {
                kind: SlotKind::Sail,
                accepts_tag: None,
            },
            SlotSpec {
                kind: SlotKind::Weapon,
                accepts_tag: None,
            },
        ],
        cargo_capacity: 100,
        base_speed: 6.0,
        base_turn_rate: 1.0,
        base_hp: 100,
        base_weapon_damage: 20,
        base_weapon_range: 50.0,
    }
}

/// Lê o teclado como intenção (PRD §63: ShipInput) e avança o modelo puro.
/// W = velas abertas, S = velas recolhidas, A/D = leme a bombordo/estibordo.
pub fn ship_input_and_motion(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut ships: Query<&mut LocalShip>,
) {
    let throttle = if keys.pressed(KeyCode::KeyW) {
        1.0
    } else {
        0.0
    };
    let turn = (keys.pressed(KeyCode::KeyA) as i32 - keys.pressed(KeyCode::KeyD) as i32) as f32;
    let input = MotionInput { throttle, turn };

    for mut ship in &mut ships {
        let LocalShip {
            stats,
            motion,
            tuning,
        } = ship.as_mut();
        step_motion(motion, stats, input, tuning, time.delta_secs());
    }
}

/// Copia o estado do modelo puro para a representação visual.
pub fn sync_ship_visual(mut ships: Query<(&LocalShip, &mut Transform)>) {
    for (ship, mut transform) in &mut ships {
        transform.translation = Vec3::new(ship.motion.x, ship.motion.y, 0.0);
        transform.rotation = Quat::from_rotation_z(ship.motion.heading);
    }
}
