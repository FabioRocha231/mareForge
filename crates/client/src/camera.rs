//! Câmera do mar (MF-058): segue o próprio navio olhando um pouco à frente
//! da proa e aceita zoom pela roda do mouse. A UI vive em `bevy_ui` (espaço
//! de tela), então o zoom não mexe no HUD.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::ship::ShipVisual;

/// Metros por pixel de tela: 0.5 = o navio médio (40 m) ocupa 80 px.
const DEFAULT_ZOOM: f32 = 0.5;
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 1.4;

/// Zoom desejado; a projeção persegue suavemente.
#[derive(Resource)]
pub struct CameraZoom(pub f32);

impl Default for CameraZoom {
    fn default() -> Self {
        Self(DEFAULT_ZOOM)
    }
}

pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: DEFAULT_ZOOM,
            ..OrthographicProjection::default_2d()
        }),
        // Começa sobre o Porto da Serra para não abrir num mar vazio.
        Transform::from_xyz(-560.0, 0.0, 0.0),
    ));
}

pub fn zoom_from_wheel(mut wheel: EventReader<MouseWheel>, mut zoom: ResMut<CameraZoom>) {
    for event in wheel.read() {
        let steps = match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / 40.0,
        };
        zoom.0 = next_zoom(zoom.0, steps);
    }
}

/// Zoom multiplicativo: cada passo aproxima/afasta ~12%.
fn next_zoom(current: f32, steps: f32) -> f32 {
    (current * 0.88_f32.powf(steps)).clamp(MIN_ZOOM, MAX_ZOOM)
}

pub fn follow_camera(
    time: Res<Time>,
    zoom: Res<CameraZoom>,
    my_ship: Res<crate::net::MyShip>,
    visuals: Query<(&ShipVisual, &Transform), Without<Camera2d>>,
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera.get_single_mut() else {
        return;
    };
    let dt = time.delta_secs();
    if let Projection::Orthographic(ortho) = projection.as_mut() {
        ortho.scale += (zoom.0 - ortho.scale) * (1.0 - (-10.0 * dt).exp());
    }
    let Some(my_id) = my_ship.0 else { return };
    let Some((visual, ship)) = visuals
        .iter()
        .find(|(visual, _)| visual.target.ship_id == my_id)
    else {
        return;
    };
    // Olha à frente da proa, proporcional ao seguimento: quem navega rápido
    // precisa ver o que vem, não o que ficou.
    let lead = Vec2::from_angle(visual.target.heading) * visual.target.speed * 1.6;
    let goal = (ship.translation.truncate() + lead).extend(transform.translation.z);
    transform.translation = transform.translation.lerp(goal, 1.0 - (-3.5 * dt).exp());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_are_multiplicative_and_clamped() {
        assert!(next_zoom(0.5, 1.0) < 0.5);
        assert!(next_zoom(0.5, -1.0) > 0.5);
        assert_eq!(next_zoom(0.5, 100.0), MIN_ZOOM);
        assert_eq!(next_zoom(0.5, -100.0), MAX_ZOOM);
    }
}
