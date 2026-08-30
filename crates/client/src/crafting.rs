//! Crafting no client (PRD MF-021/022). O catálogo de receitas chega no
//! handshake; o client lista no HUD e envia `CraftItem` (teclas 1-9 ou
//! autocraft dev). Estação, ingredientes e porão são julgados pelo servidor.

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use mareforge_domain_crafting::StationKind;
use mareforge_protocol::{CraftItem, CraftResult, RecipeEntry, RecipesSnapshot};

use crate::net::ReliableChannel;

/// Receitas conhecidas (do RecipesSnapshot do handshake).
#[derive(Resource, Debug, Default)]
pub struct KnownRecipes(pub Vec<RecipeEntry>);

/// Linha do HUD de receitas, filha da câmera (fixa na tela).
#[derive(Component)]
pub struct RecipeLine {
    pub recipe_id: u32,
}

pub struct CraftPlugin;

impl Plugin for CraftPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KnownRecipes>()
            .add_systems(Update, (handle_recipes_snapshot, handle_craft_result));
    }
}

fn station_label(station: StationKind) -> &'static str {
    match station {
        StationKind::None => "mão livre",
        StationKind::Workbench => "Workbench",
        StationKind::Anvil => "Anvil",
        StationKind::Dock => "Dock",
    }
}

fn ingredient_summary(entry: &RecipeEntry) -> String {
    entry
        .ingredients
        .iter()
        .map(|line| format!("{}× {}", line.quantity, line.name))
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Catálogo do servidor chega: spawna a coluna de receitas, filha da
/// câmera (fixa na tela, canto esquerdo).
fn handle_recipes_snapshot(
    mut commands: Commands,
    mut snapshot_events: EventReader<ClientReceiveMessage<RecipesSnapshot>>,
    mut known: ResMut<KnownRecipes>,
    camera: Query<Entity, With<Camera2d>>,
) {
    for event in snapshot_events.read() {
        known.0 = event.message().recipes.clone();
        let Ok(camera) = camera.get_single() else {
            continue;
        };
        let header = commands
            .spawn((
                Text2d::new("Receitas (1-9):"),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(Color::srgb(0.75, 0.75, 0.7)),
                Transform::from_xyz(-310.0, 160.0, 10.0),
            ))
            .set_parent(camera)
            .id();
        let _ = header;
        for entry in &known.0 {
            let line = format!(
                "{}. {} [{}] ← {} → {}",
                entry.recipe_id + 1,
                entry.display_name,
                station_label(entry.station),
                ingredient_summary(entry),
                entry.output_name
            );
            commands
                .spawn((
                    RecipeLine {
                        recipe_id: entry.recipe_id,
                    },
                    Text2d::new(line),
                    TextFont {
                        font_size: 9.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.82, 0.7)),
                    Transform::from_xyz(-310.0, 148.0 - 12.0 * entry.recipe_id as f32, 10.0),
                ))
                .set_parent(camera);
        }
        info!(recipes = known.0.len(), "catálogo de receitas recebido");
    }
}

fn handle_craft_result(mut events: EventReader<ClientReceiveMessage<CraftResult>>) {
    for event in events.read() {
        let result = event.message();
        if result.success {
            info!(
                recipe_id = result.recipe_id,
                "fabricação concluída pelo servidor"
            );
        } else {
            warn!(
                recipe_id = result.recipe_id,
                "fabricação recusada (estação, ingredientes ou porão)"
            );
        }
    }
}

/// Teclas 1-9 fabricam a receita correspondente. Dev tooling (§39):
/// MAREFORGE_AUTOCRAFT=1 tenta a lista em ciclo — smoke sem interação.
pub fn send_craft_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    known: Res<KnownRecipes>,
    mut auto_timer: Local<f32>,
    mut auto_index: Local<u32>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    let number_keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let mut selected = number_keys
        .iter()
        .position(|key| keys.just_pressed(*key))
        .map(|index| index as u32);

    if autocraft_enabled() {
        // Ciclo de 1s: com 5 receitas, a mesma cai a cada 5s — em tempo de
        // estar com recurso e estação ao mesmo tempo (janela da baía).
        *auto_timer += time.delta_secs();
        if *auto_timer >= 1.0 && !known.0.is_empty() {
            *auto_timer = 0.0;
            selected = Some(*auto_index % known.0.len() as u32);
            *auto_index = (*auto_index + 1) % known.0.len() as u32;
        }
    }

    if let Some(recipe_id) = selected {
        if known.0.iter().any(|entry| entry.recipe_id == recipe_id) {
            info!(recipe_id, "fabricando");
            let _ = connection_manager.send_message::<ReliableChannel, _>(&CraftItem { recipe_id });
        }
    }
}

fn autocraft_enabled() -> bool {
    std::env::var_os("MAREFORGE_AUTOCRAFT").is_some()
}
