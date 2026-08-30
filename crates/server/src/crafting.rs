//! Crafting no servidor (PRD §36-§38, MF-021/022). Catálogos dev (receitas
//! de equipamento e ordens de construção de navio), disponibilidade de
//! estações por área de porto (§5: portos são áreas de serviço) e o handler
//! do intent `CraftItem` — fail-closed ponta a ponta (§37).

use bevy::ecs::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_crafting::{
    can_construct, Ingredient, Recipe, ShipConstructionJob, StationKind,
};
use mareforge_domain_items::ItemCatalog;
use mareforge_domain_ships::{ShipDefinition, ShipKind, VesselPresence};
use mareforge_domain_world::WorldMap;
use mareforge_protocol::{AssignShip, CraftItem, CraftResult, RecipeEntry, RecipesSnapshot};
use mareforge_shared::ids::{ItemDefinitionId, RecipeId, RegionId};
use tracing::{info, warn};

use crate::net::{
    spawn_ship_for, DevItems, ReliableChannel, ServerShip, ServerWorldMap, DEV_SPAWN,
};

/// Registro dev de definições de navio (MF-022): os três cascos do §11.
/// Sem `Default` de propósito: cada instância carrega ids próprios, e um
/// default implícito esconderia isso (mesma razão do `new_without_default`).
#[allow(clippy::new_without_default)]
#[derive(Resource)]
pub struct DevShips {
    pub merchant: ShipDefinition,
    pub patrol: ShipDefinition,
    pub corsair: ShipDefinition,
}

impl DevShips {
    /// Sem `Default` de propósito (mesma razão do allow): cada instância
    /// carrega ids próprios de definição, e um default implícito esconderia
    /// a geração.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            merchant: ShipDefinition::small_merchant(),
            patrol: ShipDefinition::patrol(),
            corsair: ShipDefinition::corsair(),
        }
    }

    pub fn definition(&self, kind: ShipKind) -> &ShipDefinition {
        match kind {
            ShipKind::SmallMerchant => &self.merchant,
            ShipKind::Patrol => &self.patrol,
            ShipKind::Corsair => &self.corsair,
        }
    }
}

/// Catálogo dev de receitas (PRD §39: valores são balanceamento).
#[derive(Resource)]
pub struct DevRecipes {
    pub equipment: Vec<Recipe>,
    pub ships: Vec<ShipConstructionJob>,
}

impl DevRecipes {
    pub fn new(dev: &DevItems) -> Self {
        let ingredient = |item: ItemDefinitionId, quantity: u32| Ingredient { item, quantity };
        let equipment_recipe =
            |display_name: &str, output: ItemDefinitionId, ingredients: Vec<Ingredient>| Recipe {
                id: RecipeId::new(),
                display_name: String::from(display_name),
                output_item: output,
                output_quantity: 1,
                ingredients,
                required_station: StationKind::Workbench,
                craft_time_secs: 0,
            };

        let equipment = vec![
            equipment_recipe(
                "Casco Reforçado",
                dev.hull_plate,
                vec![ingredient(dev.timber, 15)],
            ),
            equipment_recipe(
                "Velas de Corrida",
                dev.racing_sails,
                vec![ingredient(dev.timber, 10), ingredient(dev.ore, 10)],
            ),
            equipment_recipe(
                "Canhão de Bronze",
                dev.bronze_cannon,
                vec![ingredient(dev.ore, 20), ingredient(dev.coral, 5)],
            ),
        ];

        let ships = vec![
            ShipConstructionJob {
                id: RecipeId::new(),
                display_name: String::from("Patrol"),
                kind: ShipKind::Patrol,
                ingredients: vec![ingredient(dev.timber, 30), ingredient(dev.ore, 30)],
                required_station: StationKind::Dock,
            },
            ShipConstructionJob {
                id: RecipeId::new(),
                display_name: String::from("Corsair"),
                kind: ShipKind::Corsair,
                ingredients: vec![ingredient(dev.ore, 40), ingredient(dev.coral, 10)],
                required_station: StationKind::Dock,
            },
        ];

        Self { equipment, ships }
    }

    /// Receita por número de protocolo: equipamento primeiro, navios depois.
    pub fn equipment_for(&self, num: u32) -> Option<&Recipe> {
        self.equipment.get(num as usize)
    }

    pub fn ship_for(&self, num: u32) -> Option<&ShipConstructionJob> {
        let offset = self.equipment.len() as u32;
        num.checked_sub(offset)
            .and_then(|index| self.ships.get(index as usize))
    }

    /// Catálogo completo para o client no handshake (MF-021/022).
    pub fn snapshot(&self, catalog: &ItemCatalog) -> RecipesSnapshot {
        let lines = |ingredients: &[Ingredient]| {
            ingredients
                .iter()
                .map(|ingredient| mareforge_protocol::IngredientLine {
                    name: catalog
                        .get(ingredient.item)
                        .map(|definition| definition.display_name.clone())
                        .unwrap_or_default(),
                    quantity: ingredient.quantity,
                })
                .collect()
        };
        let mut recipes: Vec<RecipeEntry> = self
            .equipment
            .iter()
            .enumerate()
            .map(|(num, recipe)| RecipeEntry {
                recipe_id: num as u32,
                display_name: recipe.display_name.clone(),
                station: recipe.required_station,
                ship_build: false,
                output_name: catalog
                    .get(recipe.output_item)
                    .map(|definition| definition.display_name.clone())
                    .unwrap_or_default(),
                output_quantity: recipe.output_quantity,
                ingredients: lines(&recipe.ingredients),
            })
            .collect();
        let offset = self.equipment.len() as u32;
        for (index, job) in self.ships.iter().enumerate() {
            recipes.push(RecipeEntry {
                recipe_id: offset + index as u32,
                display_name: job.display_name.clone(),
                station: job.required_station,
                ship_build: true,
                output_name: job.display_name.clone(),
                output_quantity: 1,
                ingredients: lines(&job.ingredients),
            });
        }
        RecipesSnapshot { recipes }
    }
}

/// Disponibilidade de estação no porto da região (PRD §5/§7, MF-036/037):
/// a oficina é do porto ONDE ESTÁ ATRACADO — água protegida não é doca. O
/// Porto da Serra tem Workbench + Dock; o Porto da Mina, Dock. Anvil não
/// existe no slice; região sem porto não tem estação alguma.
pub fn station_available(map: &WorldMap, region: RegionId, required: StationKind) -> bool {
    match required {
        StationKind::None => true,
        StationKind::Anvil => false,
        StationKind::Workbench | StationKind::Dock => {
            let Some(known) = map
                .regions()
                .iter()
                .find(|candidate| candidate.id == region)
            else {
                return false;
            };
            match required {
                StationKind::Workbench => known.name == "Porto da Serra",
                _ => known.port.is_some(), // Dock: toda região com porto
            }
        }
    }
}

/// Estação "efetiva" para a regra pura: a exigida quando disponível, `None`
/// quando não — `can_craft` responde `WrongStation` (fail-closed).
fn effective_station(map: &WorldMap, region: RegionId, required: StationKind) -> StationKind {
    if station_available(map, region, required) {
        required
    } else {
        StationKind::None
    }
}

/// Intent de fabricação/construção (PRD §63, MF-036/037). A oficina é
/// serviço de porto: só com o navio ATRACADO, e os insumos vêm do STORAGE
/// regional — o porão não é matéria-prima automática (Pilar 2).
#[allow(clippy::too_many_arguments)]
pub fn handle_craft(
    mut commands: Commands,
    mut craft_events: EventReader<ServerReceiveMessage<CraftItem>>,
    mut connection_manager: ResMut<ConnectionManager>,
    dev: Res<DevItems>,
    dev_ships: Res<DevShips>,
    dev_recipes: Res<DevRecipes>,
    mut metrics: ResMut<crate::net::Metrics>,
    map: Res<ServerWorldMap>,
    mut market: ResMut<crate::market::ServerMarket>,
    mut ship_ids: ResMut<crate::net::ShipIdCounter>,
    mut ships: Query<(Entity, &mut ServerShip)>,
) {
    for event in craft_events.read() {
        let client_id = event.from();
        let recipe_num = event.message().recipe_id;
        let Some((ship_entity, mut ship)) = ships
            .iter_mut()
            .find(|(_, ship)| ship.client_id == Some(client_id))
        else {
            continue;
        };

        // MF-036: sem doca, sem oficina.
        let VesselPresence::Docked(region) = ship.presence else {
            info!(
                ship_id = ship.ship_id,
                "craft recusado: atraca primeiro (E) — oficina é serviço de porto"
            );
            send_craft_result(&mut connection_manager, client_id, recipe_num, false);
            continue;
        };
        let character = ship.character;

        // 1. Equipamento (MF-021/037): insumos do storage → item no storage.
        if let Some(recipe) = dev_recipes.equipment_for(recipe_num) {
            let station = effective_station(&map.0, region, recipe.required_station);
            match market.craft_at_storage(character, region, recipe, &dev.catalog, station) {
                Ok(output) => {
                    metrics.items_crafted += u64::from(output.quantity.max(1));
                    let name = dev
                        .catalog
                        .get(output.definition)
                        .map(|definition| definition.display_name.clone())
                        .unwrap_or_default();
                    info!(
                        ship_id = ship.ship_id,
                        recipe = %recipe.display_name,
                        output = %name,
                        "equipamento fabricado no storage do porto"
                    );
                    send_craft_result(&mut connection_manager, client_id, recipe_num, true);
                }
                Err(error) => {
                    warn!(
                        ship_id = ship.ship_id,
                        recipe = %recipe.display_name,
                        error = %error,
                        "craft recusado (insumo no storage? deposite com Z)"
                    );
                    send_craft_result(&mut connection_manager, client_id, recipe_num, false);
                }
            }
            continue;
        }

        // 2. Navio (MF-022/037): Dock + insumos do storage → ShipInstance nova.
        if let Some(job) = dev_recipes.ship_for(recipe_num) {
            let built = build_ship_for_job(
                &mut commands,
                &mut connection_manager,
                &mut metrics,
                &dev,
                &dev_ships,
                &map.0,
                &mut market,
                &mut ship_ids,
                job,
                ship_entity,
                &mut ship,
                region,
            );
            send_craft_result(&mut connection_manager, client_id, recipe_num, built);
            continue;
        }

        warn!(recipe_num, "receita desconhecida (fail-closed)");
        send_craft_result(&mut connection_manager, client_id, recipe_num, false);
    }
}

/// Valida e executa a construção (MF-022/037): insumos vêm do STORAGE da
/// região, a carga embarcada do casco antigo migra para o novo (§38: ShipInstance
/// é entidade própria — a carga acompanha o dono, não some), e o casco velho
/// é aposentado. Fail-closed: validação antes de consumo, consumo antes de spawn.
#[allow(clippy::too_many_arguments)]
fn build_ship_for_job(
    commands: &mut Commands,
    connection_manager: &mut ConnectionManager,
    metrics: &mut crate::net::Metrics,
    dev: &DevItems,
    dev_ships: &DevShips,
    map: &WorldMap,
    market: &mut crate::market::ServerMarket,
    ship_ids: &mut crate::net::ShipIdCounter,
    job: &ShipConstructionJob,
    old_entity: Entity,
    old_ship: &mut ServerShip,
    region: RegionId,
) -> bool {
    let character = old_ship.character;
    let station = effective_station(map, region, job.required_station);
    // Insumos contam contra o STORAGE (MF-037) — não contra o porão.
    let mut quantities = std::collections::HashMap::new();
    for ingredient in &job.ingredients {
        let have = market.storage_quantity(character, region, ingredient.item);
        quantities.insert(ingredient.item, have);
    }
    if let Err(error) = can_construct(job, &quantities, station) {
        warn!(
            ship_id = old_ship.ship_id,
            job = %job.display_name,
            error = %error,
            "construção recusada (insumos no storage? deposite com Z)"
        );
        return false;
    }
    // A carga atual precisa caber no casco novo (§38) — ela migra inteira.
    let used = old_ship
        .hold
        .used_weight(&dev.catalog)
        .expect("porão só contém definições do catálogo");
    let new_definition = dev_ships.definition(job.kind);
    if used > new_definition.cargo_capacity {
        warn!(
            ship_id = old_ship.ship_id,
            job = %job.display_name,
            used,
            capacity = new_definition.cargo_capacity,
            "carga não cabe no casco novo; descarregue antes"
        );
        return false;
    }

    let cargo: Vec<_> = old_ship.hold.items().to_vec();
    // Consome os insumos do storage (validado acima).
    for ingredient in &job.ingredients {
        if let Err(error) =
            market.consume_from_storage(character, region, ingredient.item, ingredient.quantity)
        {
            warn!(error = %error, "consumo do storage falhou após validação; construção abortada");
            return false;
        }
    }

    let owner_client = old_ship.client_id;
    let owner_character = old_ship.character;
    let old_ship_id = old_ship.ship_id;
    commands.entity(old_entity).despawn();
    metrics.ships_constructed += 1;
    let new_ship_id = spawn_ship_for(
        commands,
        ship_ids,
        dev,
        dev_ships,
        map,
        job.kind,
        owner_client,
        owner_character,
        cargo,
    );
    if let Some(client_id) = owner_client {
        let _ = connection_manager.send_message::<ReliableChannel, _>(
            client_id,
            &AssignShip {
                ship_id: new_ship_id,
            },
        );
        if let Some(zone) = crate::net::zone_changed_for(map, new_ship_id, DEV_SPAWN.0, DEV_SPAWN.1)
        {
            let _ = connection_manager.send_message::<ReliableChannel, _>(client_id, &zone);
        }
    }
    info!(
        old_ship_id,
        new_ship_id,
        job = %job.display_name,
        ?station,
        "navio construído no Dock com insumos do storage"
    );
    true
}

fn send_craft_result(
    connection_manager: &mut ConnectionManager,
    client_id: ClientId,
    recipe_id: u32,
    success: bool,
) {
    let _ = connection_manager
        .send_message::<ReliableChannel, _>(client_id, &CraftResult { recipe_id, success });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §7: Serra tem Workbench + Dock; Mina, só Dock; mar aberto, nada;
    /// Anvil não existe no slice.
    #[test]
    fn stations_follow_port_specialization_by_region() {
        let map = WorldMap::vertical_slice();
        let serra = map.region_by_name("Porto da Serra").unwrap().id;
        let mina = map.region_by_name("Porto da Mina").unwrap().id;
        let ilha = map.region_by_name("Ilha do Coral Negro").unwrap().id;

        // Serra: Workbench + Dock (especialização de porto, §7).
        assert!(station_available(&map, serra, StationKind::Workbench));
        assert!(station_available(&map, serra, StationKind::Dock));
        // Mina: só Dock.
        assert!(!station_available(&map, mina, StationKind::Workbench));
        assert!(station_available(&map, mina, StationKind::Dock));
        // Ilha sem porto: nada (e Anvil não existe no slice).
        assert!(!station_available(&map, ilha, StationKind::Dock));
        assert!(!station_available(&map, ilha, StationKind::Workbench));
        assert!(!station_available(&map, serra, StationKind::Anvil));
        assert!(station_available(&map, serra, StationKind::None));
    }
}
