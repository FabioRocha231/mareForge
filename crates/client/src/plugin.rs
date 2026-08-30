use bevy::prelude::*;

use crate::net::ClientNetPlugin;
use crate::ship::{
    lerp_projectile_visuals, lerp_ship_visuals, spawn_wreck_visuals, update_cargo_readout,
    upsert_projectile_visuals, upsert_ship_visuals, CargoReadout,
};
use crate::zone::ZonePlugin;

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.04, 0.13, 0.22)))
            // ADR-0008: simulação a 30 Hz; render desacoplado.
            .insert_resource(Time::<Fixed>::from_hz(30.0))
            .add_plugins(ClientNetPlugin)
            .add_plugins(ZonePlugin)
            .add_systems(Startup, (setup_camera, setup_hud))
            .add_systems(
                Update,
                (
                    upsert_ship_visuals,
                    lerp_ship_visuals,
                    upsert_projectile_visuals,
                    lerp_projectile_visuals,
                    spawn_wreck_visuals,
                    update_cargo_readout,
                    close_on_esc,
                ),
            );
    }
}

fn setup_camera(mut commands: Commands) {
    // 2 px por metro: o navio de 26 m ocupa 52 px e a velocidade de cruzeiro
    // (30 m/s) fica visível — 1:1 fazia o mar parecer congelado.
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 0.5,
            ..OrthographicProjection::default_2d()
        }),
    ));
}

fn setup_hud(mut commands: Commands) {
    commands.spawn((
        Text2d::new("W/S: velas · A/D: leme · Q/E: bordos · F: saquear · ESC: sair"),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.85, 0.85)),
        Transform::from_xyz(0.0, -120.0, 0.0),
    ));
    commands.spawn((
        Text2d::new("Carga: —"),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.8, 0.5)),
        Transform::from_xyz(0.0, 130.0, 0.0),
        CargoReadout,
    ));
}

fn close_on_esc(keys: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.send(AppExit::Success);
    }
}
