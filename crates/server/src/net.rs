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

use crate::npc::NpcShip;
use crate::sets::SimulationSet;
use bevy::ecs::prelude::*;
use bevy::prelude::*;
use chrono::Utc;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use mareforge_domain_combat::{
    apply_damage, can_loot, is_expired, resolve_ship_destruction, BroadsideBattery, DamageOutcome,
    LootPolicy, Projectile, WeaponParams, WreckChest, WreckPolicy,
};
use mareforge_domain_economy::MarketPriceIndex;
use mareforge_domain_items::{
    CargoHold, Custody, EquipmentDefinition, EquipmentSlot, EquipmentStats, ItemCatalog,
    ItemDefinition, ItemInstance, ItemKind,
};
use mareforge_domain_ships::{
    compute_ship_stats, dock as dock_vessel, step_motion, undock as undock_vessel, DockPolicy,
    EquippedComponents, MotionInput, MotionTuning, ShipKind, ShipLoadout, ShipMotion, ShipStats,
    VesselPresence,
};
use mareforge_domain_world::{GatheringPolicy, RiskPolicy, WorldMap};
use mareforge_protocol::{
    AssignShip, BuySellOrder, CancelSellOrder, CatalogSnapshot, ClientHello, CraftItem,
    CraftResult, CreateSellOrder, Dock, DockResult, EquipItem, FireBroadside, GatherNode,
    GatherResult, LoadoutResult, LoadoutSnapshot, LootResult, LootWreck, MarketResult, NodeUpdated,
    NodesSnapshot, OrdersSnapshot, PortStorageSnapshot, ProjectileState, RecipesSnapshot,
    ServerWelcome, ShipDestroyed, ShipInput, ShipState, StorageDepositAll, StorageLine,
    StorageWithdrawAll, Undock, UnequipItem, WalletUpdated, WorldSnapshot, WreckState, ZoneChanged,
    PROTOCOL_VERSION,
};
use mareforge_shared::ids::{
    CharacterId, DestructionEventId, ItemDefinitionId, ItemInstanceId, RegionId, ShipInstanceId,
    WreckId, ZoneId,
};
use smallvec::SmallVec;
use tracing::{info, warn};

pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5000);
const SIM_HZ: f64 = 30.0;
/// Frequência de snapshots de rede (ADR-0008): 20 Hz — MENOR que a simulação
/// (30 Hz, FixedUpdate). A cadência é explícita em [`advance_snapshot_clock`].
pub const SNAPSHOT_HZ: f64 = 20.0;
/// Janela de graça pós-desconexão (MF-035): o navio fica no mar, vulnerável;
/// reconnect dentro da janela reassume o controle. Não é fuga de PvP — é o
/// oposto: 60s de casco parado e exposto.
pub const DISCONNECT_GRACE_SECS: u64 = 60;

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
/// O mínimo para o loop econômico do slice ser observável — e a base da
/// especialização regional (§7): madeira no Porto da Serra, minério no Porto
/// da Mina, coral raro na ilha sem lei.
#[derive(Resource)]
pub struct DevItems {
    pub catalog: ItemCatalog,
    pub timber: ItemDefinitionId,
    pub ore: ItemDefinitionId,
    pub coral: ItemDefinitionId,
    pub hull_plate: ItemDefinitionId,
    pub racing_sails: ItemDefinitionId,
    pub bronze_cannon: ItemDefinitionId,
}

impl DevItems {
    fn new() -> Self {
        let timber = ItemDefinitionId::new();
        let ore = ItemDefinitionId::new();
        let coral = ItemDefinitionId::new();
        let hull_plate = ItemDefinitionId::new();
        let racing_sails = ItemDefinitionId::new();
        let bronze_cannon = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        let mut register = |definition: ItemDefinition| {
            catalog
                .register(definition)
                .expect("catálogo dev não registra duplicatas");
        };
        register(ItemDefinition {
            id: timber,
            kind: ItemKind::Resource,
            equipment: None,
            max_stack: 100,
            base_weight: 2,
            tags: SmallVec::new(),
            display_name: String::from("Madeira"),
        });
        register(ItemDefinition {
            id: ore,
            kind: ItemKind::Resource,
            equipment: None,
            max_stack: 100,
            base_weight: 3,
            tags: SmallVec::new(),
            display_name: String::from("Minério"),
        });
        register(ItemDefinition {
            id: coral,
            kind: ItemKind::Resource,
            equipment: None,
            max_stack: 20,
            base_weight: 1,
            tags: SmallVec::new(),
            display_name: String::from("Coral Negro"),
        });
        // Equipamento dev (MF-021/038): o que o Workbench produz, agora com
        // o SLOT que ocupa. Stats de velocidade/alcance usam offsets de
        // 0,01 unidade (§32).
        register(ItemDefinition {
            id: hull_plate,
            kind: ItemKind::Equipment,
            equipment: Some(EquipmentDefinition {
                slot: EquipmentSlot::Hull,
                stats: EquipmentStats {
                    damage: 0,
                    speed: 0,
                    cargo: 0,
                    hp: 40,
                    range: 0,
                },
            }),
            max_stack: 1,
            base_weight: 8,
            tags: SmallVec::new(),
            display_name: String::from("Casco Reforçado"),
        });
        register(ItemDefinition {
            id: racing_sails,
            kind: ItemKind::Equipment,
            equipment: Some(EquipmentDefinition {
                slot: EquipmentSlot::Sail,
                stats: EquipmentStats {
                    damage: 0,
                    speed: 600,
                    cargo: 0,
                    hp: 0,
                    range: 0,
                },
            }),
            max_stack: 1,
            base_weight: 5,
            tags: SmallVec::new(),
            display_name: String::from("Velas de Corrida"),
        });
        register(ItemDefinition {
            id: bronze_cannon,
            kind: ItemKind::Equipment,
            equipment: Some(EquipmentDefinition {
                slot: EquipmentSlot::Weapon,
                stats: EquipmentStats {
                    damage: 10,
                    speed: 0,
                    cargo: 0,
                    hp: 0,
                    range: 500,
                },
            }),
            max_stack: 1,
            base_weight: 10,
            tags: SmallVec::new(),
            display_name: String::from("Canhão de Bronze"),
        });
        Self {
            catalog,
            timber,
            ore,
            coral,
            hull_plate,
            racing_sails,
            bronze_cannon,
        }
    }
}

/// Wrappers de Resource: os tipos de domínio (`LootPolicy`, `WreckPolicy`)
/// não conhecem Bevy (ADR-0006); o servidor os amarra aqui.
#[derive(Resource, Clone, Copy)]
pub struct ServerLootPolicy(pub LootPolicy);

#[derive(Resource, Clone, Copy)]
pub struct ServerWreckPolicy(pub WreckPolicy);

/// Mapa autoritativo do mundo (MF-016/017): zonas de risco e regiões. O
/// servidor é quem calcula a zona real (PRD §10) — o client só desenha.
#[derive(Resource, Clone)]
pub struct ServerWorldMap(pub WorldMap);

/// Política de risco: PvP em Protected é switch explícito (o slice nunca
/// liga); Frontier/Lawless são full loot incondicional (§9).
#[derive(Resource, Clone, Copy)]
pub struct ServerRiskPolicy(pub RiskPolicy);

/// Política de coleta (MF-019): taxa, raio de interação e respawn.
#[derive(Resource, Clone, Copy)]
pub struct ServerGatherPolicy(pub GatheringPolicy);

/// Política de atracação (MF-036): velocidade máxima para lançar amarras.
#[derive(Resource, Clone, Copy, Default)]
pub struct ServerDockPolicy(pub DockPolicy);

/// Doca do Porto da Serra (mapa do triângulo, PRD §6): dentro das águas
/// protegidas. Jogadores nascem em segurança e escolhem quando se arriscar
/// (Pilar 3). O mapa fixa em teste que este ponto é Protected.
pub const DEV_SPAWN: (f32, f32) = (-560.0, 0.0);

/// Relógio do snapshot de rede (ADR-0008, MF-032): acumula o tempo simulado
/// e dispara o envio na cadência de [`SNAPSHOT_HZ`] (20 Hz), independente do
/// tick de 30 Hz. Sem arredondar para 15 ou 30: 3s = 90 ticks = 60 snapshots.
#[derive(Resource, Default)]
pub struct SnapshotClock {
    accumulator: f64,
}

/// Avança o acumulador e devolve quantos snapshots venceram (>= 1 = enviar).
/// Pura e testável: a prova temporal do MF-032 chega aqui sem ECS.
pub fn advance_snapshot_clock(accumulator: &mut f64, delta_secs: f64) -> u32 {
    *accumulator += delta_secs;
    let period = 1.0 / SNAPSHOT_HZ;
    let mut due = 0;
    while *accumulator >= period {
        *accumulator -= period;
        due += 1;
    }
    due
}

/// Transporte alternativo para testes: o plugin usa UDP em produção e pode
/// ser trocado antes do build por canais locais do lightyear.
#[derive(Resource, Default)]
pub struct ServerTransportOverride(pub Vec<ServerTransport>);

pub struct ServerNetPlugin;

impl Plugin for ServerNetPlugin {
    fn build(&self, app: &mut App) {
        let overridden = app
            .world()
            .get_resource::<ServerTransportOverride>()
            .filter(|overrides| !overrides.0.is_empty())
            .map(|overrides| overrides.0.clone())
            .unwrap_or_else(|| vec![ServerTransport::UdpSocket(SERVER_ADDR)]);
        let net_configs = overridden
            .into_iter()
            .map(|transport| NetConfig::Netcode {
                io: IoConfig {
                    transport,
                    ..default()
                },
                config: NetcodeConfig::default(),
            })
            .collect::<Vec<_>>();
        app.add_plugins(ServerPlugins::new(ServerConfig {
            shared: shared_config(),
            net: net_configs,
            ..default()
        }));
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_resource::<CombatTuning>();
        app.init_resource::<ShipIdCounter>();
        app.init_resource::<crate::npc::NpcIdCounter>();
        app.init_resource::<crate::npc::NpcSpawnConfig>();
        app.init_resource::<crate::npc::NpcRespawnQueue>();
        app.init_resource::<ProjectileIdCounter>();
        app.init_resource::<WreckIdCounter>();
        app.insert_resource(ServerLootPolicy(LootPolicy::default()));
        app.insert_resource(ServerWreckPolicy(WreckPolicy::default()));
        app.insert_resource(ServerWorldMap(WorldMap::vertical_slice()));
        app.insert_resource(ServerRiskPolicy(RiskPolicy::default()));
        app.insert_resource(ServerGatherPolicy(GatheringPolicy::default()));
        app.insert_resource(ServerDockPolicy::default());
        app.init_resource::<SnapshotClock>();
        app.init_resource::<Metrics>();
        app.init_resource::<crate::nodes::NodeIdCounter>();
        app.init_resource::<LiveWreckRecords>();
        app.init_resource::<CombatImpacts>();
        app.init_resource::<PendingShipDestructions>();
        let economy = crate::market::ServerEconomyConfig::default();
        app.insert_resource(crate::market::ServerPriceIndex(MarketPriceIndex::new(
            economy.price_index_window_size,
        )));
        // Persistência (MF-033/034): o store nasce do ambiente (Postgres de
        // produção, arquivo de dev, ou nenhum) e amarra mercado e navios.
        let store = crate::persist::store_from_env();
        app.insert_resource(store.clone());
        app.insert_resource(crate::market::ServerMarket::with_store(store.0.clone()));
        let dev_items = DevItems::new();
        app.insert_resource(crate::crafting::DevShips::new());
        app.insert_resource(crate::crafting::DevRecipes::new(&dev_items));
        app.insert_resource(dev_items);
        app.add_plugins(crate::loadout::LoadoutPlugin);
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
        app.register_message::<Dock>(ChannelDirection::ClientToServer);
        app.register_message::<Undock>(ChannelDirection::ClientToServer);
        app.register_message::<EquipItem>(ChannelDirection::ClientToServer);
        app.register_message::<UnequipItem>(ChannelDirection::ClientToServer);
        app.register_message::<FireBroadside>(ChannelDirection::ClientToServer);
        app.register_message::<LootWreck>(ChannelDirection::ClientToServer);
        app.register_message::<GatherNode>(ChannelDirection::ClientToServer);
        app.register_message::<CraftItem>(ChannelDirection::ClientToServer);
        app.register_message::<StorageDepositAll>(ChannelDirection::ClientToServer);
        app.register_message::<StorageWithdrawAll>(ChannelDirection::ClientToServer);
        app.register_message::<CreateSellOrder>(ChannelDirection::ClientToServer);
        app.register_message::<CancelSellOrder>(ChannelDirection::ClientToServer);
        app.register_message::<BuySellOrder>(ChannelDirection::ClientToServer);
        app.register_message::<ServerWelcome>(ChannelDirection::ServerToClient);
        app.register_message::<AssignShip>(ChannelDirection::ServerToClient);
        app.register_message::<DockResult>(ChannelDirection::ServerToClient);
        app.register_message::<LoadoutSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<LoadoutResult>(ChannelDirection::ServerToClient);
        app.register_message::<WorldSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<ShipDestroyed>(ChannelDirection::ServerToClient);
        app.register_message::<LootResult>(ChannelDirection::ServerToClient);
        app.register_message::<ZoneChanged>(ChannelDirection::ServerToClient);
        app.register_message::<NodesSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<NodeUpdated>(ChannelDirection::ServerToClient);
        app.register_message::<GatherResult>(ChannelDirection::ServerToClient);
        app.register_message::<RecipesSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<CraftResult>(ChannelDirection::ServerToClient);
        app.register_message::<CatalogSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<WalletUpdated>(ChannelDirection::ServerToClient);
        app.register_message::<OrdersSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<PortStorageSnapshot>(ChannelDirection::ServerToClient);
        app.register_message::<MarketResult>(ChannelDirection::ServerToClient);
        app.add_systems(Startup, start_server);
        app.add_systems(Startup, crate::nodes::spawn_dev_nodes.after(start_server));
        app.add_systems(Startup, crate::npc::setup_npcs.after(start_server));
        app.add_systems(Startup, crate::market::load_state.after(start_server));
        app.add_systems(Update, crate::market::save_state);
        app.add_event::<crate::market::TradeExecuted>();
        app.add_systems(
            FixedUpdate,
            crate::market::update_price_index_from_trades
                .in_set(SimulationSet::EconomyConsequences),
        );
        // MF-027 cont.: wrecks persistem como snapshot derivado do estado
        // em memória. O `persist_wrecks` corre após o tick para evitar
        // pressão no hot loop.
        app.add_systems(Startup, load_wrecks.after(crate::market::load_state));
        app.add_systems(
            FixedUpdate,
            persist_wrecks
                .after(expire_wrecks)
                .in_set(SimulationSet::Persistence),
        );
        // (Bevy 0.15 implementa tuplas de sistemas até 15 elementos — o
        // tick do A1 é grande demais para uma tupla só: duas rodadas.)
        app.add_systems(
            FixedUpdate,
            (
                handle_connections,
                handle_hello,
                handle_input,
                handle_dock,
                handle_undock,
                handle_fire,
                handle_loot,
                crate::nodes::handle_gather,
                crate::crafting::handle_craft,
                crate::market::handle_storage,
            )
                .in_set(SimulationSet::Input),
        );
        // Ordem importa: snapshots veem o estado JÁ simulado deste tick.
        simulate_world(app);
        app.add_systems(
            FixedUpdate,
            crate::npc::simulate_npcs
                .after(respawn_destroyed_ships)
                .in_set(SimulationSet::Destruction),
        );
        app.add_systems(
            FixedUpdate,
            crate::npc::respawn_npcs
                .after(crate::npc::simulate_npcs)
                .in_set(SimulationSet::Destruction),
        );
        app.add_systems(
            FixedUpdate,
            (expire_orders, crate::nodes::respawn_nodes).in_set(SimulationSet::EconomyConsequences),
        );
        app.add_systems(FixedUpdate, world_status.in_set(SimulationSet::Telemetry));
        app.add_systems(FixedUpdate, send_snapshots.in_set(SimulationSet::Snapshot));
        app.add_systems(
            FixedUpdate,
            (expire_ship_grace, expire_wrecks)
                .chain()
                .in_set(SimulationSet::Persistence),
        );
    }
}

fn start_server(mut commands: Commands) {
    commands.start_server();
    info!(addr = %SERVER_ADDR, "mareforge server listening");
}

/// Navio autoritativo: a única cópia do estado que vale (Pilar 4). O dono é
/// um [`CharacterId`] persistente (MF-035); `client_id` é o transporte da
/// sessão atual (`None` = dono offline, navio em janela de graça).
#[derive(Component)]
pub struct ServerShip {
    pub ship_id: u32,
    /// Sessão atual (`None` durante a janela de graça pós-desconexão).
    pub client_id: Option<ClientId>,
    /// Dono persistente — carteira, storage, orders e janelas de wreck.
    pub character: CharacterId,
    /// Instância do casco (localização `ShipCargo` do porão referencia este id).
    pub ship_instance: ShipInstanceId,
    /// Tipo do casco (persistência e reconstrução de stats, MF-034/035).
    pub kind: ShipKind,
    /// Presença (MF-036): AtSea ou Docked(região). Serviços e ações olham
    /// para cá — não para a posição dentro da baía.
    pub presence: VesselPresence,
    /// Equipamento instalado nos slots (MF-039). Custódias vivas: swap
    /// devolve o antigo ao storage, naufrágio o leva ao full loot.
    pub loadout: ShipLoadout,
    pub input: ShipInput,
    pub hp: u32,
    pub hold: CargoHold,
    pub battery: BroadsideBattery,
    pub stats: ShipStats,
    pub motion: ShipMotion,
    pub tuning: MotionTuning,
    /// Zona atual conforme o mapa (MF-017). `None` = fora das águas
    /// declaradas (UnknownZone): combate fail-closed, nenhuma UI nova.
    pub zone: Option<ZoneId>,
    /// Trip iniciada por Undock. `None` antes do primeiro Undock ou depois
    /// de Dock/Sink.
    pub trip: Option<TripTelemetry>,
    /// Restore AtSea abre apenas medição operacional de duração; origem e
    /// preço de marcação não são inventados (alpha não persiste telemetria).
    pub restored_trip_started_at: Option<f32>,
}

/// Dono desconectado; o navio fica no mar por [`DISCONNECT_GRACE_SECS`],
/// vulnerável (MF-035). Reconnect dentro da janela reassume; expirou, o
/// navio é retirado do mundo e persistido (política pós-janela é a âncora
/// no store — a decisão final de mundo persistido é do próximo capítulo).
#[derive(Component)]
pub struct GraceWindow {
    pub since: Instant,
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
    /// Personagem com direito exclusivo de saque na janela inicial (MF-035:
    /// o killer é o personagem, não a conexão).
    pub exclusive_looter: Option<CharacterId>,
    /// Segundos decorridos no momento do spawn, relativos ao
    /// `Bevy::Time::elapsed_secs()` no momento do spawn. Persistido para
    /// permitir restore após restart com o tempo de vida restante correto.
    pub spawned_at_secs: f32,
    pub x: f32,
    pub y: f32,
}

#[derive(Resource, Default)]
pub struct ShipIdCounter(pub u32);

#[derive(Resource, Default)]
pub struct ProjectileIdCounter(pub u32);

#[derive(Resource, Default)]
pub struct WreckIdCounter(pub u32);

/// Buffer em memória do snapshot de wrecks persistidos (MF-027 cont.).
/// `simulate_world` e `expire_wrecks` mantêm este Resource em sincronia
/// com os `ServerWreck` ativos; o sistema `persist_wrecks` drena para o
/// store. Separar a escrita do store do simulate_world mantém o limite
/// de SystemParams do Bevy 0.15 (15 por sistema).
#[derive(Resource, Default)]
pub struct LiveWreckRecords(pub Vec<crate::persist::WreckRecord>);

/// Impactos de projéteis coletados no set `Combat` e consumidos no set
/// `Destruction` (MF-054). Desacopla a colisão (borrow mutável em projéteis)
/// da resolução de dano (borrow mutável em navios) sem estourar o limite de
/// SystemParams do Bevy 0.15.
///
/// Tupla: (entidade do projétil, alvo ship_id, dano, dono do projétil ship_id).
#[derive(Resource, Default)]
pub struct CombatImpacts(pub Vec<(Entity, u32, u32, u32)>);

/// Naufrágio decidido em `apply_combat_damage` (MF-054): só marca e despawna
/// o navio. `resolve_destructions` materializa wreck + mensagem;
/// `respawn_destroyed_ships` recria o casco do dono.
#[derive(Resource, Default)]
pub struct PendingShipDestructions(pub Vec<PendingShipDestruction>);

pub struct PendingShipDestruction {
    pub target_ship_id: u32,
    pub victim_client_id: Option<ClientId>,
    pub victim_character: CharacterId,
    pub victim_x: f32,
    pub victim_y: f32,
    pub equipment: Vec<ItemDefinitionId>,
    pub cargo: Vec<ItemInstance>,
    pub audience: Vec<ClientId>,
    pub exclusive_looter: Option<CharacterId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TradeRouteKey {
    pub origin: RegionId,
    pub destination: RegionId,
}

/// MF-052: estado da trip iniciada em Undock. O valor da carga é marcado
/// uma vez, no preço regional do porto de origem; itens sem VWAP ficam
/// unpriced em vez de valer zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TripTelemetry {
    pub started_at: f32,
    pub origin: RegionId,
    pub marked_cargo_value: u64,
    pub priced_quantity: u32,
    pub unpriced_quantity: u32,
}

/// Telemetria econômica + gameplay (MF-029, §71+§72): faucets, sinks, perdas
/// e os contadores derivados de combate contados no ponto onde acontecem.
/// Nada é derivado retroativamente: cada incremento é um `+= 1` no evento
/// correspondente.
#[derive(Resource, Debug, Default, Clone)]
pub struct Metrics {
    // §71 — econômica.
    pub items_gathered: u64,
    pub items_crafted: u64,
    pub items_destroyed: u64,
    pub ships_constructed: u64,
    pub ships_destroyed: u64,
    pub loot_transfers: u64,
    pub npc_bounty_gold_minted: u64,
    // §72 — gameplay. Apenas navios de player contam em `ship_losses_by_kind`;
    // o `ships_destroyed` acima inclui NPC. `pvp_engagements` é o número de
    // impactos projetil-vs-player-em-player (não hits — engagements). Índices
    // de `ship_losses_by_kind` casam com `ShipKind as usize`:
    // SmallMerchant=0, Patrol=1, Corsair=2.
    pub ship_losses_by_kind: [u64; 3],
    pub wrecks_looted: u64,
    pub pvp_engagements: u64,
    /// §72 route_usage: total de cruzamentos de fronteira de zona por
    /// player ships. Counter agregado; pares (from, to) ficariam em
    /// outra estrutura se gepeto quiser granularidade.
    pub zone_transitions: u64,
    /// §72 average_trip_duration: soma de durações de trips encerradas
    /// (dock ou sink). Dividido por `trip_count` no log para a média.
    pub trip_total_secs: f64,
    /// §72 average_trip_duration: denominador da média de duração.
    pub trip_count: u64,
    /// MF-050: rotas completas por par direcional de portos.
    pub completed_routes: HashMap<TradeRouteKey, u64>,
    pub same_port_returns: u64,
    pub trips_sunk: u64,
    /// MF-053: valor marcado no Undock de cada trip que saiu de um porto.
    pub cargo_value_departed: u64,
    /// MF-053: valor marcado das trips finalizadas em Dock bem-sucedido.
    pub cargo_value_arrived: u64,
    /// MF-053: valor marcado das trips finalizadas em ShipDestroyed. Carga
    /// afundada não é carga destruída: parte pode sobreviver no wreck.
    pub cargo_value_sunk: u64,
    /// MF-052: soma dos valores marcados das trips 100% priced e com carga.
    pub cargo_value_at_risk_total: u64,
    /// MF-052: denominador de `average_cargo_value_at_risk`.
    pub cargo_value_trip_count: u64,
    /// MF-052: unidades com preço regional conhecido no Undock.
    pub cargo_value_priced_items: u64,
    /// MF-052: unidades sem preço regional conhecido no Undock.
    pub cargo_value_unpriced_items: u64,
    /// MF-052: cobertura agregada; 0/0 é reportada como 100%.
    pub cargo_value_coverage_pct: f32,
}

pub enum TripOutcome {
    Docked(RegionId),
    Sunk,
}

/// MF-049: encerra a trip ativa de um player ship em Dock bem-sucedido ou
/// Sunk — soma a duração ao `trip_total_secs`, incrementa `trip_count` e
/// zera a trip ativa. No-op se não havia trip ativa. Retorna `true`
/// quando uma viagem foi contabilizada.
pub fn finalize_trip(
    ship: &mut ServerShip,
    metrics: &mut Metrics,
    now: f32,
    outcome: TripOutcome,
) -> bool {
    if let Some(trip) = ship.trip.take() {
        finalize_marked_trip(trip, metrics, now, outcome);
        true
    } else if let Some(started) = ship.restored_trip_started_at.take() {
        metrics.trip_total_secs += (now - started) as f64;
        metrics.trip_count += 1;
        true
    } else {
        false
    }
}

fn finalize_marked_trip(
    trip: TripTelemetry,
    metrics: &mut Metrics,
    now: f32,
    outcome: TripOutcome,
) {
    metrics.trip_total_secs += (now - trip.started_at) as f64;
    metrics.trip_count += 1;
    match outcome {
        TripOutcome::Docked(destination) => {
            metrics.cargo_value_arrived = metrics
                .cargo_value_arrived
                .saturating_add(trip.marked_cargo_value);
            if trip.origin != destination {
                *metrics
                    .completed_routes
                    .entry(TradeRouteKey {
                        origin: trip.origin,
                        destination,
                    })
                    .or_default() += 1;
            } else {
                metrics.same_port_returns += 1;
            }
        }
        TripOutcome::Sunk => {
            metrics.trips_sunk += 1;
            metrics.cargo_value_sunk = metrics
                .cargo_value_sunk
                .saturating_add(trip.marked_cargo_value);
        }
    }

    metrics.cargo_value_priced_items += u64::from(trip.priced_quantity);
    metrics.cargo_value_unpriced_items += u64::from(trip.unpriced_quantity);
    let total_quantity = metrics.cargo_value_priced_items + metrics.cargo_value_unpriced_items;
    metrics.cargo_value_coverage_pct = if total_quantity == 0 {
        100.0
    } else {
        metrics.cargo_value_priced_items as f32 / total_quantity as f32 * 100.0
    };
    if trip.priced_quantity > 0 && trip.unpriced_quantity == 0 {
        metrics.cargo_value_at_risk_total += trip.marked_cargo_value;
        metrics.cargo_value_trip_count += 1;
    }
}

/// MF-049: abre uma trip nova em Undock bem-sucedido. Idempotente: se já
/// existe trip ativa, mantém (proteção contra
/// Undock espúrio durante viagem — não pode acontecer pelo domínio, mas
/// falhamos em silêncio em vez de resetar medição).
pub fn start_trip(
    ship: &mut ServerShip,
    metrics: &mut Metrics,
    price_index: &crate::market::ServerPriceIndex,
    now: f32,
    origin_port: RegionId,
) {
    if ship.trip.is_none() {
        let mut marked_cargo_value = 0u64;
        let mut priced_quantity = 0u32;
        let mut unpriced_quantity = 0u32;
        for custody in ship.hold.items() {
            let quantity = custody.instance.quantity;
            if let Some(vwap) = price_index.vwap(origin_port, custody.instance.definition) {
                priced_quantity = priced_quantity.saturating_add(quantity);
                marked_cargo_value = marked_cargo_value
                    .saturating_add(vwap.unit_price.0.saturating_mul(u64::from(quantity)));
            } else {
                unpriced_quantity = unpriced_quantity.saturating_add(quantity);
            }
        }
        metrics.cargo_value_departed = metrics
            .cargo_value_departed
            .saturating_add(marked_cargo_value);
        ship.trip = Some(TripTelemetry {
            started_at: now,
            origin: origin_port,
            marked_cargo_value,
            priced_quantity,
            unpriced_quantity,
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_ship_for(
    commands: &mut Commands,
    ship_ids: &mut ShipIdCounter,
    dev: &DevItems,
    dev_ships: &crate::crafting::DevShips,
    map: &WorldMap,
    kind: ShipKind,
    client_id: Option<ClientId>,
    character: CharacterId,
    carry: Vec<Custody>,
) -> u32 {
    let ship_id = ship_ids.0;
    ship_ids.0 += 1;
    let definition = dev_ships.definition(kind).clone();
    let stats = compute_ship_stats(
        &definition,
        &EquippedComponents::default(),
        &ItemCatalog::default(),
    )
    .expect("stats de navio sem equipamento não podem falhar");
    let ship_instance = ShipInstanceId::new();
    let mut hold = CargoHold::new(ship_instance, stats.cargo_capacity);
    // Carga dev (PRD §39): mercadoria de teste para o loop econômico —
    // afundou, a carga vai pro wreck e muda de dono. Só o merchant de
    // dev respawn nasce semeado; navios construídos migram a carga real.
    if kind == ShipKind::SmallMerchant {
        hold.insert(
            &dev.catalog,
            ItemInstance::new_resource(ItemInstanceId::new(), dev.timber, 15),
        )
        .expect("carga dev cabe no porão vazio");
    }
    if !carry.is_empty() {
        // Construção (MF-022): a carga do casco antigo migra para o novo.
        // `take_all` é atômico; a capacidade foi conferida pelo chamador.
        hold.take_all(&dev.catalog, carry)
            .expect("carga migra: capacidade conferida antes da construção");
    }

    // Nasce na doca do Porto da Serra, em águas protegidas (Pilar 3: o
    // risco é escolha do jogador, não condição de nascimento).
    let zone = map.zone_at(DEV_SPAWN.0, DEV_SPAWN.1).ok().map(|z| z.id);

    commands.spawn((ServerShip {
        ship_id,
        client_id,
        character,
        ship_instance,
        kind,
        presence: VesselPresence::AtSea,
        loadout: ShipLoadout::new(),
        input: ShipInput {
            throttle: 0.0,
            turn: 0.0,
        },
        hp: stats.max_hp,
        hold,
        battery: BroadsideBattery::default(),
        stats,
        motion: ShipMotion {
            x: DEV_SPAWN.0,
            y: DEV_SPAWN.1,
            ..ShipMotion::default()
        },
        tuning: MotionTuning::default(),
        zone,
        // MF-049: navios novos não iniciam trip — só Undock bem-sucedido
        // abre medição. O spawn aqui é o equivalente a "já está fora do
        // porto"; a primeira trip do personagem começa quando ele Decide
        // sair novamente.
        trip: None,
        restored_trip_started_at: None,
    },));
    ship_id
}

/// Recria um navio a partir do registro persistido (MF-035: reconnect pós-
/// janela de graça = mesmos casco, HP, posição e carga embarcada). A zona
/// vem do mapa na posição salva; fora do mar declarado, `None` (fail-closed
/// no combate até tocar águas conhecidas de novo).
#[allow(clippy::too_many_arguments)]
pub(crate) fn restore_ship_from_record(
    commands: &mut Commands,
    ship_ids: &mut ShipIdCounter,
    dev: &DevItems,
    dev_ships: &crate::crafting::DevShips,
    map: &WorldMap,
    record: crate::persist::ShipRecord,
    now: f32,
) -> u32 {
    let ship_id = ship_ids.0;
    ship_ids.0 += 1;
    let definition = dev_ships.definition(record.kind).clone();
    let stats = compute_ship_stats(
        &definition,
        &EquippedComponents::default(),
        &ItemCatalog::default(),
    )
    .expect("stats de navio sem equipamento não podem falhar");
    let mut hold = CargoHold::new(record.ship_instance, stats.cargo_capacity);
    if !record.cargo.is_empty() {
        hold.take_all(&dev.catalog, record.cargo)
            .expect("carga restaurada cabe: era do mesmo casco");
    }
    // Loadout restaurado: slots voltam a ocupados, stats RECALCULADOS com
    // o equipamento instalado (fail-closed pelo catálogo).
    let mut loadout = ShipLoadout::new();
    for custody in record.equipped {
        let slot = match custody.location {
            mareforge_domain_items::ItemLocation::Equipped { slot, .. } => slot,
            _ => continue,
        };
        loadout.equip(record.ship_instance, custody, slot);
    }
    let equipped_stats = compute_ship_stats(
        dev_ships.definition(record.kind),
        &loadout.components(),
        &dev.catalog,
    )
    .expect("loadout restaurado contém definições do catálogo");
    hold.set_capacity(equipped_stats.cargo_capacity);
    let zone = map.zone_at(record.x, record.y).ok().map(|z| z.id);
    // MF-049: presença persistida é a verdade — restore mantém Docked se
    // estava atracado, AtSea se estava fora. Trip só começa por evento
    // explícito, então AtSea restaurado abre nova medição em `now` (não
    // reconstruímos duração anterior — não há dado persistido para isso).
    let restored_trip_started_at = match record.presence {
        VesselPresence::AtSea => Some(now),
        VesselPresence::Docked(_) => None,
    };
    commands.spawn((ServerShip {
        ship_id,
        client_id: None,
        character: record.character,
        ship_instance: record.ship_instance,
        kind: record.kind,
        presence: record.presence,
        loadout,
        input: ShipInput {
            throttle: 0.0,
            turn: 0.0,
        },
        hp: record.hp.min(equipped_stats.max_hp),
        hold,
        battery: BroadsideBattery::default(),
        stats: equipped_stats,
        motion: ShipMotion {
            x: record.x,
            y: record.y,
            heading: record.heading,
            ..ShipMotion::default()
        },
        tuning: MotionTuning::default(),
        zone,
        trip: None,
        restored_trip_started_at,
    },));
    ship_id
}

/// Mensagem inicial de zona para um navio que acabou de nascer (PRD §10: o
/// client precisa do estado atual, não só das transições).
pub(crate) fn zone_changed_for(
    map: &WorldMap,
    ship_id: u32,
    x: f32,
    y: f32,
) -> Option<ZoneChanged> {
    map.zone_at(x, y).ok().map(|zone| ZoneChanged {
        ship_id,
        tier: zone.tier,
        zone_name: zone.name.to_string(),
    })
}

fn handle_connections(
    mut commands: Commands,
    mut connect: EventReader<ConnectEvent>,
    mut disconnect: EventReader<DisconnectEvent>,
    mut ships: Query<(Entity, &mut ServerShip)>,
) {
    for connection in connect.read() {
        info!(client = ?connection.client_id, "client conectado; aguardando ClientHello");
    }
    for event in disconnect.read() {
        info!(client = ?event.client_id, "client desconectado");
        for (entity, mut ship) in &mut ships {
            if ship.client_id == Some(event.client_id) {
                // MF-035: a conexão caiu, o personagem não. O navio fica no
                // mar por DISCONNECT_GRACE_SECS, PARADO e VULNERÁVEL —
                // logout não é fuga de PvP (o casco é um alvo parado na
                // janela). O dono pode reassumir com o mesmo token.
                ship.client_id = None;
                ship.input = ShipInput {
                    throttle: 0.0,
                    turn: 0.0,
                };
                commands.entity(entity).insert(GraceWindow {
                    since: Instant::now(),
                });
                warn!(ship_id = ship.ship_id, "navio em janela de graça");
            }
        }
    }
}

/// Handshake (ADR-0011) + identidade persistente (MF-035). O token do
/// `ClientHello` resolve o CharacterId; a sessão pode ser nova, mas a
/// carteira, o storage, as orders e o navio (na janela de graça ou
/// persistido no store) são do personagem.
// System Bevy: params são injeção de dependência, não assinatura.
#[allow(clippy::too_many_arguments)]
fn handle_hello(
    mut commands: Commands,
    mut hello_events: EventReader<ServerReceiveMessage<ClientHello>>,
    mut connection_manager: ResMut<ConnectionManager>,
    dev: Res<DevItems>,
    dev_ships: Res<crate::crafting::DevShips>,
    dev_recipes: Res<crate::crafting::DevRecipes>,
    mut market: ResMut<crate::market::ServerMarket>,
    store: Res<crate::persist::StoreHandle>,
    map: Res<ServerWorldMap>,
    mut ships: Query<(Entity, &mut ServerShip)>,
    nodes: Query<&crate::nodes::ServerNode>,
    mut ship_ids: ResMut<ShipIdCounter>,
    time: Res<Time>,
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
        // Fail-closed (§69): hello sem token não vira identidade anônima.
        let token = hello.identity.trim();
        if token.is_empty() {
            warn!(client = ?client_id, "hello sem identidade; recusado");
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &ServerWelcome {
                    protocol_version: PROTOCOL_VERSION,
                    accepted: false,
                },
            );
            continue;
        }
        let character = market.character(token);

        // Reassumir um navio vivo nesta sessão? (hello duplicado)
        if let Some((_, ship)) = ships
            .iter()
            .find(|(_, ship)| ship.character == character && ship.client_id == Some(client_id))
        {
            info!(ship_id = ship.ship_id, "hello duplicado ignorado");
            continue;
        }
        // Reassumir o navio em janela de graça (reconnect, MF-035).
        if let Some((entity, mut ship)) = ships
            .iter_mut()
            .find(|(_, ship)| ship.character == character && ship.client_id.is_none())
        {
            ship.client_id = Some(client_id);
            commands.entity(entity).remove::<GraceWindow>();
            let ship_id = ship.ship_id;
            let reclaimed_kind = ship.kind;
            let position = (ship.motion.x, ship.motion.y);
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &ServerWelcome {
                    protocol_version: PROTOCOL_VERSION,
                    accepted: true,
                },
            );
            let _ = connection_manager.send_message::<ReliableChannel, _>(
                client_id,
                &AssignShip {
                    ship_id,
                    kind: reclaimed_kind,
                },
            );
            if let Some(zone) = zone_changed_for(&map.0, ship_id, position.0, position.1) {
                let _ = connection_manager.send_message::<ReliableChannel, _>(client_id, &zone);
            }
            // Captura ANTES de reemprestar `ships` para o broadcast.
            let equipped: Vec<_> = ship.loadout.items().cloned().collect();
            send_initial_world(
                &mut connection_manager,
                &dev,
                &dev_recipes,
                &nodes,
                &market,
                &map,
                &ships,
                client_id,
                character,
            );
            crate::loadout::send_loadout_snapshot(
                &mut connection_manager,
                client_id,
                dev_ships.definition(reclaimed_kind),
                &dev.catalog,
                &equipped,
            );
            info!(ship_id, "sessão reassumida dentro da janela de graça");
            continue;
        }
        // Personagem já conectado por OUTRA sessão: recusa explícita.
        if ships.iter().any(|(_, ship)| ship.character == character) {
            warn!(client = ?client_id, "personagem já em mar; hello ignorado");
            continue;
        }

        // Navio novo: restaura o persistido (pós-janela, MF-034/035) ou
        // nasce na doca da Serra.
        let restored = store
            .0
            .as_ref()
            .and_then(|store| store.load_ship(character).ok().flatten());
        let (ship_id, position, restored_equipped, ship_kind) = match restored {
            Some(record) => {
                let position = (record.x, record.y);
                let equipped = record.equipped.clone();
                let ship_kind = record.kind;
                let ship_id = restore_ship_from_record(
                    &mut commands,
                    &mut ship_ids,
                    &dev,
                    &dev_ships,
                    &map.0,
                    record,
                    // MF-049: passamos o instante atual para que o restore
                    // normalize a medição operacional quando o navio volta
                    // AtSea.
                    time.elapsed_secs(),
                );
                info!(
                    ship_id,
                    "navio restaurado do store (personagem volta ao mar)"
                );
                (ship_id, position, equipped, ship_kind)
            }
            None => {
                let ship_id = spawn_ship_for(
                    &mut commands,
                    &mut ship_ids,
                    &dev,
                    &dev_ships,
                    &map.0,
                    ShipKind::SmallMerchant,
                    Some(client_id),
                    character,
                    Vec::new(),
                );
                (ship_id, DEV_SPAWN, Vec::new(), ShipKind::SmallMerchant)
            }
        };
        let _ = connection_manager.send_message::<ReliableChannel, _>(
            client_id,
            &ServerWelcome {
                protocol_version: PROTOCOL_VERSION,
                accepted: true,
            },
        );
        let _ = connection_manager.send_message::<ReliableChannel, _>(
            client_id,
            &AssignShip {
                ship_id,
                kind: ship_kind,
            },
        );
        if let Some(zone) = zone_changed_for(&map.0, ship_id, position.0, position.1) {
            let _ = connection_manager.send_message::<ReliableChannel, _>(client_id, &zone);
        }
        send_initial_world(
            &mut connection_manager,
            &dev,
            &dev_recipes,
            &nodes,
            &market,
            &map,
            &ships,
            client_id,
            character,
        );
        crate::loadout::send_loadout_snapshot(
            &mut connection_manager,
            client_id,
            dev_ships.definition(ship_kind),
            &dev.catalog,
            &restored_equipped,
        );
        info!(client = ?client_id, ship_id, "navio autoritativo criado");
    }
}

/// Pacote de estado inicial pós-hello: nodes, receitas, catálogo, carteira
/// e quadro de orders. Depois disto, o client só recebe deltas.
#[allow(clippy::too_many_arguments)]
fn send_initial_world(
    connection_manager: &mut ConnectionManager,
    dev: &DevItems,
    dev_recipes: &crate::crafting::DevRecipes,
    nodes: &Query<&crate::nodes::ServerNode>,
    market: &crate::market::ServerMarket,
    map: &ServerWorldMap,
    ships: &Query<(Entity, &mut ServerShip)>,
    client_id: ClientId,
    character: CharacterId,
) {
    let _ = connection_manager.send_message::<ReliableChannel, _>(
        client_id,
        &crate::nodes::nodes_snapshot(nodes, &dev.catalog),
    );
    let _ = connection_manager
        .send_message::<ReliableChannel, _>(client_id, &dev_recipes.snapshot(&dev.catalog));
    let _ = connection_manager.send_message::<ReliableChannel, _>(
        client_id,
        &crate::market::catalog_snapshot(&dev.catalog),
    );
    let _ = connection_manager.send_message::<ReliableChannel, _>(
        client_id,
        &WalletUpdated {
            gold: market.balance(character).0,
        },
    );
    let viewers: Vec<(Option<ClientId>, CharacterId)> = ships
        .iter()
        .map(|(_, ship)| (ship.client_id, ship.character))
        .collect();
    crate::market::broadcast_orders(connection_manager, market, &dev.catalog, &map.0, &viewers);
}

/// Último input vence; validação/clamp acontece dentro do modelo puro.
fn handle_input(
    mut input_events: EventReader<ServerReceiveMessage<ShipInput>>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in input_events.read() {
        let client_id = event.from();
        for mut ship in &mut ships {
            if ship.client_id == Some(client_id) {
                let incoming = *event.message();
                if ship.input != incoming {
                    info!(
                        ship_id = ship.ship_id,
                        throttle = incoming.throttle,
                        turn = incoming.turn,
                        "input aplicado"
                    );
                }
                ship.input = incoming;
                break;
            }
        }
    }
}

/// Disparo de bordo (PRD MF-009): cooldown decide; o projétil nasce
/// server-authoritative a partir do estado real do navio. A zona do atirador
/// é a primeira porta (MF-017): de águas protegidas os canhões ficam frios —
/// e de fora do mapa, fail-closed (§69).
fn handle_fire(
    mut commands: Commands,
    mut fire_events: EventReader<ServerReceiveMessage<FireBroadside>>,
    tuning: Res<CombatTuning>,
    map: Res<ServerWorldMap>,
    risk: Res<ServerRiskPolicy>,
    mut projectile_ids: ResMut<ProjectileIdCounter>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in fire_events.read() {
        let client_id = event.from();
        let side = event.message().side;
        let Some(mut ship) = ships
            .iter_mut()
            .find(|ship| ship.client_id == Some(client_id))
        else {
            continue;
        };
        if matches!(ship.presence, VesselPresence::Docked(_)) {
            info!(
                ship_id = ship.ship_id,
                "disparo recusado: canhões presos enquanto atracado (MF-036)"
            );
            continue;
        }
        match map.0.zone_at(ship.motion.x, ship.motion.y) {
            Ok(zone) if risk.0.pvp_allowed(zone.tier) => {}
            Ok(zone) => {
                info!(
                    ship_id = ship.ship_id,
                    zone = zone.name,
                    "disparo recusado: canhões frios em águas protegidas"
                );
                continue;
            }
            Err(_) => {
                warn!(
                    ship_id = ship.ship_id,
                    "disparo recusado: fora do mar declarado"
                );
                continue;
            }
        }
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
    time: Res<Time>,
    wreck_policy: Res<ServerWreckPolicy>,
    tuning: Res<CombatTuning>,
    mut metrics: ResMut<Metrics>,
    mut ships: Query<&mut ServerShip>,
    mut wrecks: Query<(Entity, &mut ServerWreck)>,
) {
    for event in loot_events.read() {
        let client_id = event.from();
        let wreck_num = event.message().wreck_id;

        let Some(mut ship) = ships
            .iter_mut()
            .find(|ship| ship.client_id == Some(client_id))
        else {
            continue;
        };
        let Some((wreck_entity, mut wreck)) = wrecks
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

        let elapsed = time.elapsed_secs() - wreck.spawned_at_secs;
        if !can_loot(
            elapsed,
            &wreck_policy.0,
            ship.character,
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

        // §70/MF-028: só um vencedor. O despawn do Bevy é DEFERIDO — dois
        // LootWreck no mesmo tick encontrariam a mesma entidade. O baú esvazia
        // NA HORA; o segundo evento bate no guard de vazio abaixo.
        if wreck.chest.is_empty() {
            warn!(
                wreck_num,
                "wreck já saqueado neste tick (double loot barrado)"
            );
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
                // §72 wrecks_looted: transferência atômica concluída.
                metrics.wrecks_looted += 1;
                // Drenagem imediata do baú (não-deferida): a prova de que o
                // primeiro saque venceu fica visível para o resto do tick.
                wreck.chest.drain();
                commands.entity(wreck_entity).despawn();
                // WreckRemoved não existe mais (MF-031): o client percebe a
                // remoção pela ausência no próximo snapshot (TTL visual).
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

/// Registra a cadeia que decompõe o antigo `simulate_world` (MF-054,
/// ADR-0008). Os sets rodam em ordem via `.chain()`: os handlers de input já
/// consumiram as mensagens do client, então aqui entram movimento → zonas →
/// combate → destruição → economia → telemetria → snapshot → persistência.
fn simulate_world(app: &mut App) {
    app.configure_sets(
        FixedUpdate,
        (
            SimulationSet::Input,
            SimulationSet::Movement,
            SimulationSet::Zones,
            SimulationSet::Combat,
            SimulationSet::Destruction,
            SimulationSet::EconomyConsequences,
            SimulationSet::Telemetry,
            SimulationSet::Snapshot,
            SimulationSet::Persistence,
        )
            .chain(),
    );

    app.add_systems(
        FixedUpdate,
        (
            simulate_movement.in_set(SimulationSet::Movement),
            simulate_zones.in_set(SimulationSet::Zones),
            simulate_combat.in_set(SimulationSet::Combat),
            (
                apply_combat_damage,
                resolve_destructions,
                respawn_destroyed_ships,
            )
                .chain()
                .in_set(SimulationSet::Destruction),
        ),
    );
}

/// Avança física dos navios (MF-017): casco atracado fica imóvel com recarga
/// de canhão; os demais aplicam o input ao `step_motion`.
fn simulate_movement(time: Res<Time>, mut ships: Query<&mut ServerShip>) {
    let dt = time.delta_secs();

    for mut ship in &mut ships {
        let ServerShip {
            presence,
            input,
            stats,
            motion,
            tuning,
            battery,
            ..
        } = ship.as_mut();
        if matches!(presence, VesselPresence::Docked(_)) {
            // MF-036: atracado = casco imóvel. O input é ignorado (o dono
            // precisa desatracar para navegar); recarga de canhão corre.
            motion.speed = 0.0;
            battery.advance(dt);
            continue;
        }
        step_motion(
            motion,
            stats,
            MotionInput {
                throttle: input.throttle,
                turn: input.turn,
            },
            tuning,
            dt,
        );
        battery.advance(dt);
    }
}

/// O servidor é quem calcula a zona real (PRD §10). Mudou a zona, o dono é
/// avisado por canal confiável; saiu do mar declarado, o estado legal fica
/// indefinido (fail-closed no combate).
fn simulate_zones(
    mut connection_manager: ResMut<ConnectionManager>,
    mut metrics: ResMut<Metrics>,
    map: Res<ServerWorldMap>,
    mut ships: Query<&mut ServerShip>,
) {
    for mut ship in &mut ships {
        // §72 zone_transitions: zona capturada antes da comparação para não
        // contar a entrada inicial a partir de zona desconhecida.
        let zone_before = ship.zone;
        match map.0.zone_at(ship.motion.x, ship.motion.y) {
            Ok(found) => {
                if ship.zone != Some(found.id) {
                    if ship.client_id.is_some() && zone_before.is_some() {
                        metrics.zone_transitions += 1;
                    }
                    ship.zone = Some(found.id);
                    info!(
                        ship_id = ship.ship_id,
                        zone = found.name,
                        tier = ?found.tier,
                        "navio cruzou uma fronteira"
                    );
                    if let Some(client_id) = ship.client_id {
                        let _ = connection_manager.send_message::<ReliableChannel, _>(
                            client_id,
                            &ZoneChanged {
                                ship_id: ship.ship_id,
                                tier: found.tier,
                                zone_name: found.name.to_string(),
                            },
                        );
                    }
                }
            }
            Err(_) => {
                if ship.zone.is_some() {
                    warn!(ship_id = ship.ship_id, "navio saiu do mar declarado");
                    ship.zone = None;
                }
            }
        }
    }
}

/// Projéteis avançam, expiram e colidem (decisão imutável primeiro). Os
/// impactos são bufferizados em `CombatImpacts` para o set `Destruction`.
fn simulate_combat(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<CombatTuning>,
    ships: Query<&ServerShip>,
    mut projectiles: Query<(Entity, &mut ServerProjectile)>,
    mut impacts: ResMut<CombatImpacts>,
) {
    let dt = time.delta_secs();

    let ship_positions: HashMap<u32, (f32, f32)> = ships
        .iter()
        .map(|ship| (ship.ship_id, (ship.motion.x, ship.motion.y)))
        .collect();

    impacts.0.clear();
    for (projectile_entity, mut projectile) in &mut projectiles {
        projectile.0.advance(dt);
        if projectile.0.expired() {
            commands.entity(projectile_entity).despawn();
            continue;
        }

        for (ship_id, (x, y)) in &ship_positions {
            if *ship_id == projectile.0.owner_ship_id {
                continue;
            }
            if projectile.0.hit_ship(*x, *y, tuning.hit_radius) {
                impacts.0.push((
                    projectile_entity,
                    *ship_id,
                    projectile.0.damage,
                    projectile.0.owner_ship_id,
                ));
                break; // um projétil atinge um navio só
            }
        }
    }
}

/// Aplica dano e decide naufrágios (MF-013). Marca as destruições em
/// `PendingShipDestructions` e despawna projétil + casco; a materialização
/// de wreck/mensagem/respawn fica nos próximos sistemas.
#[allow(clippy::too_many_arguments)]
fn apply_combat_damage(
    mut commands: Commands,
    mut metrics: ResMut<Metrics>,
    map: Res<ServerWorldMap>,
    risk_policy: Res<ServerRiskPolicy>,
    time: Res<Time>,
    mut impacts: ResMut<CombatImpacts>,
    mut pending: ResMut<PendingShipDestructions>,
    mut ships: Query<(Entity, &mut ServerShip)>,
) {
    pending.0.clear();
    let impacts = std::mem::take(&mut impacts.0);

    for (projectile_entity, target_ship_id, damage, killer_ship_id) in impacts {
        commands.entity(projectile_entity).despawn();

        // §72 pvp_engagements: projétil entre players. Calculado ANTES do
        // borrow mutável do alvo — só lê `client_id`, não precisa do resto.
        // Owner e target ambos com `client_id.is_some()` excluem auto-dano
        // e envolvimento com NPC.
        let (target_is_player, killer_is_player) = {
            let mut t = false;
            let mut k = false;
            for (_, candidate) in &ships {
                if candidate.ship_id == target_ship_id && candidate.client_id.is_some() {
                    t = true;
                }
                if candidate.ship_id == killer_ship_id && candidate.client_id.is_some() {
                    k = true;
                }
            }
            (t, k)
        };
        if target_is_player && killer_is_player {
            metrics.pvp_engagements += 1;
        }

        // Escopo: o borrow mutável do navio termina antes de reler a frota.
        let sinking = {
            let Some((entity, mut ship)) = ships
                .iter_mut()
                .find(|(_, ship)| ship.ship_id == target_ship_id)
            else {
                continue;
            };
            // A zona da VÍTIMA decide (MF-017, §9): proteção é da vítima.
            let pvp_here = map
                .0
                .zone_at(ship.motion.x, ship.motion.y)
                .map(|zone| risk_policy.0.pvp_allowed(zone.tier))
                .unwrap_or(false);
            if !pvp_here {
                info!(
                    ship_id = target_ship_id,
                    "impacto ignorado: vítima em águas protegidas ou fora do mapa"
                );
                continue;
            }
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
                    metrics.ships_destroyed += 1;
                    if ship.client_id.is_some() {
                        // §72 ship_losses_by_kind: conta só navios de player.
                        // NPC killings já estão em `ships_destroyed`.
                        metrics.ship_losses_by_kind[ship.kind as usize] += 1;
                        // MF-049: trip termina em Sunk de player ship.
                        // Próxima trip começa no próximo Undock.
                        finalize_trip(
                            &mut ship,
                            &mut metrics,
                            time.elapsed_secs(),
                            TripOutcome::Sunk,
                        );
                    }
                    info!(ship_id = target_ship_id, damage, "SHIP DESTROYED");
                    // Full loot (§22-§25): casco é perda total; parte da carga
                    // e do EQUIPAMENTO INSTALADO (MF-039) sobrevive e vira
                    // wreck. Equipar nunca criou proteção.
                    let victim_client_id = ship.client_id;
                    let victim_character = ship.character;
                    let victim_x = ship.motion.x;
                    let victim_y = ship.motion.y;
                    let equipment: Vec<ItemDefinitionId> = ship
                        .loadout
                        .items()
                        .map(|custody| custody.instance.definition)
                        .collect();
                    let cargo: Vec<ItemInstance> = ship
                        .hold
                        .items()
                        .iter()
                        .map(|custody| custody.instance.clone())
                        .collect();
                    Some((
                        entity,
                        victim_client_id,
                        victim_character,
                        victim_x,
                        victim_y,
                        equipment,
                        cargo,
                    ))
                }
            }
        };

        let Some((
            entity,
            victim_client_id,
            victim_character,
            victim_x,
            victim_y,
            victim_equipment,
            cargo,
        )) = sinking
        else {
            continue;
        };

        // ShipDestroyed é recortado por AOI (MF-031): só quem enxerga a
        // vítima fica sabendo do naufrágio (o próprio dono sempre vê).
        let audience: Vec<ClientId> = ships
            .iter()
            .filter(|(_, witness)| {
                crate::aoi::is_visible((victim_x, victim_y), (witness.motion.x, witness.motion.y))
            })
            .filter_map(|(_, witness)| witness.client_id)
            .collect();

        let exclusive_looter = ships
            .iter()
            .find(|(_, candidate)| candidate.ship_id == killer_ship_id)
            .map(|(_, candidate)| candidate.character);

        pending.0.push(PendingShipDestruction {
            target_ship_id,
            victim_client_id,
            victim_character,
            victim_x,
            victim_y,
            equipment: victim_equipment,
            cargo,
            audience,
            exclusive_looter,
        });

        commands.entity(entity).despawn();
    }
}

/// Materializa o naufrágio: mensagem `ShipDestroyed` (AOI), resolução de
/// full loot e spawn do wreck no mar.
#[allow(clippy::too_many_arguments)]
fn resolve_destructions(
    mut commands: Commands,
    mut connection_manager: ResMut<ConnectionManager>,
    mut wreck_ids: ResMut<WreckIdCounter>,
    loot_policy: Res<ServerLootPolicy>,
    mut live_wrecks: ResMut<LiveWreckRecords>,
    mut metrics: ResMut<Metrics>,
    time: Res<Time>,
    pending: Res<PendingShipDestructions>,
) {
    for destruction in &pending.0 {
        let _ = connection_manager.send_message_to_target::<ReliableChannel, _>(
            &ShipDestroyed {
                ship_id: destruction.target_ship_id,
            },
            NetworkTarget::Only(destruction.audience.clone()),
        );

        let event = DestructionEventId::new();
        let outcome = resolve_ship_destruction(
            event,
            &destruction.equipment,
            &destruction.cargo,
            &loot_policy.0,
        );
        metrics.items_destroyed += outcome.destroyed_items.len() as u64;
        info!(
            ship_id = destruction.target_ship_id,
            event = ?event,
            afundados = outcome.destroyed_items.len(),
            sobreviventes = outcome.wreck_items.len(),
            equipamento = destruction.equipment.len(),
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
            let spawned_at_secs = time.elapsed_secs();
            commands.spawn((ServerWreck {
                wreck_num,
                wreck_id,
                chest,
                exclusive_looter: destruction.exclusive_looter,
                spawned_at_secs,
                x: destruction.victim_x,
                y: destruction.victim_y,
            },));
            // Buffer atualizado em memória; a persistência acontece no
            // sistema `persist_wrecks` (corre fora do hot loop para
            // respeitar o limite de SystemParams).
            live_wrecks.0.push(crate::persist::WreckRecord {
                wreck_num,
                wreck_id,
                x: destruction.victim_x,
                y: destruction.victim_y,
                exclusive_looter: destruction.exclusive_looter,
                spawned_at_secs: spawned_at_secs as f64,
            });
            // WreckSpawned não existe mais (MF-031): o wreck aparece no
            // snapshot de quem o enxerga, com o mesmo recorte de AOI.
            info!(
                wreck_num,
                x = destruction.victim_x,
                y = destruction.victim_y,
                "wreck no mar aguardando saqueadores"
            );
        }
    }
}

/// Dev respawn (PRD §39): conveniência de teste — o loop do vertical slice
/// não pode matar a sessão do jogador. A regra definitiva de reconstrução é
/// o Dock (PRD §38, Phase 7). Também avança o `TickCounter` no fim do tick.
#[allow(clippy::too_many_arguments)]
fn respawn_destroyed_ships(
    mut commands: Commands,
    mut connection_manager: ResMut<ConnectionManager>,
    mut ship_ids: ResMut<ShipIdCounter>,
    dev: Res<DevItems>,
    dev_ships: Res<crate::crafting::DevShips>,
    map: Res<ServerWorldMap>,
    pending: Res<PendingShipDestructions>,
    mut counter: ResMut<crate::plugin::TickCounter>,
) {
    for destruction in &pending.0 {
        let Some(victim_client_id) = destruction.victim_client_id else {
            continue;
        };

        let new_ship_id = spawn_ship_for(
            &mut commands,
            &mut ship_ids,
            &dev,
            &dev_ships,
            &map.0,
            ShipKind::SmallMerchant,
            Some(victim_client_id),
            destruction.victim_character,
            Vec::new(),
        );
        let _ = connection_manager.send_message::<ReliableChannel, _>(
            victim_client_id,
            &AssignShip {
                ship_id: new_ship_id,
                kind: ShipKind::SmallMerchant,
            },
        );
        if let Some(zone) = zone_changed_for(&map.0, new_ship_id, DEV_SPAWN.0, DEV_SPAWN.1) {
            let _ = connection_manager.send_message::<ReliableChannel, _>(victim_client_id, &zone);
        }
        info!(
            client = ?victim_client_id,
            new_ship_id,
            "dev respawn (PRD §39)"
        );
    }

    counter.0 += 1;
}

/// Face protocolar de um navio autoritativo (MF-043: cooldowns são display;
/// o servidor continua dono do `BroadsideBattery`).
fn to_ship_state(ship: &ServerShip, catalog: &ItemCatalog) -> ShipState {
    ShipState {
        ship_id: ship.ship_id,
        kind: ship.kind,
        x: ship.motion.x,
        y: ship.motion.y,
        heading: ship.motion.heading,
        speed: ship.motion.speed,
        cargo_weight: ship
            .hold
            .used_weight(catalog)
            .expect("porão só contém definições do catálogo"),
        hp: ship.hp,
        max_hp: ship.stats.max_hp,
        max_speed: ship.stats.speed,
        weapon_damage: ship.stats.weapon_damage,
        weapon_range: ship.stats.weapon_range,
        port_cooldown_secs: ship.battery.port_cooldown,
        starboard_cooldown_secs: ship.battery.starboard_cooldown,
        is_npc: false,
        cargo_capacity: ship.stats.cargo_capacity,
    }
}

/// Snapshot de rede a 20 Hz (ADR-0008, MF-032) — a simulação corre a 30 Hz;
/// o relógio ([`SnapshotClock`]) decide se ESTE tick transmite. O estado do
/// mundo é coletado uma vez e recortado por destinatário via AOI
/// (ADR-0009, MF-031): cada cliente recebe o SEU mundo.
#[allow(clippy::too_many_arguments)]
fn send_snapshots(
    mut connection_manager: ResMut<ConnectionManager>,
    counter: Res<crate::plugin::TickCounter>,
    mut clock: ResMut<SnapshotClock>,
    dev: Res<DevItems>,
    time: Res<Time>,
    ships: Query<(Entity, &mut ServerShip)>,
    npc_ships: Query<&NpcShip>,
    projectiles: Query<(Entity, &mut ServerProjectile)>,
    wrecks: Query<&ServerWreck>,
) {
    if advance_snapshot_clock(&mut clock.accumulator, f64::from(time.delta_secs())) == 0 {
        return;
    }

    let wreck_states: Vec<WreckState> = wrecks
        .iter()
        .map(|wreck| WreckState {
            wreck_id: wreck.wreck_num,
            x: wreck.x,
            y: wreck.y,
            stack_count: wreck.chest.items().len() as u32,
        })
        .collect();
    let mut ship_states: Vec<ShipState> = ships
        .iter()
        .map(|(_, ship)| to_ship_state(ship, &dev.catalog))
        .collect();
    ship_states.extend(
        npc_ships
            .iter()
            .map(|npc| crate::npc::to_npc_ship_state(npc, &dev.catalog)),
    );
    let projectile_states: Vec<ProjectileState> = projectiles
        .iter()
        .map(|(_, projectile)| ProjectileState {
            projectile_id: projectile.0.projectile_id,
            x: projectile.0.x,
            y: projectile.0.y,
            heading: projectile.0.heading,
        })
        .collect();

    for (_, viewer) in ships.iter() {
        let Some(client_id) = viewer.client_id else {
            continue; // navio fantasma (grace): ninguém para receber
        };
        let snapshot = crate::aoi::build_snapshot(
            u64::from(counter.0),
            (viewer.motion.x, viewer.motion.y),
            &ship_states,
            &projectile_states,
            &wreck_states,
        );
        let _ = connection_manager.send_message_to_target::<UnreliableChannel, _>(
            &snapshot,
            NetworkTarget::Single(client_id),
        );
    }
}

/// Telemetria de mundo (PRD §72/§71): posição, zona e o pulso econômico
/// (ouro cunhado/queimado/volume) a cada 5s.
fn world_status(
    time: Res<Time>,
    map: Res<ServerWorldMap>,
    market: Res<crate::market::ServerMarket>,
    metrics: Res<Metrics>,
    ships: Query<&ServerShip>,
    mut timer: Local<f32>,
) {
    *timer += time.delta_secs();
    if *timer < 5.0 {
        return;
    }
    *timer = 0.0;
    let avg_trip_secs = if metrics.trip_count > 0 {
        metrics.trip_total_secs / metrics.trip_count as f64
    } else {
        0.0
    };
    let average_cargo_value_at_risk = metrics
        .cargo_value_at_risk_total
        .checked_div(metrics.cargo_value_trip_count)
        .unwrap_or(0);
    // §72 players_per_zone: snapshot por pulso, não persiste entre pulsos.
    let mut players_per_zone: std::collections::BTreeMap<&'static str, u32> =
        std::collections::BTreeMap::new();
    for ship in &ships {
        if ship.client_id.is_some() {
            if let Ok(zone) = map.0.zone_at(ship.motion.x, ship.motion.y) {
                *players_per_zone.entry(zone.name).or_insert(0) += 1;
            } else {
                *players_per_zone.entry("fora do mar declarado").or_insert(0) += 1;
            }
        }
    }
    info!(
        gold_minted = market.ledger.minted().0,
        gold_burned = market.ledger.burned().0,
        market_volume = market.ledger.market_volume().0,
        items_gathered = metrics.items_gathered,
        items_crafted = metrics.items_crafted,
        items_destroyed = metrics.items_destroyed,
        ships_constructed = metrics.ships_constructed,
        ships_destroyed = metrics.ships_destroyed,
        npc_bounty_gold_minted = metrics.npc_bounty_gold_minted,
        merchant_deaths =
            metrics.ship_losses_by_kind[mareforge_domain_ships::ShipKind::SmallMerchant as usize],
        patrol_deaths =
            metrics.ship_losses_by_kind[mareforge_domain_ships::ShipKind::Patrol as usize],
        corsair_deaths =
            metrics.ship_losses_by_kind[mareforge_domain_ships::ShipKind::Corsair as usize],
        wrecks_looted = metrics.wrecks_looted,
        pvp_engagements = metrics.pvp_engagements,
        zone_transitions = metrics.zone_transitions,
        completed_routes = ?metrics.completed_routes,
        same_port_returns = metrics.same_port_returns,
        trips_sunk = metrics.trips_sunk,
        cargo_value_departed = metrics.cargo_value_departed,
        cargo_value_arrived = metrics.cargo_value_arrived,
        cargo_value_sunk = metrics.cargo_value_sunk,
        average_cargo_value_at_risk,
        cargo_value_priced_items = metrics.cargo_value_priced_items,
        cargo_value_unpriced_items = metrics.cargo_value_unpriced_items,
        cargo_value_coverage_pct = metrics.cargo_value_coverage_pct,
        avg_trip_secs,
        trip_count = metrics.trip_count,
        players_per_zone = ?players_per_zone,
        "economic pulse"
    );
    for ship in &ships {
        let zone = map
            .0
            .zone_at(ship.motion.x, ship.motion.y)
            .map(|zone| zone.name)
            .unwrap_or("fora do mar declarado");
        info!(
            ship_id = ship.ship_id,
            x = format!("{:.1}", ship.motion.x),
            y = format!("{:.1}", ship.motion.y),
            speed = format!("{:.2}", ship.motion.speed),
            throttle = format!("{:.2}", ship.input.throttle),
            zone,
            "world status"
        );
    }
}

/// Fim da janela de graça (MF-035): dono não voltou. O navio sai do mundo e
/// é ancorado no store (casco, HP, posição e carga embarcada) — o próximo
/// hello do personagem restaura esse registro. NÃO vira wreck: não houve
/// naufrágio, e persistir E dropar duplicaria a carga. A política final de
/// "mundo persistido com dono offline" é decisão do próximo capítulo.
fn expire_ship_grace(
    mut commands: Commands,
    store: Res<crate::persist::StoreHandle>,
    ships: Query<(Entity, &ServerShip, &GraceWindow)>,
) {
    for (entity, ship, grace) in &ships {
        if grace.since.elapsed() < Duration::from_secs(DISCONNECT_GRACE_SECS) {
            continue;
        }
        if let Some(store) = store.0.as_ref() {
            let record = crate::persist::ShipRecord {
                ship_instance: ship.ship_instance,
                character: ship.character,
                kind: ship.kind,
                hp: ship.hp,
                x: ship.motion.x,
                y: ship.motion.y,
                heading: ship.motion.heading,
                cargo: ship.hold.items().to_vec(),
                equipped: ship.loadout.items().cloned().collect(),
                // MF-049: persiste a presença atual; o restore usa isso
                // para zerar ou iniciar a medição de trip.
                presence: ship.presence,
            };
            match store.save_ship(&record) {
                Ok(()) => info!(
                    ship_id = ship.ship_id,
                    x = ship.motion.x,
                    y = ship.motion.y,
                    "navio persistido no store (fim da janela de graça)"
                ),
                Err(error) => warn!(
                    error = %error,
                    ship_id = ship.ship_id,
                    "falha ao persistir navio; ele some do mundo"
                ),
            }
        }
        commands.entity(entity).despawn();
        info!(
            ship_id = ship.ship_id,
            "janela de graça expirou; navio deixou o mar"
        );
    }
}

/// Wrecks expirados somem do mar (PRD §26: 5 minutos; tuning no recurso).
/// Sem broadcast (MF-031): cada client percebe pela ausência no snapshot.
fn expire_wrecks(
    mut commands: Commands,
    time: Res<Time>,
    wreck_policy: Res<ServerWreckPolicy>,
    mut live_wrecks: ResMut<LiveWreckRecords>,
    wrecks: Query<(Entity, &ServerWreck)>,
) {
    let mut any_expired = false;
    for (entity, wreck) in &wrecks {
        let elapsed = time.elapsed_secs() - wreck.spawned_at_secs;
        if is_expired(elapsed, &wreck_policy.0) {
            info!(
                wreck_num = wreck.wreck_num,
                "wreck expirou e afundou de vez"
            );
            commands.entity(entity).despawn();
            any_expired = true;
        }
    }
    if any_expired {
        // Reconstrói o buffer em memória a partir dos wrecks que sobraram.
        live_wrecks.0.clear();
        for (_, wreck) in &wrecks {
            live_wrecks.0.push(crate::persist::WreckRecord {
                wreck_num: wreck.wreck_num,
                wreck_id: wreck.wreck_id,
                x: wreck.x,
                y: wreck.y,
                exclusive_looter: wreck.exclusive_looter,
                spawned_at_secs: wreck.spawned_at_secs as f64,
            });
        }
    }
}

/// Persiste o buffer `LiveWreckRecords` para o store ativo (MF-027 cont.).
/// Roda após `expire_wrecks` no fim do `FixedUpdate`, para que a lista em
/// memória reflita as remoções do tick antes da escrita.
fn persist_wrecks(store: Res<crate::persist::StoreHandle>, live_wrecks: Res<LiveWreckRecords>) {
    store.save_wreck_quiet(&live_wrecks.0);
}

/// Carrega os wrecks persistidos e respawna os que ainda não expiraram
/// (MF-027 cont., PRD §67). O `spawned_at_secs` preservado é interpretado
/// como "tempo decorrido no boot anterior"; o novo `spawned_at_secs` é
/// calculado como `now - elapsed_in_old_boot` para preservar o tempo de
/// vida restante.
fn load_wrecks(
    mut commands: Commands,
    store: Res<crate::persist::StoreHandle>,
    time: Res<Time>,
    wreck_policy: Res<ServerWreckPolicy>,
    mut wreck_ids: ResMut<WreckIdCounter>,
    mut live_wrecks: ResMut<LiveWreckRecords>,
) {
    let Some(store) = store.0.clone() else {
        return;
    };
    let records = match store.load_wreck_snapshot() {
        Ok(records) => records,
        Err(error) => {
            warn!(error = %error, "snapshot de wrecks ilegível; mundo sem wrecks");
            return;
        }
    };
    let now = time.elapsed_secs();
    let mut respawned = 0u32;
    let mut dropped = 0u32;
    for record in records {
        let elapsed_in_old_boot = record.spawned_at_secs as f32;
        let remaining = wreck_policy.0.total_lifetime_secs - elapsed_in_old_boot;
        if remaining <= 0.0 {
            dropped += 1;
            continue;
        }
        let new_spawned_at_secs = now - elapsed_in_old_boot;
        let chest = WreckChest::new(record.wreck_id);
        commands.spawn((ServerWreck {
            wreck_num: record.wreck_num,
            wreck_id: record.wreck_id,
            chest,
            exclusive_looter: record.exclusive_looter,
            spawned_at_secs: new_spawned_at_secs,
            x: record.x,
            y: record.y,
        },));
        live_wrecks.0.push(crate::persist::WreckRecord {
            wreck_num: record.wreck_num,
            wreck_id: record.wreck_id,
            x: record.x,
            y: record.y,
            exclusive_looter: record.exclusive_looter,
            spawned_at_secs: record.spawned_at_secs,
        });
        respawned += 1;
    }
    if let Some(max) = live_wrecks.0.iter().map(|w| w.wreck_num).max() {
        wreck_ids.0 = wreck_ids.0.max(max + 1);
    }
    info!(
        respawned,
        dropped, "wrecks restaurados do store (MF-027 cont.)"
    );
}

/// Orders expiradas (MF-041): escrow volta ao storage do seller e o client
/// recebe o board ativo sem a ordem vencida.
fn expire_orders(
    mut market: ResMut<crate::market::ServerMarket>,
    mut connection_manager: ResMut<ConnectionManager>,
    dev: Res<DevItems>,
    map: Res<ServerWorldMap>,
    ships: Query<&ServerShip>,
) {
    let expired = market.expire_orders(Utc::now());
    if expired == 0 {
        return;
    }
    info!(expired, "orders expiradas; escrow devolvido ao storage");
    let viewers = crate::market::viewers_of(&ships);
    crate::market::broadcast_orders(
        &mut connection_manager,
        &market,
        &dev.catalog,
        &map.0,
        &viewers,
    );
}

/// Atracar (MF-036): validação pura via `domain-ships::dock` — dentro da
/// área do porto, devagar o bastante e não estava atracado. Atracado, o
/// casco congela e os serviços de porto ligam.
#[allow(clippy::too_many_arguments)]
fn handle_dock(
    mut dock_events: EventReader<ServerReceiveMessage<Dock>>,
    mut connection_manager: ResMut<ConnectionManager>,
    map: Res<ServerWorldMap>,
    policy: Res<ServerDockPolicy>,
    dev: Res<DevItems>,
    market: Res<crate::market::ServerMarket>,
    time: Res<Time>,
    mut metrics: ResMut<Metrics>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in dock_events.read() {
        let client_id = event.from();
        let Some(mut ship) = ships
            .iter_mut()
            .find(|ship| ship.client_id == Some(client_id))
        else {
            continue;
        };
        let at_port = crate::market::port_region(&map.0, ship.motion.x, ship.motion.y);
        match dock_vessel(
            &ship.presence,
            ship.motion.speed,
            at_port.map(|(region, _)| region),
            &policy.0,
        ) {
            Ok(presence) => {
                let (destination, name) = at_port.expect("dock validou que há porto aqui");
                ship.presence = presence;
                info!(ship_id = ship.ship_id, region = name, "navio atracado");
                // MF-049: trip termina em dock bem-sucedido. Próxima trip
                // começa no undock.
                finalize_trip(
                    &mut ship,
                    &mut metrics,
                    time.elapsed_secs(),
                    TripOutcome::Docked(destination),
                );
                let _ = connection_manager.send_message::<ReliableChannel, _>(
                    client_id,
                    &DockResult {
                        success: true,
                        docked: true,
                        reason: format!("atracado em {name}"),
                    },
                );
                let storage = market
                    .port_storage(ship.character, destination)
                    .unwrap_or_default();
                let _ = connection_manager.send_message::<ReliableChannel, _>(
                    client_id,
                    &port_storage_snapshot(&dev.catalog, name, storage),
                );
            }
            Err(error) => {
                info!(ship_id = ship.ship_id, error = %error, "atracagem recusada");
                let _ = connection_manager.send_message::<ReliableChannel, _>(
                    client_id,
                    &DockResult {
                        success: false,
                        docked: false,
                        reason: error.to_string(),
                    },
                );
            }
        }
    }
}

/// Linhas de storage para a UI, agregando pilhas por item e omitindo itens
/// desconhecidos do catálogo (fail-closed: UI não inventa nome).
fn port_storage_snapshot(
    catalog: &ItemCatalog,
    region: &str,
    storage: &[Custody],
) -> PortStorageSnapshot {
    let mut quantities: HashMap<ItemDefinitionId, u32> = HashMap::new();
    for custody in storage {
        *quantities.entry(custody.instance.definition).or_default() += custody.instance.quantity;
    }
    let mut lines: Vec<StorageLine> = quantities
        .into_iter()
        .filter_map(|(item, quantity)| {
            let item_name = catalog.get(item)?.display_name.clone();
            Some(StorageLine {
                item,
                item_name,
                quantity,
            })
        })
        .collect();
    lines.sort_by(|a, b| a.item_name.cmp(&b.item_name));
    PortStorageSnapshot {
        region: region.to_owned(),
        lines,
    }
}

/// Desatracar (MF-036): de volta ao ponto de atracação (que é onde o casco
/// parou — a doca é onde você deixou o navio), mesmo HP, mesma carga.
fn handle_undock(
    mut undock_events: EventReader<ServerReceiveMessage<Undock>>,
    mut connection_manager: ResMut<ConnectionManager>,
    mut metrics: ResMut<Metrics>,
    price_index: Res<crate::market::ServerPriceIndex>,
    time: Res<Time>,
    mut ships: Query<&mut ServerShip>,
) {
    for event in undock_events.read() {
        let client_id = event.from();
        let Some(mut ship) = ships
            .iter_mut()
            .find(|ship| ship.client_id == Some(client_id))
        else {
            continue;
        };
        let origin_port = match ship.presence {
            VesselPresence::Docked(region) => Some(region),
            VesselPresence::AtSea => None,
        };
        match undock_vessel(&ship.presence) {
            Ok(presence) => {
                let origin_port = origin_port.expect("undock validou que o navio estava atracado");
                ship.presence = presence;
                // MF-049: trip começa em undock bem-sucedido. Dock anterior
                // já encerrou a trip anterior, então `start_trip` é o ponto
                // de partida autoritativo.
                start_trip(
                    &mut ship,
                    &mut metrics,
                    &price_index,
                    time.elapsed_secs(),
                    origin_port,
                );
                info!(ship_id = ship.ship_id, "navio desatracou");
                let _ = connection_manager.send_message::<ReliableChannel, _>(
                    client_id,
                    &DockResult {
                        success: true,
                        docked: false,
                        reason: String::from("desatracou — amarras soltas"),
                    },
                );
            }
            Err(error) => {
                info!(ship_id = ship.ship_id, error = %error, "desatracagem recusada");
                let _ = connection_manager.send_message::<ReliableChannel, _>(
                    client_id,
                    &DockResult {
                        success: false,
                        docked: true,
                        reason: error.to_string(),
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mareforge_domain_items::ItemLocation;
    use mareforge_shared::ids::RegionId;

    use super::*;

    /// MF-032 (ADR-0008): 3 segundos de simulação a 30 Hz = 90 ticks e
    /// ~60 snapshots a 20 Hz. O relógio é o acoplador entre os dois — e a
    /// tolerância é mínima (±1), só o essencial para aritmética de ponto
    /// flutuante em 1/30 e 1/20.
    #[test]
    fn snapshot_clock_beats_at_20hz_over_a_30hz_tick() {
        let mut accumulator = 0.0f64;
        let mut snapshots = 0u32;
        let mut ticks = 0u32;
        while ticks < 90 {
            snapshots += advance_snapshot_clock(&mut accumulator, 1.0 / SIM_HZ);
            ticks += 1;
        }
        assert_eq!(ticks, 90);
        assert!(
            (60i32 - snapshots as i32).abs() <= 1,
            "esperado ~60 snapshots em 3s, veio {snapshots}"
        );
    }

    /// MF-029 (§72): os novos contadores de telemetria de gameplay
    /// inicializam zerados e respeitam o índice `ShipKind as usize` para
    /// `ship_losses_by_kind`. O índice SmallMerchant=0, Patrol=1,
    /// Corsair=2 é parte do contrato com os chamadores (simulate_world e
    /// o log de pulse).
    #[test]
    fn metrics_gameplay_counters_start_at_zero_and_index_by_kind() {
        let metrics = Metrics::default();
        assert_eq!(metrics.ship_losses_by_kind, [0; 3]);
        assert_eq!(metrics.wrecks_looted, 0);
        assert_eq!(metrics.pvp_engagements, 0);
        // Índice por ShipKind (parte do contrato; mantenha em sincronia
        // com o array no struct).
        assert_eq!(ShipKind::SmallMerchant as usize, 0);
        assert_eq!(ShipKind::Patrol as usize, 1);
        assert_eq!(ShipKind::Corsair as usize, 2);

        // Simulação de contagem: incrementar como o simulate_world faz.
        let mut metrics = Metrics::default();
        metrics.ship_losses_by_kind[ShipKind::SmallMerchant as usize] += 1;
        metrics.ship_losses_by_kind[ShipKind::Corsair as usize] += 1;
        metrics.wrecks_looted += 3;
        metrics.pvp_engagements += 7;
        assert_eq!(
            metrics.ship_losses_by_kind[ShipKind::SmallMerchant as usize],
            1
        );
        assert_eq!(metrics.ship_losses_by_kind[ShipKind::Patrol as usize], 0);
        assert_eq!(metrics.ship_losses_by_kind[ShipKind::Corsair as usize], 1);
        assert_eq!(metrics.wrecks_looted, 3);
        assert_eq!(metrics.pvp_engagements, 7);
    }

    /// MF-043: o snapshot copia a recarga autoritativa do `BroadsideBattery`
    /// para os campos de display do client.
    #[test]
    fn ship_state_populates_battery_cooldowns() {
        let ship_instance = ShipInstanceId::new();
        let definition = crate::crafting::DevShips::new()
            .definition(ShipKind::SmallMerchant)
            .clone();
        let stats = compute_ship_stats(
            &definition,
            &EquippedComponents::default(),
            &ItemCatalog::default(),
        )
        .expect("stats base do merchant não falham");
        let expected_cargo_capacity = stats.cargo_capacity;
        let ship = ServerShip {
            ship_id: 7,
            client_id: None,
            character: CharacterId::new(),
            ship_instance,
            kind: ShipKind::SmallMerchant,
            presence: VesselPresence::AtSea,
            loadout: ShipLoadout::new(),
            input: ShipInput {
                throttle: 0.0,
                turn: 0.0,
            },
            hp: stats.max_hp,
            hold: CargoHold::new(ship_instance, stats.cargo_capacity),
            battery: BroadsideBattery {
                port_cooldown: 2.5,
                starboard_cooldown: 0.75,
            },
            stats,
            motion: ShipMotion {
                x: 10.0,
                y: 20.0,
                ..ShipMotion::default()
            },
            tuning: MotionTuning::default(),
            zone: None,
            trip: None,
            restored_trip_started_at: None,
        };

        let state = to_ship_state(&ship, &ItemCatalog::default());
        assert_eq!(state.ship_id, 7);
        assert_eq!(state.kind, ShipKind::SmallMerchant);
        assert_eq!(state.x, 10.0);
        assert_eq!(state.y, 20.0);
        assert_eq!(state.cargo_weight, 0);
        assert_eq!(state.port_cooldown_secs, 2.5);
        assert_eq!(state.starboard_cooldown_secs, 0.75);
        assert!(!state.is_npc);
        assert_eq!(state.cargo_capacity, expected_cargo_capacity);
    }

    /// Simulação e snapshot não são a mesma cadência: há ticks de 30 Hz em
    /// que NENHUM snapshot vence (30/20 = 1,5 — alterna 2 e 1 ticks).
    #[test]
    fn not_every_simulation_tick_sends_a_snapshot() {
        let mut accumulator = 0.0f64;
        let due: Vec<u32> = (0..30)
            .map(|_| advance_snapshot_clock(&mut accumulator, 1.0 / SIM_HZ))
            .collect();
        let sent = due.iter().sum::<u32>();
        assert!(
            (20i32 - sent as i32).abs() <= 1,
            "30 ticks de simulação devem gerar ~20 snapshots, veio {sent}"
        );
        assert!(
            due.contains(&0),
            "deve haver tick sem snapshot (30 Hz ≠ 20 Hz)"
        );
    }

    fn storage_catalog() -> (ItemCatalog, ItemDefinitionId, ItemDefinitionId) {
        let timber = ItemDefinitionId::new();
        let hull = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        catalog
            .register(ItemDefinition {
                id: timber,
                kind: ItemKind::Resource,
                equipment: None,
                max_stack: 100,
                base_weight: 2,
                tags: SmallVec::new(),
                display_name: String::from("Madeira"),
            })
            .expect("catálogo de teste não registra duplicatas");
        catalog
            .register(ItemDefinition::equipment(
                hull,
                String::from("Casco Reforçado"),
                8,
                EquipmentSlot::Hull,
                EquipmentStats::default(),
            ))
            .expect("catálogo de teste não registra duplicatas");
        (catalog, timber, hull)
    }

    fn custody(item: ItemDefinitionId, quantity: u32, region: RegionId) -> Custody {
        Custody {
            instance: ItemInstance::new_resource(ItemInstanceId::new(), item, quantity),
            location: ItemLocation::PortStorage(region),
        }
    }

    #[test]
    fn port_storage_snapshot_aggregates_duplicate_stacks() {
        let (catalog, timber, _) = storage_catalog();
        let region = RegionId::new();
        let snapshot = port_storage_snapshot(
            &catalog,
            "Porto da Serra",
            &[custody(timber, 10, region), custody(timber, 15, region)],
        );

        assert_eq!(snapshot.region, "Porto da Serra");
        assert_eq!(snapshot.lines.len(), 1);
        assert_eq!(snapshot.lines[0].item, timber);
        assert_eq!(snapshot.lines[0].item_name, "Madeira");
        assert_eq!(snapshot.lines[0].quantity, 25);
    }

    #[test]
    fn port_storage_snapshot_skips_items_missing_from_catalog() {
        let (catalog, _, hull) = storage_catalog();
        let region = RegionId::new();
        let snapshot = port_storage_snapshot(
            &catalog,
            "Porto da Serra",
            &[
                custody(hull, 1, region),
                custody(ItemDefinitionId::new(), 7, region),
            ],
        );

        assert_eq!(snapshot.lines.len(), 1);
        assert_eq!(snapshot.lines[0].item, hull);
    }

    #[test]
    fn port_storage_snapshot_includes_equipment_and_resources() {
        let (catalog, timber, hull) = storage_catalog();
        let region = RegionId::new();
        let snapshot = port_storage_snapshot(
            &catalog,
            "Porto da Serra",
            &[custody(hull, 1, region), custody(timber, 5, region)],
        );

        assert_eq!(snapshot.lines.len(), 2);
        assert!(snapshot
            .lines
            .iter()
            .any(|line| line.item == hull && line.quantity == 1));
    }
}
