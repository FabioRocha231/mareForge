//! Networking autoritativo do servidor (PRD MF-006..MF-015, ADR-0002/0003).
//!
//! O servidor é a única fonte de verdade: aplica `ShipInput`/`FireBroadside`/
//! `LootWreck` dos clients nos modelos puros de `domain-ships`, `domain-combat`
//! e `domain-items` a cada tick de 30 Hz e transmite `WorldSnapshot`.
//! Handshake de versão segue o ADR-0011.
//!
//! Nota: canais e registros de mensagens devem ser um espelho exato do
//! `client/src/net.rs`.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_combat::{
    apply_damage, can_loot, is_expired, resolve_ship_destruction, BroadsideBattery, DamageOutcome,
    LootPolicy, Projectile, WeaponParams, WreckChest, WreckPolicy,
};
use mareforge_domain_items::{
    CargoHold, Custody, ItemCatalog, ItemDefinition, ItemInstance, ItemKind,
};
use mareforge_domain_ships::{
    compute_ship_stats, step_motion, EquippedComponents, MotionInput, MotionTuning, ShipMotion,
    ShipStats,
};
use mareforge_protocol::{
    AssignShip, ClientHello, FireBroadside, LootResult, LootWreck, ProjectileState, ServerWelcome,
    ShipDestroyed, ShipInput, ShipState, WorldSnapshot, WreckRemoved, WreckSpawned,
    PROTOCOL_VERSION,
};
use mareforge_shared::ids::{
    DestructionEventId, ItemDefinitionId, ItemInstanceId, ShipInstanceId, WreckId,
};
use smallvec::SmallVec;
use tracing::{info, warn};

pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5000);
const SIM_HZ: f64 = 30.0;

/// Canal confiável (handshake, tiro, ciclo de wreck e loot).
#[derive(Channel)]
pub struct ReliableChannel;

/// Canal não-confiável (inputs e snapshots: o próximo já substitui o anterior).
#[derive(Channel)]
pub struct UnreliableChannel;

fn shared_config() -> SharedConfig {
    SharedConfig {
        server_replication_send_interval: Duration::from_secs_f64(1.0 / SIM_HZ),
        client_replication_send_interval: Duration::from_secs_f64(1.0 / SIM_HZ),
        tick: TickConfig {
            tick_duration: Duration::from_secs_f64(1.0 / SIM_HZ),
        },
    }
}

/// Tuning de combate e saque (PRD §23: valores de balanceamento vivem em
/// configuração).
#[derive(Resource, Debug, Clone, Copy)]
pub struct CombatTuning {
    pub cooldown_secs: f32,
    pub projectile_speed: f32,
    pub muzzle_offset: f32,
    /// Raio do círculo de colisão de um navio (aprox. meia eslora), em metros.
    pub hit_radius: f32,
    /// Distância máxima para interagir com um wreck, em metros (PRD §27).
    pub interact_radius: f32,
}

impl Default for CombatTuning {
    fn default() -> Self {
        Self {
            cooldown_secs: 4.0,
            projectile_speed: 40.0,
            muzzle_offset: 5.0,
            hit_radius: 10.0,
            interact_radius: 30.0,
        }
    }
}

/// Catálogo dev (PRD §32/§39): nomes são conteúdo, não contrato arquitetural.
/// O mínimo para o loop econômico do slice ser observável.
#[derive(Resource)]
pub struct DevItems {
    pub catalog: ItemCatalog,
    pub timber: ItemDefinitionId,
}

impl DevItems {
    fn new() -> Self {
        let timber = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        catalog
            .register(ItemDefinition {
                id: timber,
                kind: ItemKind::Resource,
                equipment: None,
                max_stack: 100,
                base_weight: 2,
                tags: SmallVec::new(),
                display_name: String::from("Timber"),
            })
            .expect("catálogo dev não registra duplicatas");
        Self { catalog, timber }
    }
}

/// Wrappers de Resource: os tipos de domínio (`LootPolicy`, `WreckPolicy`)
/// não conhecem Bevy (ADR-0006); o servidor os amarra aqui.
#[derive(Resource, Clone, Copy)]
pub struct ServerLootPolicy(pub LootPolicy);

#[derive(Resource, Clone, Copy)]
pub struct ServerWreckPolicy(pub WreckPolicy);

pub struct ServerNetPlugin;

impl Plugin for ServerNetPlugin {
    fn build(&self, app: &mut App) {
        let net_config = NetConfig::Netcode {
            io: IoConfig {
                transport: ServerTransport::UdpSocket(SERVER_ADDR),
                ..default()
            },
            config: NetcodeConfig::default(),
        };
        app.add_plugins(ServerPlugins::new(ServerConfig {
            shared: shared_config(),
            net: vec![net_config],
            ..default()
        }));
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_resource::<CombatTuning>();
        app.init_resource::<ShipIdCounter>();
        app.init_resource::<ProjectileIdCounter>();
        app.init_resource::<WreckIdCounter>();
        app.insert_resource(ServerLootPolicy(LootPolicy::default()));
        app.insert_resource(ServerWreckPolicy(WreckPolicy::default()));
        app.insert_resource(DevItems::new());
        app.add_channel::<ReliableChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        });
        app.add_channel::<UnreliableChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
        });
        app.register_message::<ClientHello>(ChannelDirection::ClientToServer);
        app.register_message::<ShipInput>(ChannelDirection::ClientToServer);
        app.register_message::<FireBroadside>(ChannelDirection::ClientToServer);
        app.register_message::<LootWreck>(ChannelDirection::ClientToServer);
        app.register_message::<ServerWelcome>(ChannelDirection::ServerToClient);
        app.register_message::<AssignShip>(ChannelDirection::ServerToClient);
        app.register_message::<WorldSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<ShipDestroyed>(ChannelDirection::ServerToClient);
        app.register_message::<WreckSpawned>(ChannelDirection::ServerToClient);
        app.register_message::<WreckRemoved>(ChannelDirection::ServerToClient);
        app.register_message::<LootResult>(ChannelDirection::ServerToClient);
        app.add_systems(Startup, start_server);
        app.add_systems(
            FixedUpdate,
            (
                handle_connections,
                handle_hello,
                handle_input,
                handle_fire,
                handle_loot,
                simulate_and_snapshot,
                expire_wrecks,
            ),
        );
    }
}

fn start_server(mut commands: Commands) {
    commands.start_server();
    info!(addr = %SERVER_ADDR, "mareforge server listening");
}

/// Navio autoritativo: a única cópia do estado que vale (Pilar 4).
#[derive(Component)]
pub struct ServerShip {
    pub ship_id: u32,
    pub client_id: ClientId,
    /// Id numérico do dono (para janelas exclusivas de wreck).
    pub client_num: u64,
    /// Instância do casco (localização `ShipCargo` do porão referencia este id).
    pub ship_instance: ShipInstanceId,
    pub input: ShipInput,
    pub hp: u32,
    pub hold: CargoHold,
    pub battery: BroadsideBattery,
    pub stats: ShipStats,
    pub motion: ShipMotion,
    pub tuning: MotionTuning,
}

#[derive(Component)]
pub struct ServerProjectile(pub Projectile);

/// Wreck no mar (PRD §26): baú com o que sobreviveu, janela exclusiva ao
/// killer e expiração.
#[derive(Component)]
pub struct ServerWreck {
    pub wreck_num: u32,
    pub wreck_id: WreckId,
    pub chest: WreckChest,
    pub exclusive_looter: Option<u64>,
    pub spawned_at: Instant,
    pub x: f32,
    pub y: f32,
}

#[derive(Resource, Default)]
pub struct ShipIdCounter(pub u32);

#[derive(Resource, Default)]
pub struct ProjectileIdCounter(pub u32);

#[derive(Resource, Default)]
pub struct WreckIdCounter(pub u32);

fn spawn_ship_for(
    commands: &mut Commands,
    ship_ids: &mut ShipIdCounter,
    dev: &DevItems,
    client_id: ClientId,
) -> u32 {
    let ship_id = ship_ids.0;
    ship_ids.0 += 1;
    let client_num = match client_id {
        ClientId::Netcode(n) => n,
        _ => 0,
    };
    let definition = mareforge_domain_ships::ShipDefinition::small_merchant_placeholder();
    let stats = compute_ship_stats(
        &definition,
        &EquippedComponents::default(),
        &ItemCatalog::default(),
    )
    .expect("stats de navio sem equipamento não podem falhar");
    let ship_instance = ShipInstanceId::new();
    let mut hold = CargoHold::new(ship_instance, stats.cargo_capacity);
    // Carga dev (PRD §39): mercadoria de teste para o loop econômico —
    // afundou, a carga vai pro wreck e muda de dono.
    hold.insert(
        &dev.catalog,
        ItemInstance::new_resource(ItemInstanceId::new(), dev.timber, 10),
    )
    .expect("carga dev cabe no porão vazio");

    commands.spawn((ServerShip {
        ship_id,
        client_id,
        client_num,
        ship_instance,
        input: ShipInput {
            throttle: 0.0,
            turn: 0.0,
        },
        hp: stats.max_hp,
        hold,
        battery: BroadsideBattery::default(),
        stats,
        motion: ShipMotion::default(),
        tuning: MotionTuning::default(),
    },));
    ship_id
}

fn handle_connections(
    mut commands: Commands,
    mut connect: EventReader<ConnectEvent>,
    mut disconnect: EventReader<DisconnectEvent>,
    ships: Query<(Entity, &ServerShip)>,
) {
    for connection in connect.read() {
        info!(client = ?connection.client_id, "client conectado; aguardando ClientHello");
    }
    for event in disconnect.read() {
        info!(client = ?event.client_id, "client desconectado");
        for (entity, ship) in &ships {
            if ship.client_id == event.client_id {
                commands.entity(entity).despawn();
                info!(ship_id = ship.ship_id, "navio do client removido do mundo");
            }
        }
    }
}

/// Handshake (ADR-0011): só spawna navio para hello com versão atual.
fn handle_hello(
    mut commands: Commands,
    mut hello_events: EventReader<ServerReceiveMessage<ClientHello>>,
    mut connection_manager: ResMut<ConnectionManager>,
    dev: Res<DevItems>,
    ships: Query<&ServerShip>,
    mut ship_ids: ResMut<ShipIdCounter>,
) {
    for event in hello_events.read() {
        let client_id = event.from();
        let hello = event.message();

        if hello.protocol_version != PROTOCOL_VERSION {
            warn!(
                client = ?client_id,
                got = hello.protocol_version,
                expected = PROTOCOL_VERSION,
                "rejeitando client com protocolo incompatível"
            );
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &ServerWelcome {
                    protocol_version: PROTOCOL_VERSION,
                    accepted: false,
                },
            );
            continue;
        }

        let already_has_ship = ships.iter().any(|ship| ship.client_id == client_id);
        if already_has_ship {
            continue;
        }

        let ship_id = spawn_ship_for(&mut commands, &mut ship_ids, &dev, client_id);
        let _ = connection_manager.send_message::<ReliableChannel, _>(
            client_id,
            &ServerWelcome {
                protocol_version: PROTOCOL_VERSION,
                accepted: true,
            },
        );
        let _ = connection_manager
            .send_message::<ReliableChannel, _>(client_id, &AssignShip { ship_id });
        info!(client = ?client_id, ship_id, "navio autoritativo criado");
    }
}

/// Último input vence; validação/clamp acontece dentro do modelo puro.
fn handle_input(
    mut input_events: EventReader<ServerReceiveMessage<ShipInput>>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in input_events.read() {
        let client_id = event.from();
        for mut ship in &mut ships {
            if ship.client_id == client_id {
                ship.input = *event.message();
                break;
            }
        }
    }
}

/// Disparo de bordo (PRD MF-009): cooldown decide; o projétil nasce
/// server-authoritative a partir do estado real do navio.
fn handle_fire(
    mut commands: Commands,
    mut fire_events: EventReader<ServerReceiveMessage<FireBroadside>>,
    tuning: Res<CombatTuning>,
    mut projectile_ids: ResMut<ProjectileIdCounter>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in fire_events.read() {
        let client_id = event.from();
        let side = event.message().side;
        let Some(mut ship) = ships.iter_mut().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        if !ship.battery.try_fire(side, tuning.cooldown_secs) {
            continue; // recarregando: clique ignorado, sem spam de projétil
        }
        let projectile_id = projectile_ids.0;
        projectile_ids.0 += 1;
        let weapon = WeaponParams {
            damage: ship.stats.weapon_damage,
            speed: tuning.projectile_speed,
            range: ship.stats.weapon_range,
            muzzle_offset: tuning.muzzle_offset,
        };
        let projectile = Projectile::from_broadside(
            projectile_id,
            ship.ship_id,
            side,
            ship.motion.x,
            ship.motion.y,
            ship.motion.heading,
            weapon,
        );
        info!(
            ship_id = ship.ship_id,
            ?side,
            projectile_id,
            "broadside disparada"
        );
        commands.spawn((ServerProjectile(projectile),));
    }
}

/// Saque de wreck (PRD §27, MF-015): perto do casco, dentro da janela e com
/// porão — a transferência é atômica, tudo-ou-nada.
// System Bevy: params são injeção de dependência, não assinatura.
#[allow(clippy::too_many_arguments)]
fn handle_loot(
    mut commands: Commands,
    mut loot_events: EventReader<ServerReceiveMessage<LootWreck>>,
    mut connection_manager: ResMut<ConnectionManager>,
    dev: Res<DevItems>,
    wreck_policy: Res<ServerWreckPolicy>,
    tuning: Res<CombatTuning>,
    mut ships: Query<&mut ServerShip>,
    mut wrecks: Query<(Entity, &mut ServerWreck)>,
) {
    for event in loot_events.read() {
        let client_id = event.from();
        let wreck_num = event.message().wreck_id;

        let Some(mut ship) = ships.iter_mut().find(|ship| ship.client_id == client_id) else {
            continue;
        };
        let Some((wreck_entity, wreck)) = wrecks
            .iter_mut()
            .find(|(_, wreck)| wreck.wreck_num == wreck_num)
        else {
            warn!(wreck_num, "loot de wreck inexistente");
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &LootResult {
                    wreck_id: wreck_num,
                    success: false,
                },
            );
            continue;
        };

        let elapsed = wreck.spawned_at.elapsed().as_secs_f32();
        if !can_loot(
            elapsed,
            &wreck_policy.0,
            ship.client_num,
            wreck.exclusive_looter,
        ) {
            info!(wreck_num, "janela exclusiva do killer ainda ativa");
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &LootResult {
                    wreck_id: wreck_num,
                    success: false,
                },
            );
            continue;
        }

        let dx = ship.motion.x - wreck.x;
        let dy = ship.motion.y - wreck.y;
        if dx * dx + dy * dy > tuning.interact_radius * tuning.interact_radius {
            info!(wreck_num, "longe demais do wreck para saquear");
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &LootResult {
                    wreck_id: wreck_num,
                    success: false,
                },
            );
            continue;
        }

        // Transferência atômica: clona a intenção; só drena o baú se tudo couber.
        let incoming: Vec<Custody> = wreck.chest.items().to_vec();
        match ship.hold.take_all(&dev.catalog, incoming) {
            Ok(moved) => {
                let weight = moved
                    .iter()
                    .filter_map(|custody| {
                        dev.catalog
                            .get(custody.instance.definition)
                            .map(|def| def.base_weight * custody.instance.quantity)
                    })
                    .sum::<u32>();
                let stacks = moved.len() as u32;
                commands.entity(wreck_entity).despawn();
                let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
                    &WreckRemoved {
                        wreck_id: wreck_num,
                    },
                    NetworkTarget::All,
                );
                let _ = connection_manager.send_message::<ReliableChannel, _>(
                    client_id,
                    &LootResult {
                        wreck_id: wreck_num,
                        success: true,
                    },
                );
                info!(
                    ship_id = ship.ship_id,
                    wreck_num, stacks, weight, "carga mudou de dono: wreck saqueado"
                );
            }
            Err(e) => {
                warn!(wreck_num, error = %e, "porão sem espaço: loot rejeitado");
                let _ = connection_manager.send_message::<ReliableChannel, _>(
                    client_id,
                    &LootResult {
                        wreck_id: wreck_num,
                        success: false,
                    },
                );
            }
        }
    }
}

/// O tick autoritativo: move navios e projéteis, resolve impactos, destrói
/// com resolução de loot, e transmite o snapshot do mundo.
#[allow(clippy::too_many_arguments)]
fn simulate_and_snapshot(
    mut commands: Commands,
    mut connection_manager: ResMut<ConnectionManager>,
    mut counter: ResMut<crate::plugin::TickCounter>,
    mut ship_ids: ResMut<ShipIdCounter>,
    mut wreck_ids: ResMut<WreckIdCounter>,
    dev: Res<DevItems>,
    loot_policy: Res<ServerLootPolicy>,
    tuning: Res<CombatTuning>,
    time: Res<Time>,
    mut ships: Query<(Entity, &mut ServerShip)>,
    mut projectiles: Query<(Entity, &mut ServerProjectile)>,
) {
    let dt = time.delta_secs();
    let mut snapshot = WorldSnapshot {
        tick: u64::from(counter.0),
        ships: Vec::with_capacity(ships.iter().count()),
        projectiles: Vec::with_capacity(projectiles.iter().count()),
    };

    // 1. Física dos navios.
    for (_, mut ship) in &mut ships {
        let ServerShip {
            ship_id,
            input,
            stats,
            motion,
            tuning: motion_tuning,
            battery,
            hold,
            ..
        } = ship.as_mut();
        step_motion(
            motion,
            stats,
            MotionInput {
                throttle: input.throttle,
                turn: input.turn,
            },
            motion_tuning,
            dt,
        );
        battery.advance(dt);
        let cargo_weight = hold
            .used_weight(&dev.catalog)
            .expect("porão só contém definições do catálogo");
        snapshot.ships.push(ShipState {
            ship_id: *ship_id,
            x: motion.x,
            y: motion.y,
            heading: motion.heading,
            speed: motion.speed,
            cargo_weight,
        });
    }

    // 2. Projéteis: avançam, expiram, colidem (decisão imutável primeiro).
    let ship_positions: HashMap<u32, (f32, f32)> = ships
        .iter()
        .map(|(_, ship)| (ship.ship_id, (ship.motion.x, ship.motion.y)))
        .collect();

    let mut expired: Vec<Entity> = Vec::new();
    // (projétil, alvo ship_id, dano, dono do projétil ship_id)
    let mut impacts: Vec<(Entity, u32, u32, u32)> = Vec::new();
    for (projectile_entity, mut projectile) in &mut projectiles {
        projectile.0.advance(dt);
        if projectile.0.expired() {
            expired.push(projectile_entity);
            continue;
        }
        snapshot.projectiles.push(ProjectileState {
            projectile_id: projectile.0.projectile_id,
            x: projectile.0.x,
            y: projectile.0.y,
            heading: projectile.0.heading,
        });

        for (ship_id, (x, y)) in &ship_positions {
            if *ship_id == projectile.0.owner_ship_id {
                continue;
            }
            if projectile.0.hit_ship(*x, *y, tuning.hit_radius) {
                impacts.push((
                    projectile_entity,
                    *ship_id,
                    projectile.0.damage,
                    projectile.0.owner_ship_id,
                ));
                break; // um projétil atinge um navio só
            }
        }
    }

    // 3. Impactos e destruição com resolução de full loot (MF-013).
    for (projectile_entity, target_ship_id, damage, killer_ship_id) in impacts {
        commands.entity(projectile_entity).despawn();

        // Escopo: o borrow mutável do navio termina antes de reler a frota
        // (procura do killer) e antes do respawn.
        let sinking = {
            let Some((entity, mut ship)) = ships
                .iter_mut()
                .find(|(_, ship)| ship.ship_id == target_ship_id)
            else {
                continue;
            };
            match apply_damage(ship.hp, damage) {
                DamageOutcome::Survived { remaining_hp } => {
                    ship.hp = remaining_hp;
                    info!(
                        ship_id = target_ship_id,
                        damage,
                        hp = remaining_hp,
                        "impacto no casco"
                    );
                    None
                }
                DamageOutcome::Destroyed => {
                    info!(ship_id = target_ship_id, damage, "SHIP DESTROYED");
                    let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
                        &ShipDestroyed {
                            ship_id: target_ship_id,
                        },
                        NetworkTarget::All,
                    );

                    // Full loot (§22-§25): casco é perda total; parte da carga
                    // sobrevive e vira wreck. Equipamento entra quando existir
                    // (MF-021) — hoje a lista vem vazia.
                    let victim_client_id = ship.client_id;
                    let victim_x = ship.motion.x;
                    let victim_y = ship.motion.y;
                    let cargo: Vec<ItemInstance> = ship
                        .hold
                        .items()
                        .iter()
                        .map(|custody| custody.instance.clone())
                        .collect();
                    Some((entity, victim_client_id, victim_x, victim_y, cargo))
                }
            }
        };

        let Some((entity, victim_client_id, victim_x, victim_y, cargo)) = sinking else {
            continue;
        };

        let destruction = DestructionEventId::new();
        let outcome = resolve_ship_destruction(destruction, &[], &cargo, &loot_policy.0);
        info!(
            ship_id = target_ship_id,
            event = ?destruction,
            afundados = outcome.destroyed_items.len(),
            sobreviventes = outcome.wreck_items.len(),
            "resolução de loot determinística"
        );

        if !outcome.wreck_items.is_empty() {
            let wreck_num = wreck_ids.0;
            wreck_ids.0 += 1;
            let wreck_id = WreckId::new();
            let mut chest = WreckChest::new(wreck_id);
            for survivor in &outcome.wreck_items {
                chest.insert(*survivor, ItemInstanceId::new());
            }
            let exclusive = ships
                .iter()
                .find(|(_, candidate)| candidate.ship_id == killer_ship_id)
                .map(|(_, candidate)| candidate.client_num);
            commands.spawn((ServerWreck {
                wreck_num,
                wreck_id,
                chest,
                exclusive_looter: exclusive,
                spawned_at: Instant::now(),
                x: victim_x,
                y: victim_y,
            },));
            let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
                &WreckSpawned {
                    wreck_id: wreck_num,
                    x: victim_x,
                    y: victim_y,
                    stack_count: outcome.wreck_items.len() as u32,
                },
                NetworkTarget::All,
            );
            info!(
                wreck_num,
                x = victim_x,
                y = victim_y,
                "wreck no mar aguardando saqueadores"
            );
        }

        commands.entity(entity).despawn();
        // Dev respawn (PRD §39): conveniência de teste — o loop do
        // vertical slice não pode matar a sessão do jogador. A regra
        // definitiva de reconstrução é o Dock (PRD §38, Phase 7).
        let new_ship_id = spawn_ship_for(&mut commands, &mut ship_ids, &dev, victim_client_id);
        let _ = connection_manager.send_message::<ReliableChannel, _>(
            victim_client_id,
            &AssignShip {
                ship_id: new_ship_id,
            },
        );
        info!(client = ?victim_client_id, new_ship_id, "dev respawn (PRD §39)");
    }

    for entity in expired {
        commands.entity(entity).despawn();
    }

    if !snapshot.ships.is_empty() || !snapshot.projectiles.is_empty() {
        let _ = connection_manager
            .send_message_to_target::<UnreliableChannel, _>(&snapshot, NetworkTarget::All);
    }

    counter.0 += 1;
}

/// Wrecks expirados somem do mar (PRD §26: 5 minutos; tuning no recurso).
fn expire_wrecks(
    mut commands: Commands,
    mut connection_manager: ResMut<ConnectionManager>,
    wreck_policy: Res<ServerWreckPolicy>,
    wrecks: Query<(Entity, &ServerWreck)>,
) {
    for (entity, wreck) in &wrecks {
        if is_expired(wreck.spawned_at.elapsed().as_secs_f32(), &wreck_policy.0) {
            info!(
                wreck_num = wreck.wreck_num,
                "wreck expirou e afundou de vez"
            );
            let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
                &WreckRemoved {
                    wreck_id: wreck.wreck_num,
                },
                NetworkTarget::All,
            );
            commands.entity(entity).despawn();
        }
    }
}
