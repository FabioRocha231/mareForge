use bevy::prelude::*;

use crate::assets::AssetManifestPlugin;
use crate::crafting::{send_craft_input, CraftPlugin};
use crate::hud::{setup_hud, toggle_sea_hud, HudPlugin};
use crate::market::{send_market_input, spawn_market_panel, MarketPlugin};
use crate::net::{ClientNetPlugin, MyDocked};
use crate::nodes::NodePlugin;
use crate::port_screen::PortPlugin;
use crate::ship::{
    expire_stale_visuals, lerp_projectile_visuals, lerp_ship_visuals, upsert_projectile_visuals,
    upsert_ship_visuals, upsert_wreck_visuals,
};
use crate::world::WorldVisualPlugin;
use crate::zone::ZonePlugin;

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.04, 0.10, 0.18)))
            // ADR-0008: simulacao a 30 Hz; render desacoplado.
            .insert_resource(Time::<Fixed>::from_hz(30.0))
            .add_plugins(AssetManifestPlugin)
            .add_plugins(ClientNetPlugin)
            .add_plugins(ZonePlugin)
            .add_plugins(WorldVisualPlugin)
            .add_plugins(NodePlugin)
            .add_plugins(CraftPlugin)
            .add_plugins(MarketPlugin)
            .add_plugins(PortPlugin)
            .add_plugins(HudPlugin)
            .add_systems(
                Startup,
                (setup_camera, setup_hud, spawn_market_panel).chain(),
            )
            .add_systems(
                Update,
                (
                    upsert_ship_visuals,
                    lerp_ship_visuals,
                    upsert_projectile_visuals,
                    lerp_projectile_visuals,
                    upsert_wreck_visuals,
                    expire_stale_visuals,
                    toggle_sea_hud,
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
    // (30 m/s) fica visivel - 1:1 fazia o mar parecer congelado.
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 0.5,
            ..OrthographicProjection::default_2d()
        }),
    ));
}

fn close_on_esc(
    keys: Res<ButtonInput<KeyCode>>,
    docked: Res<MyDocked>,
    mut exit: EventWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) && !docked.0 {
        exit.send(AppExit::Success);
    }
}
