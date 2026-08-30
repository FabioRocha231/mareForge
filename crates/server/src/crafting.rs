//! Crafting no servidor (PRD §36-§38, MF-021/022). Catálogos dev (receitas
//! de equipamento e ordens de construção de navio), disponibilidade de
//! estações por área de porto (§5: portos são áreas de serviço) e o handler
//! do intent `CraftItem` — fail-closed ponta a ponta (§37).

use bevy::ecs::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_crafting::{
    can_construct, craft, Ingredient, Recipe, ShipConstructionJob, StationKind,
};
use mareforge_domain_items::ItemCatalog;
use mareforge_domain_ships::{ShipDefinition, ShipKind};
use mareforge_domain_world::WorldMap;
use mareforge_protocol::{AssignShip, CraftItem, CraftResult, RecipeEntry, RecipesSnapshot};
use mareforge_shared::ids::{ItemDefinitionId, RecipeId};
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

/// Disponibilidade de estação na posição (PRD §5/§7): porto é ÁREA de
/// serviço — aqui, as águas protegidas inteiras de cada baía. O Porto da
/// Serra tem Workbench + Dock; o Porto da Mina, Dock. Anvil não existe no
/// slice; mar sem lei e rota não têm estação alguma.
pub fn station_available(map: &WorldMap, x: f32, y: f32, required: StationKind) -> bool {
    match required {
        StationKind::None => true,
        StationKind::Anvil => false,
        StationKind::Workbench | StationKind::Dock => {
            let Ok(zone) = map.zone_at(x, y) else {
                return false;
            };
            if zone.tier != mareforge_domain_world::RiskTier::Protected {
                return false;
            }
            match required {
                StationKind::Workbench => zone.name == "Águas do Porto da Serra",
                _ => true, // Dock: as duas baías protegidas
            }
        }
    }
}

/// Estação "efetiva" para a regra pura: a exigida quando disponível, `None`
/// quando não — `can_craft` responde `WrongStation` (fail-closed).
fn effective_station(map: &WorldMap, x: f32, y: f32, required: StationKind) -> StationKind {
    if station_available(map, x, y, required) {
        required
    } else {
        StationKind::None
    }
}

/// Intent de fabricação/construção (PRD §63: CraftItem).
#[allow(clippy::too_many_arguments)]
pub fn handle_craft(
    mut commands: Commands,
    mut craft_events: EventReader<ServerReceiveMessage<CraftItem>>,
    mut connection_manager: ResMut<ConnectionManager>,
    dev: Res<DevItems>,
    dev_ships: Res<DevShips>,
    dev_recipes: Res<DevRecipes>,
    map: Res<ServerWorldMap>,
    mut ship_ids: ResMut<crate::net::ShipIdCounter>,
    mut ships: Query<(Entity, &mut ServerShip)>,
) {
    for event in craft_events.read() {
        let client_id = event.from();
        let recipe_num = event.message().recipe_id;
        let Some((ship_entity, mut ship)) = ships
            .iter_mut()
            .find(|(_, ship)| ship.client_id == client_id)
        else {
            continue;
        };

        // 1. Equipamento (MF-021): recursos → item de equipment no porão.
        if let Some(recipe) = dev_recipes.equipment_for(recipe_num) {
            let station = effective_station(
                &map.0,
                ship.motion.x,
                ship.motion.y,
                recipe.required_station,
            );
            match craft(recipe, &mut ship.hold, &dev.catalog, station) {
                Ok(output) => {
                    let name = dev
                        .catalog
                        .get(output.definition)
                        .map(|definition| definition.display_name.clone())
                        .unwrap_or_default();
                    info!(
                        ship_id = ship.ship_id,
                        recipe = %recipe.display_name,
                        output = %name,
                        "equipamento fabricado"
                    );
                    send_craft_result(&mut connection_manager, client_id, recipe_num, true);
                }
                Err(error) => {
                    warn!(
                        ship_id = ship.ship_id,
                        recipe = %recipe.display_name,
                        error = %error,
                        "craft recusado"
                    );
                    send_craft_result(&mut connection_manager, client_id, recipe_num, false);
                }
            }
            continue;
        }

        // 2. Navio (MF-022): Dock + recursos → ShipInstance nova.
        if let Some(job) = dev_recipes.ship_for(recipe_num) {
            let built = build_ship_for_job(
                &mut commands,
                &mut connection_manager,
                &dev,
                &dev_ships,
                &map.0,
                &mut ship_ids,
                job,
                ship_entity,
                &mut ship,
            );
            send_craft_result(&mut connection_manager, client_id, recipe_num, built);
            continue;
        }

        warn!(recipe_num, "receita desconhecida (fail-closed)");
        send_craft_result(&mut connection_manager, client_id, recipe_num, false);
    }
}

/// Valida e executa a construção: consome recursos do porão antigo, spawna o
/// navio novo na doca com a carga transferida, e aposenta o casco velho.
#[allow(clippy::too_many_arguments)]
fn build_ship_for_job(
    commands: &mut Commands,
    connection_manager: &mut ConnectionManager,
    dev: &DevItems,
    dev_ships: &DevShips,
    map: &WorldMap,
    ship_ids: &mut crate::net::ShipIdCounter,
    job: &ShipConstructionJob,
    old_entity: Entity,
    old_ship: &mut ServerShip,
) -> bool {
    let station = effective_station(
        map,
        old_ship.motion.x,
        old_ship.motion.y,
        job.required_station,
    );
    let mut quantities = std::collections::HashMap::new();
    for custody in old_ship.hold.items() {
        *quantities.entry(custody.instance.definition).or_insert(0) += custody.instance.quantity;
    }
    if let Err(error) = can_construct(job, &quantities, station) {
        warn!(
            ship_id = old_ship.ship_id,
            job = %job.display_name,
            error = %error,
            "construção recusada"
        );
        return false;
    }
    // A carga atual precisa caber no casco novo (§38: ShipInstance é
    // entidade própria — a carga migra, não some).
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
    // Consome os ingredientes do porão antigo (validado acima).
    for ingredient in &job.ingredients {
        old_ship
            .hold
            .remove(ingredient.item, ingredient.quantity)
            .expect("can_construct validou os ingredientes");
    }

    let victim_client_id = old_ship.client_id;
    let old_ship_id = old_ship.ship_id;
    commands.entity(old_entity).despawn();
    let new_ship_id = spawn_ship_for(
        commands,
        ship_ids,
        dev,
        dev_ships,
        map,
        job.kind,
        victim_client_id,
        cargo,
    );
    let _ = connection_manager.send_message::<ReliableChannel, _>(
        victim_client_id,
        &AssignShip {
            ship_id: new_ship_id,
        },
    );
    if let Some(zone) = crate::net::zone_changed_for(map, new_ship_id, DEV_SPAWN.0, DEV_SPAWN.1) {
        let _ = connection_manager.send_message::<ReliableChannel, _>(victim_client_id, &zone);
    }
    info!(
        old_ship_id,
        new_ship_id,
        job = %job.display_name,
        ?station,
        "navio construído no Dock"
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
    fn stations_follow_port_specialization() {
        let map = WorldMap::vertical_slice();
        let (serra_x, serra_y) = crate::net::DEV_SPAWN; // doca da Serra
        let (mina_x, mina_y) = (600.0, 0.0);
        let (sea_x, sea_y) = (2000.0, 900.0); // mar sem lei

        assert!(station_available(
            &map,
            serra_x,
            serra_y,
            StationKind::Workbench
        ));
        assert!(station_available(&map, serra_x, serra_y, StationKind::Dock));
        assert!(!station_available(
            &map,
            mina_x,
            mina_y,
            StationKind::Workbench
        ));
        assert!(station_available(&map, mina_x, mina_y, StationKind::Dock));
        assert!(!station_available(&map, sea_x, sea_y, StationKind::Dock));
        assert!(!station_available(&map, sea_x, sea_y, StationKind::Anvil));
        assert!(station_available(&map, sea_x, sea_y, StationKind::None));
    }
}
