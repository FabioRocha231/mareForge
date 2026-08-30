use bevy::prelude::*;

use crate::net::ClientNetPlugin;
use crate::ship::{lerp_ship_visuals, upsert_ship_visuals};

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.04, 0.13, 0.22)))
            // ADR-0008: simulação a 30 Hz; render desacoplado.
            .insert_resource(Time::<Fixed>::from_hz(30.0))
            .add_plugins(ClientNetPlugin)
            .add_systems(Startup, (setup_camera, setup_hint))
            .add_systems(
                Update,
                (upsert_ship_visuals, lerp_ship_visuals, close_on_esc),
            );
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_hint(mut commands: Commands) {
    commands.spawn((
        Text2d::new("W/S: velas · A/D: leme · ESC: sair"),
        TextFont {
            font_size: 24.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.85, 0.85)),
        Transform::from_xyz(0.0, -240.0, 0.0),
    ));
}

fn close_on_esc(keys: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.send(AppExit::Success);
    }
}
