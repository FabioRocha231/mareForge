//! Nós de recurso no servidor (PRD MF-018/019/020, Phase 6). O node é
//! server-authoritative: estoque, proximidade e porão são julgados aqui. O
//! layout é conteúdo dev — a distribuição regional é o que cria o triângulo
//! econômico (Pilar 2): madeira no Porto da Serra, minério no Porto da Mina,
//! coral raro na ilha sem lei.

use std::time::{Duration, Instant};

use bevy::ecs::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_items::{ItemCatalog, ItemInstance};
use mareforge_domain_world::ResourceNode;
use mareforge_protocol::{GatherNode, GatherResult, NodeState, NodeUpdated, NodesSnapshot};
use mareforge_shared::ids::{ItemDefinitionId, ItemInstanceId, ResourceNodeId};
use tracing::{info, warn};

use crate::net::{DevItems, ReliableChannel, ServerGatherPolicy, ServerShip};

/// Node autoritativo no mundo. `node_num` é a face protocolar (u32 estável
/// durante a sessão); `node.id` é a identidade de mundo.
#[derive(Component)]
pub struct ServerNode {
    pub node_num: u32,
    pub node: ResourceNode,
    /// Preenchido quando o estoque zera: quando `Instant::now()` alcançar,
    /// o depósito repovoa (Phase 6: respawn).
    pub respawn_at: Option<Instant>,
}

#[derive(Resource, Default)]
pub struct NodeIdCounter(pub u32);

/// Layout dev (PRD §6/§7): (nome do node, região, x, y, estoque máximo).
/// Coordenadas dentro das águas da própria região — madeira em Protected do
/// Porto da Serra, minério no Porto da Mina, coral em Lawless na ilha. O
/// "Bosque/Mina do Caminho" fica na saída de cada baía, na rota leste-oeste.
const NODE_LAYOUT: &[(&str, &str, f32, f32, u32)] = &[
    ("Bosque da Serra", "Porto da Serra", -700.0, 90.0, 60),
    ("Bosque da Serra", "Porto da Serra", -500.0, 130.0, 60),
    ("Bosque da Serra", "Porto da Serra", -690.0, -110.0, 60),
    ("Bosque da Serra", "Porto da Serra", -470.0, -70.0, 60),
    ("Bosque do Caminho", "Porto da Serra", -430.0, 20.0, 60),
    ("Mina Profunda", "Porto da Mina", 700.0, 90.0, 60),
    ("Mina Profunda", "Porto da Mina", 500.0, 130.0, 60),
    ("Mina Profunda", "Porto da Mina", 690.0, -110.0, 60),
    ("Mina Profunda", "Porto da Mina", 470.0, -70.0, 60),
    ("Mina do Caminho", "Porto da Mina", 430.0, 20.0, 60),
    ("Recife do Coral", "Ilha do Coral Negro", 0.0, 860.0, 30),
    ("Recife do Coral", "Ilha do Coral Negro", -120.0, 950.0, 30),
    ("Recife do Coral", "Ilha do Coral Negro", 130.0, 990.0, 30),
];

/// Recurso de cada região do slice (MF-020: disponibilidade distinta).
fn resource_of_region(region: &str, dev: &DevItems) -> Option<ItemDefinitionId> {
    match region {
        "Porto da Serra" => Some(dev.timber),
        "Porto da Mina" => Some(dev.ore),
        "Ilha do Coral Negro" => Some(dev.coral),
        _ => None,
    }
}

/// Spawna os nodes dev no mundo (Startup; recursos já inseridos no build).
pub fn spawn_dev_nodes(
    mut commands: Commands,
    map: Res<crate::net::ServerWorldMap>,
    dev: Res<DevItems>,
    policy: Res<ServerGatherPolicy>,
    mut node_ids: ResMut<NodeIdCounter>,
) {
    for (name, region_name, x, y, max_stock) in NODE_LAYOUT {
        let region = map
            .0
            .region_by_name(region_name)
            .unwrap_or_else(|_| panic!("mapa do slice declara a região {region_name}"));
        let resource = resource_of_region(region_name, &dev)
            .unwrap_or_else(|| panic!("região {region_name} tem recurso dev definido"));
        let node_num = node_ids.0;
        node_ids.0 += 1;
        commands.spawn((ServerNode {
            node_num,
            node: ResourceNode {
                id: ResourceNodeId::new(),
                name,
                x: *x,
                y: *y,
                region: region.id,
                resource,
                stock: *max_stock,
                max_stock: *max_stock,
            },
            respawn_at: None,
        },));
    }
    info!(
        nodes = NODE_LAYOUT.len(),
        radius = policy.0.interact_radius,
        "mundo semeado de recursos"
    );
}

/// Face protocolar de um node: estado + nome do recurso via catálogo.
fn node_state(node: &ResourceNode, num: u32, catalog: &ItemCatalog) -> Option<NodeState> {
    let definition = catalog.get(node.resource)?;
    Some(NodeState {
        node_id: num,
        x: node.x,
        y: node.y,
        resource_name: definition.display_name.clone(),
        stock: node.stock,
        max_stock: node.max_stock,
    })
}

/// Mundo inteiro de nodes para o client que acabou de dar hello.
pub fn nodes_snapshot(nodes: &Query<&ServerNode>, catalog: &ItemCatalog) -> NodesSnapshot {
    NodesSnapshot {
        nodes: nodes
            .iter()
            .filter_map(|server_node| node_state(&server_node.node, server_node.node_num, catalog))
            .collect(),
    }
}

/// Coleta (PRD MF-019): perto do node, com estoque e espaço de porão. O
/// servidor corta o pedido ao que couber — nada se perde no mar.
pub fn handle_gather(
    mut gather_events: EventReader<ServerReceiveMessage<GatherNode>>,
    mut connection_manager: ResMut<ConnectionManager>,
    dev: Res<DevItems>,
    policy: Res<ServerGatherPolicy>,
    mut metrics: ResMut<crate::net::Metrics>,
    mut ships: Query<&mut ServerShip>,
    mut nodes: Query<&mut ServerNode>,
) {
    for event in gather_events.read() {
        let client_id = event.from();
        let node_num = event.message().node_id;

        let Some(mut ship) = ships
            .iter_mut()
            .find(|ship| ship.client_id == Some(client_id))
        else {
            continue;
        };
        let Some(mut server_node) = nodes
            .iter_mut()
            .find(|server_node| server_node.node_num == node_num)
        else {
            warn!(node_num, "coleta de nó inexistente");
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &GatherResult {
                    node_id: node_num,
                    success: false,
                    gathered: 0,
                },
            );
            continue;
        };

        if !server_node
            .node
            .in_range(ship.motion.x, ship.motion.y, policy.0.interact_radius)
        {
            info!(node_num, "longe demais do nó para coletar");
            send_failure(&mut connection_manager, client_id, node_num);
            continue;
        }
        if server_node.node.is_depleted() {
            info!(node_num, "nó esgotado; aguarde o respawn");
            send_failure(&mut connection_manager, client_id, node_num);
            continue;
        }

        // Quanto cabe no porão? (fail-closed pelo catálogo — ADR-0006)
        let Some(definition) = dev.catalog.get(server_node.node.resource) else {
            warn!(node_num, "recurso do nó fora do catálogo; recusado");
            send_failure(&mut connection_manager, client_id, node_num);
            continue;
        };
        let free = ship
            .hold
            .free_weight(&dev.catalog)
            .expect("porão só contém definições do catálogo");
        let affordable = free / definition.base_weight.max(1);
        let amount = policy
            .0
            .amount_per_gather
            .min(server_node.node.stock)
            .min(affordable);
        if amount == 0 {
            info!(node_num, "porão cheio: coleta rejeitada");
            send_failure(&mut connection_manager, client_id, node_num);
            continue;
        }

        let taken = server_node.node.take(amount);
        ship.hold
            .insert(
                &dev.catalog,
                ItemInstance::new_resource(ItemInstanceId::new(), server_node.node.resource, taken),
            )
            .expect("cabe: o espaço foi conferido acima");
        metrics.items_gathered += u64::from(taken);
        if server_node.node.is_depleted() {
            server_node.respawn_at =
                Some(Instant::now() + Duration::from_secs_f32(policy.0.respawn_secs));
        }

        if let Some(state) = node_state(&server_node.node, node_num, &dev.catalog) {
            let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
                &NodeUpdated { node: state },
                NetworkTarget::All,
            );
        }
        let _ = connection_manager.send_message::<ReliableChannel, _>(
            client_id,
            &GatherResult {
                node_id: node_num,
                success: true,
                gathered: taken,
            },
        );
        info!(
            ship_id = ship.ship_id,
            node_num,
            gathered = taken,
            resource = %definition.display_name,
            "recursos coletados"
        );
    }
}

fn send_failure(connection_manager: &mut ConnectionManager, client_id: ClientId, node_id: u32) {
    let _ = connection_manager.send_message::<ReliableChannel, _>(
        client_id,
        &GatherResult {
            node_id,
            success: false,
            gathered: 0,
        },
    );
}

/// Respawn (Phase 6): depósito esgotado repovoa após a política.
pub fn respawn_nodes(
    mut connection_manager: ResMut<ConnectionManager>,
    dev: Res<DevItems>,
    mut nodes: Query<&mut ServerNode>,
) {
    let now = Instant::now();
    for mut server_node in &mut nodes {
        let Some(when) = server_node.respawn_at else {
            continue;
        };
        if now < when {
            continue;
        }
        server_node.node.refill();
        server_node.respawn_at = None;
        if let Some(state) = node_state(&server_node.node, server_node.node_num, &dev.catalog) {
            let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
                &NodeUpdated { node: state },
                NetworkTarget::All,
            );
        }
        info!(node_num = server_node.node_num, "nó repovoado");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::DEV_SPAWN;
    use mareforge_domain_world::WorldMap;

    /// A distribuição regional do slice tem que ser distinta (MF-020) e a
    /// geografia honesta: cada node dentro das águas da sua região, e o
    /// recurso raro só na ilha sem lei.
    #[test]
    fn layout_matches_triangular_economy() {
        let map = WorldMap::vertical_slice();
        let count = |region: &str| {
            NODE_LAYOUT
                .iter()
                .filter(|(_, region_name, ..)| *region_name == region)
                .count()
        };
        assert_eq!(count("Porto da Serra"), 5);
        assert_eq!(count("Porto da Mina"), 5);
        assert_eq!(count("Ilha do Coral Negro"), 3);

        // Cada node fica dentro de uma zona declarada da sua região —
        // madeira/minério em águas protegidas, coral em lawless.
        for (name, region_name, x, y, stock) in NODE_LAYOUT {
            let zone = map
                .zone_at(*x, *y)
                .unwrap_or_else(|_| panic!("node {name} fora do mar declarado"));
            let expected_tier = if *region_name == "Ilha do Coral Negro" {
                mareforge_domain_world::RiskTier::Lawless
            } else {
                mareforge_domain_world::RiskTier::Protected
            };
            assert_eq!(
                zone.tier, expected_tier,
                "node {name} ({x}, {y}) na zona errada"
            );
            assert!(*stock > 0);
        }
    }

    /// O node da saída da baía fica no caminho do spawn para leste — o dev
    /// smoke (AUTOSAIL) cruza em faixa de coleta sem manobra.
    #[test]
    fn route_node_is_on_the_dev_sail_path() {
        let (_, _, x, y, _) = NODE_LAYOUT
            .iter()
            .find(|(name, ..)| *name == "Bosque do Caminho")
            .expect("node do caminho existe");
        let (spawn_x, spawn_y) = DEV_SPAWN;
        assert!((*x - spawn_x).abs() < 200.0, "node perto da doca");
        assert!((*y - spawn_y).abs() < 30.0, "node na linha de navegação");
    }
}
