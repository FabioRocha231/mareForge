use bevy::prelude::*;

use crate::crafting::{send_craft_input, CraftPlugin};
use crate::market::{send_market_input, MarketPlugin};
use crate::net::ClientNetPlugin;
use crate::nodes::NodePlugin;
use crate::ship::{
    expire_stale_visuals, lerp_projectile_visuals, lerp_ship_visuals, update_cargo_readout,
    upsert_projectile_visuals, upsert_ship_visuals, upsert_wreck_visuals, CargoReadout,
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
            .add_plugins(NodePlugin)
            .add_plugins(CraftPlugin)
            .add_plugins(MarketPlugin)
            .add_systems(Startup, (setup_camera, setup_hud).chain())
            .add_systems(
                Update,
                (
                    upsert_ship_visuals,
                    lerp_ship_visuals,
                    upsert_projectile_visuals,
                    lerp_projectile_visuals,
                    upsert_wreck_visuals,
                    expire_stale_visuals,
                    update_cargo_readout,
                    crate::ship::follow_camera,
                    send_craft_input,
                    send_market_input,
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

/// HUD filho da câmera: a câmera segue o navio (follow_camera) e o HUD vai
/// junto — texto em coordenada de mundo some quando se navega.
fn setup_hud(mut commands: Commands, camera: Query<Entity, With<Camera2d>>) {
    let Ok(camera) = camera.get_single() else {
        return;
    };
    let hud = |commands: &mut Commands, text: &str, size: f32, color: Color, y: f32| {
        commands
            .spawn((
                Text2d::new(text.to_owned()),
                TextFont {
                    font_size: size,
                    ..default()
                },
                TextColor(color),
                Transform::from_xyz(0.0, y, 10.0),
            ))
            .set_parent(camera)
            .id()
    };
    hud(
        &mut commands,
        "W/S velas · A/D leme · Q/R bordos · E atracar · T/Y/U equipar · Shift+T/Y/U desequipar (atracado) · F saquear · G coletar · 1-9 oficina · Z/X storage · V/N/B mercado · ESC sair",
        11.0,
        Color::srgb(0.85, 0.85, 0.85),
        -170.0,
    );
    let cargo = hud(
        &mut commands,
        "Carga: —",
        13.0,
        Color::srgb(0.95, 0.8, 0.5),
        140.0,
    );
    commands.entity(cargo).insert(CargoReadout);

    // Painel de mercado (carteira + quadro): canto direito da tela.
    commands
        .spawn((
            Text2d::new("Ouro: —"),
            TextFont {
                font_size: 10.0,
                ..default()
            },
            TextColor(Color::srgb(0.95, 0.87, 0.55)),
            Transform::from_xyz(210.0, 160.0, 10.0),
            crate::market::MarketReadout,
        ))
        .set_parent(camera);
}

fn close_on_esc(keys: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.send(AppExit::Success);
    }
}
