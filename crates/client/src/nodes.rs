//! Nós de recurso no client (PRD MF-018, Phase 6). O node é desenhado do
//! snapshot do handshake e atualizado por deltas — o client nunca inventa
//! estoque (Pilar 4). Coleta é intenção (`GatherNode`); o servidor decide.

use std::collections::HashMap;

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::*;
use mareforge_protocol::{GatherResult, NodeState, NodeUpdated, NodesSnapshot};

use crate::assets::{frames, image_failed, layers, GameAssets};

/// Nós conhecidos: posição e estoque para a coleta (alvo do G / autogather).
#[derive(Resource, Debug, Default)]
pub struct KnownNodes(pub HashMap<u32, NodeInfo>);

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub pos: Vec2,
    pub stock: u32,
    pub resource_name: String,
}

/// Entidade visual de um node: forma + rótulo filho com nome e estoque.
#[derive(Component)]
pub struct NodeVisual {
    pub node_id: u32,
    /// Cor do recurso; o node esgotado escurece até o respawn.
    pub resource_color: Color,
}

/// Rótulo de estoque pendurado no node visual — casa pelo `node_id`.
#[derive(Component)]
pub struct NodeLabel {
    pub node_id: u32,
}

pub struct NodePlugin;

impl Plugin for NodePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KnownNodes>().add_systems(
            Update,
            (
                handle_nodes_snapshot,
                handle_node_updated,
                handle_gather_result,
            ),
        );
    }
}

fn resource_color(name: &str) -> Color {
    match name {
        "Madeira" => Color::srgb(0.45, 0.32, 0.18),
        "Minério" => Color::srgb(0.45, 0.5, 0.58),
        "Coral Negro" => Color::srgb(0.42, 0.2, 0.52),
        // Recurso novo do servidor antes de ganhar cor aqui: cinza honesto.
        _ => Color::srgb(0.5, 0.5, 0.5),
    }
}

fn resource_frame(name: &str) -> usize {
    match name {
        "Madeira" => frames::WOOD_NODE,
        "Minério" => frames::ORE_NODE,
        "Coral Negro" => frames::CORAL_NODE,
        _ => frames::ORE_NODE,
    }
}

fn dimmed(color: Color) -> Color {
    let srgba = color.to_srgba();
    Color::srgba(srgba.red * 0.35, srgba.green * 0.35, srgba.blue * 0.35, 1.0)
}

fn spawn_node_visual(
    commands: &mut Commands,
    assets: &GameAssets,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    state: &NodeState,
) {
    let color = resource_color(&state.resource_name);
    let label = format!(
        "{} {}/{}",
        state.resource_name, state.stock, state.max_stock
    );
    let mut entity = commands.spawn((
        NodeVisual {
            node_id: state.node_id,
            resource_color: color,
        },
        Transform::from_xyz(state.x, state.y, layers::RESOURCES),
    ));
    if image_failed(asset_server, &assets.water_and_islands) {
        entity.insert((
            Mesh2d(meshes.add(Circle::new(9.0))),
            MeshMaterial2d(materials.add(color)),
        ));
    } else {
        entity.insert(Sprite {
            color,
            image: assets.water_and_islands.clone(),
            texture_atlas: Some(TextureAtlas {
                layout: assets.water_and_islands_layout.clone(),
                index: resource_frame(&state.resource_name),
            }),
            ..default()
        });
    }
    entity.with_children(|parent| {
        parent.spawn((
            NodeLabel {
                node_id: state.node_id,
            },
            Text2d::new(label),
            TextFont {
                font_size: 9.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.88, 0.8)),
            Transform::from_xyz(0.0, -16.0, layers::LABELS - layers::RESOURCES),
        ));
    });
}

/// Estado completo no handshake (depois disso, só deltas).
#[allow(clippy::too_many_arguments)]
fn handle_nodes_snapshot(
    mut commands: Commands,
    mut snapshot_events: EventReader<ClientReceiveMessage<NodesSnapshot>>,
    mut known: ResMut<KnownNodes>,
    assets: Res<GameAssets>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    existing: Query<&NodeVisual>,
) {
    for event in snapshot_events.read() {
        for state in &event.message().nodes {
            known.0.insert(
                state.node_id,
                NodeInfo {
                    pos: Vec2::new(state.x, state.y),
                    stock: state.stock,
                    resource_name: state.resource_name.clone(),
                },
            );
            if existing
                .iter()
                .any(|visual| visual.node_id == state.node_id)
            {
                continue;
            }
            spawn_node_visual(
                &mut commands,
                &assets,
                &asset_server,
                &mut meshes,
                &mut materials,
                state,
            );
        }
        info!(
            nodes = event.message().nodes.len(),
            "mapa de recursos recebido"
        );
    }
}

/// Delta de node (coleta de outro jogador, ou o próprio respawn).
fn handle_node_updated(
    mut updated_events: EventReader<ClientReceiveMessage<NodeUpdated>>,
    mut known: ResMut<KnownNodes>,
    mut nodes: Query<(&NodeVisual, Option<&mut Sprite>)>,
    mut labels: Query<(&mut Text2d, &NodeLabel)>,
) {
    for event in updated_events.read() {
        let state = &event.message().node;
        known.0.insert(
            state.node_id,
            NodeInfo {
                pos: Vec2::new(state.x, state.y),
                stock: state.stock,
                resource_name: state.resource_name.clone(),
            },
        );
        let Some((visual, sprite)) = nodes
            .iter_mut()
            .find(|(visual, _)| visual.node_id == state.node_id)
        else {
            continue;
        };
        // Esgotado escurece; repovoado volta à cor do recurso.
        let tint = if state.stock == 0 {
            dimmed(visual.resource_color)
        } else {
            visual.resource_color
        };
        if let Some(mut sprite) = sprite {
            sprite.color = tint;
        }
        let label_text = format!(
            "{} {}/{}",
            state.resource_name, state.stock, state.max_stock
        );
        for (mut text, label) in &mut labels {
            if label.node_id == state.node_id {
                text.0 = label_text.clone();
            }
        }
    }
}

fn handle_gather_result(mut events: EventReader<ClientReceiveMessage<GatherResult>>) {
    for event in events.read() {
        let result = event.message();
        if result.success {
            info!(
                node_id = result.node_id,
                gathered = result.gathered,
                "coleta no porão"
            );
        } else {
            warn!(node_id = result.node_id, "coleta recusada pelo servidor");
        }
    }
}
