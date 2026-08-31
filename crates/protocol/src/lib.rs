//! protocol: tipos de wire do mareForge (PRD §63/§64).
//!
//! Tipos puros com serde — o transporte (lightyear, ADR-0002) escolhe o
//! formato na camada dele; este crate define apenas o **contrato** entre
//! client e servidor. Versionamento no handshake conforme ADR-0011.

use mareforge_domain_combat::weapon::BroadsideSide;
use mareforge_domain_crafting::recipe::StationKind;
use mareforge_domain_items::EquipmentSlot;
use mareforge_domain_ships::ShipKind;
use mareforge_domain_world::RiskTier;
use mareforge_shared::ids::ItemDefinitionId;
use serde::{Deserialize, Serialize};

/// Versão atual do protocolo. Qualquer mudança incompatível deve incrementar
/// este número (semântica: quebra de contrato = +1).
///
/// v2: combate — FireBroadside, ShipDestroyed e projéteis no snapshot.
/// v3: economia do naufrágio — cargo_weight no ShipState, ciclo de vida de
///     Wreck (WreckSpawned/WreckRemoved) e loot (LootWreck/LootResult).
/// v4: geografia de risco (MF-017) — ZoneChanged com tier e nome da zona;
///     o servidor é quem calcula a zona real (PRD §10), a UI representa.
/// v5: economia de recursos (MF-018/019) — nós visíveis (NodesSnapshot no
///     hello, NodeUpdated em toda mudança), coleta GatherNode/GatherResult.
/// v6: crafting (MF-021/022) — catálogo de receitas no hello
///     (RecipesSnapshot), intents CraftItem e veredito CraftResult.
/// v7: mercado regional (MF-023..026) — catálogo de itens e carteira no
///     hello, intents de storage (Z/X), sell/cancel/buy de orders, e
///     veredito MarketResult.
/// v8: A1-P0 — identidade estável (MF-035): `ClientHello.identity` carrega o
///     token persistente do jogador; a conexão nunca é dona de nada. E AOI
///     (MF-031): `WorldSnapshot` passa a ser construído por destinatário e
///     inclui wrecks visíveis (`WreckState`) — WreckSpawned/WreckRemoved
///     saem do protocolo (visibilidade de wreck vem do snapshot, com TTL no
///     client).
/// v9: A1-P1 — dock/undock (MF-036): intents `Dock`/`Undock` e veredito
///     `DockResult` (com estado resultante `docked`). Atracar é um ESTADO
///     explícito: serviços de porto (storage, craft, mercado, construção)
///     exigem `Docked`; movimento/tiro/coleta/saque exigem `AtSea`.
/// v10: A1-P2 — loadout (MF-039): intents `EquipItem`/`UnequipItem`,
///     veredito `LoadoutResult` e `LoadoutSnapshot` no handshake. E o
///     `ShipState` passa a carregar os stats AUTORITATIVOS (hp, max_hp,
///     velocidade máxima, dano e alcance) — o client nunca deriva stats.
/// v11: MF-056B — `ShipState.kind` é a identidade visual autoritativa do
///     casco; o client escolhe sprite, nunca stats ou regras.
/// v12: MF-056H — `ShipState.cargo_capacity` expõe o limite autoritativo do
///     porão para a UI.
pub const PROTOCOL_VERSION: u16 = 12;

/// Primeira mensagem do client após conectar (ADR-0011). `identity` é o
/// token persistente do jogador (MF-035): o servidor resolve token →
/// CharacterId; ClientId/conexão é só transporte da sessão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientHello {
    pub protocol_version: u16,
    pub identity: String,
}

impl ClientHello {
    /// Hello da versão atual com o token de identidade do jogador.
    pub fn current(identity: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            identity: identity.into(),
        }
    }
}

/// Resposta do servidor. Conexão com versão incompatível é rejeitada
/// (`accepted == false`) e encerrada em seguida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerWelcome {
    pub protocol_version: u16,
    pub accepted: bool,
}

/// O servidor atribui um navio ao jogador aceito (janela com visão própria).
/// O `kind` alimenta a UI de loadout para filtrar itens compatíveis (MF-039).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignShip {
    pub ship_id: u32,
    pub kind: ShipKind,
}

/// Intenção de navegação do jogador (PRD §63: ShipInput).
/// Campos espelham `MotionInput` de `domain-ships`; o servidor é quem
/// valida e aplica (Pilar 4).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShipInput {
    /// [0, 1] — velas.
    pub throttle: f32,
    /// [-1, 1] — leme; positivo = bombordo (anti-horário).
    pub turn: f32,
}

/// Estado autoritativo de um navio no tick do snapshot (PRD §64: ShipState).
/// Desde o v10 carrega os STATS do servidor (MF-039): o client exibe, nunca
/// calcula — equipar vela/casco/canhão aparece aqui no próximo snapshot.
/// v10+: os cooldowns de bordo são campos de display do client; a verdade
/// continua no servidor (`BroadsideBattery`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShipState {
    pub ship_id: u32,
    /// Tipo autoritativo para apresentação. Nunca participa de regras no client.
    pub kind: mareforge_domain_ships::ShipKind,
    pub x: f32,
    pub y: f32,
    /// Radianos, 0 = +X, anti-horário (convenção de `domain-ships`).
    pub heading: f32,
    /// m/s (atual).
    pub speed: f32,
    /// Peso atual da carga (unidades de peso; o limite é do navio).
    pub cargo_weight: u32,
    pub hp: u32,
    pub max_hp: u32,
    /// m/s (máximo com o loadout atual).
    pub max_speed: f32,
    pub weapon_damage: u32,
    pub weapon_range: f32,
    #[serde(default)]
    pub port_cooldown_secs: f32,
    #[serde(default)]
    pub starboard_cooldown_secs: f32,
    /// MF-044: NPC é não-player. Aditivo, default false.
    #[serde(default)]
    pub is_npc: bool,
    /// MF-056H: limite de peso do porão com o loadout atual. Aditivo,
    /// default 0 (cliente antigo não quebra).
    #[serde(default)]
    pub cargo_capacity: u32,
}

/// Instala um item do storage regional no slot dele (MF-039). Só atracado;
/// slot ocupado é swap — o antigo volta ao storage, nada é destruído.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipItem {
    pub item: ItemDefinitionId,
}

/// Desinstala o slot; o item volta ao storage da região onde está atracado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnequipItem {
    pub slot: EquipmentSlot,
}

/// Uma linha do loadout do PRÓPRIO navio: o slot existe no casco? há item?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadoutLine {
    pub slot: EquipmentSlot,
    /// Display name do item equipado (vazio quando o slot está livre).
    pub item_name: String,
    pub equipped: bool,
}

/// Loadout completo do navio do observador, no hello e a cada troca.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadoutSnapshot {
    pub slots: Vec<LoadoutLine>,
}

/// Veredito de equipar/desequipar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadoutResult {
    pub success: bool,
    pub reason: String,
}

/// Comando de disparo de bordo do jogador (PRD §19). Confiável: é um clique,
/// perder um tiro é perder gameplay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FireBroadside {
    pub side: BroadsideSide,
}

/// Estado autoritativo de um projétil no tick do snapshot (PRD §20).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectileState {
    pub projectile_id: u32,
    pub x: f32,
    pub y: f32,
    /// Direção de voo em radianos.
    pub heading: f32,
}

/// Navio afundou (PRD §21). O servidor retransmite a todos; a resolução de
/// loot acontece do lado do servidor (MF-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipDestroyed {
    pub ship_id: u32,
}

/// Destroço visível no snapshot do destinatário (MF-031: visibilidade de
/// wreck é recorte de AOI como navios e projéteis).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WreckState {
    pub wreck_id: u32,
    pub x: f32,
    pub y: f32,
    /// Quantidade de pilhas de itens dentro do baú (para UI).
    pub stack_count: u32,
}

/// Snapshot do mundo **do ponto de vista do destinatário** (PRD §64, ADR-0009,
/// MF-031): só entidades dos chunks visíveis + anel de borda. Enviado a 20 Hz
/// (ADR-0008) por canal não-confiável.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub tick: u64,
    pub ships: Vec<ShipState>,
    pub projectiles: Vec<ProjectileState>,
    pub wrecks: Vec<WreckState>,
}

/// Jogador quer saquear um wreck (PRD §27: precisa estar nele, com porão).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootWreck {
    pub wreck_id: u32,
}

/// Resultado da tentativa de saque (MF-015: atômico, capacity-aware).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootResult {
    pub wreck_id: u32,
    pub success: bool,
}

/// A zona real do navio mudou (PRD §10, MF-017). O servidor define a zona;
/// o client apenas representa. Enviado por canal confiável para o dono do
/// navio — no spawn (estado inicial) e a cada travessia de fronteira.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneChanged {
    pub ship_id: u32,
    pub tier: RiskTier,
    /// Nome declarado da zona no mapa do servidor (ex.: "Rota da Costa").
    pub zone_name: String,
}

/// Estado visível de um nó de recurso (PRD §64: ResourceNodeUpdated,
/// MF-018). Nós são server-authoritative: o client desenha e pede.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeState {
    /// Id numérico do nó (protocolo usa u32, como wrecks e navios).
    pub node_id: u32,
    pub x: f32,
    pub y: f32,
    /// Nome do recurso para a UI (display name do catálogo do servidor).
    pub resource_name: String,
    /// Unidades disponíveis agora (0 = esgotado, aguardando respawn).
    pub stock: u32,
    pub max_stock: u32,
}

/// Todos os nós do mundo, enviados no handshake — depois, só deltas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodesSnapshot {
    pub nodes: Vec<NodeState>,
}

/// Um nó mudou (coleta de outro jogador ou respawn). Mesmo formato do
/// estado: o client substitui o que sabe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeUpdated {
    pub node: NodeState,
}

/// Jogador quer coletar um nó (PRD MF-019: perto, com estoque e porão).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatherNode {
    pub node_id: u32,
}

/// Resultado da tentativa de coleta (MF-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatherResult {
    pub node_id: u32,
    pub success: bool,
    /// Unidades efetivamente coletadas (0 quando falha).
    pub gathered: u32,
}

/// Uma receita do catálogo do servidor, pronta para exibição (MF-021/022).
/// O `recipe_id` é o índice numérico que o client devolve em `CraftItem`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeEntry {
    pub recipe_id: u32,
    pub display_name: String,
    /// Estação exigida (Workbench/Dock/None — §36).
    pub station: StationKind,
    /// "item" quando produz equipamento/recurso; "navio" quando o output é
    /// um ShipInstance construído no Dock (PRD §38: navio não é item).
    pub ship_build: bool,
    pub output_name: String,
    /// Quantidade produzida por craft (1 para equipamento).
    pub output_quantity: u32,
    /// Linhas de ingredientes já resolvidas para UI (nome + quantidade).
    pub ingredients: Vec<IngredientLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngredientLine {
    pub name: String,
    pub quantity: u32,
}

/// Catálogo completo de receitas, enviado no handshake — estático na sessão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipesSnapshot {
    pub recipes: Vec<RecipeEntry>,
}

/// Jogador quer fabricar (PRD §63: CraftItem). O servidor valida estação,
/// ingredientes e porão — fail-closed (§37).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CraftItem {
    pub recipe_id: u32,
}

/// Resultado da tentativa de fabricação/construção.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CraftResult {
    pub recipe_id: u32,
    pub success: bool,
}

/// Linha do catálogo de itens (MF-023): id real para os intents, nome e
/// peso para a UI. Enviada no handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemLine {
    pub id: ItemDefinitionId,
    pub name: String,
    pub weight: u32,
    /// Slot do equipamento, quando o item é equipável (MF-038).
    #[serde(default)]
    pub equipment_slot: Option<EquipmentSlot>,
}

/// Catálogo completo de itens do servidor, no handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    pub items: Vec<ItemLine>,
}

/// Carteira global do personagem (PRD §31: ouro não afunda com o navio).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletUpdated {
    pub gold: u64,
}

/// Uma sell order visível (MF-025). `region` é o nome da região; ordens de
/// regiões diferentes NUNCA cruzam (§44) — a cor local decide a compra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderLine {
    pub order_num: u32,
    pub region: String,
    pub item_name: String,
    pub unit_price: u64,
    pub quantity: u32,
    /// A order é deste client (habilita o cancelar).
    pub mine: bool,
}

/// Todas as orders abertas, enviadas no hello e a cada mudança (delta no
/// OrderUpdated seria o próximo passo; o slice reenvia o quadro inteiro).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdersSnapshot {
    pub orders: Vec<OrderLine>,
}

/// Uma linha do storage regional do porto onde está atracado (post-review:
/// habilita a aba Loadout do PortScreen a oferecer "Equipar" via UI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageLine {
    pub item: ItemDefinitionId,
    pub item_name: String,
    pub quantity: u32,
}

/// Snapshot do storage do porto onde o jogador acabou de dockar. Enviado
/// após `DockResult { success: true, docked: true }`. Sem ele, a aba Loadout
/// só permite desequipar (post-review: agora também permite equipar itens
/// compatíveis com os slots do casco).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortStorageSnapshot {
    pub region: String,
    pub lines: Vec<StorageLine>,
}

/// Atracar no porto onde está (MF-036). O servidor valida: dentro da área
/// do porto, devagar o bastante. Confiável: é uma decisão do jogador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dock;

/// Desatracar (MF-036): volta ao ponto de atracação, mesmo HP, mesma carga.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Undock;

/// Veredito de dock/undock (MF-036). `docked` é o ESTADO resultante — o
/// client não infere presença por texto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockResult {
    pub success: bool,
    pub docked: bool,
    /// Texto de display (nome do porto ou motivo da recusa).
    pub reason: String,
}

/// Guarda TUDO do porão no storage regional do porto onde está (MF-023).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageDepositAll;

/// Tira do storage tudo que couber de volta no porão (MF-023).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageWithdrawAll;

/// Cria sell order no mercado do porto onde está (MF-024/025). O item sai
/// do storage regional e entra em escrow atomicamente no servidor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSellOrder {
    pub item: ItemDefinitionId,
    pub quantity: u32,
    pub unit_price: u64,
}

/// Cancela sua order; o item volta do escrow pro storage. Fee não volta (§46).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelSellOrder {
    pub order_num: u32,
}

/// Compra de uma order da região onde está (MF-025): ouro sai da carteira,
/// item vai pro seu storage local, seller recebe líquido da taxa (§47).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuySellOrder {
    pub order_num: u32,
    pub quantity: u32,
}

/// Veredito de qualquer operação de storage/mercado. `reason` é texto de
/// display para HUD/log — a máquina de verdade vive no servidor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketResult {
    pub success: bool,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_protocol_version_is_twelve() {
        assert_eq!(PROTOCOL_VERSION, 12);
        assert_eq!(
            ClientHello::current("token").protocol_version,
            PROTOCOL_VERSION
        );
    }

    #[test]
    fn ship_state_carries_authoritative_stats() {
        let state = ShipState {
            ship_id: 1,
            kind: mareforge_domain_ships::ShipKind::SmallMerchant,
            x: 0.0,
            y: 0.0,
            heading: 0.0,
            speed: 30.0,
            cargo_weight: 38,
            hp: 140,
            max_hp: 140,
            max_speed: 36.0,
            weapon_damage: 30,
            weapon_range: 55.0,
            port_cooldown_secs: 3.5,
            starboard_cooldown_secs: 1.25,
            is_npc: false,
            cargo_capacity: 100,
        };
        let bytes = bincode::serialize(&state).unwrap();
        let decoded = bincode::deserialize::<ShipState>(&bytes).unwrap();
        assert_eq!(decoded, state);
        assert_eq!(decoded.port_cooldown_secs, 3.5);
        assert_eq!(decoded.starboard_cooldown_secs, 1.25);
        assert_eq!(decoded.cargo_capacity, 100);
    }

    #[test]
    fn ship_state_roundtrips_is_npc_flag() {
        for is_npc in [false, true] {
            let state = ShipState {
                ship_id: 3,
                kind: mareforge_domain_ships::ShipKind::Patrol,
                x: 1.0,
                y: 2.0,
                heading: 0.5,
                speed: 0.0,
                cargo_weight: 0,
                hp: 70,
                max_hp: 70,
                max_speed: 40.0,
                weapon_damage: 25,
                weapon_range: 55.0,
                port_cooldown_secs: 0.0,
                starboard_cooldown_secs: 0.0,
                is_npc,
                cargo_capacity: 70,
            };
            let bytes = bincode::serialize(&state).unwrap();
            let decoded = bincode::deserialize::<ShipState>(&bytes).unwrap();
            assert_eq!(decoded, state);
            assert_eq!(decoded.is_npc, is_npc);
            assert_eq!(decoded.cargo_capacity, 70);
        }
    }

    #[test]
    fn loadout_messages_roundtrip() {
        let equip = EquipItem {
            item: ItemDefinitionId::new(),
        };
        let bytes = bincode::serialize(&equip).unwrap();
        assert_eq!(bincode::deserialize::<EquipItem>(&bytes).unwrap(), equip);

        let unequip = UnequipItem {
            slot: EquipmentSlot::Sail,
        };
        let bytes = bincode::serialize(&unequip).unwrap();
        assert_eq!(
            bincode::deserialize::<UnequipItem>(&bytes).unwrap(),
            unequip
        );

        let snapshot = LoadoutSnapshot {
            slots: vec![
                LoadoutLine {
                    slot: EquipmentSlot::Hull,
                    item_name: String::from("Casco Reforçado"),
                    equipped: true,
                },
                LoadoutLine {
                    slot: EquipmentSlot::Sail,
                    item_name: String::new(),
                    equipped: false,
                },
            ],
        };
        let bytes = bincode::serialize(&snapshot).unwrap();
        assert_eq!(
            bincode::deserialize::<LoadoutSnapshot>(&bytes).unwrap(),
            snapshot
        );
    }

    #[test]
    fn dock_messages_roundtrip() {
        let dock = Dock;
        let bytes = bincode::serialize(&dock).unwrap();
        assert_eq!(bincode::deserialize::<Dock>(&bytes).unwrap(), dock);

        let undock = Undock;
        let bytes = bincode::serialize(&undock).unwrap();
        assert_eq!(bincode::deserialize::<Undock>(&bytes).unwrap(), undock);

        let refused = DockResult {
            success: false,
            docked: false,
            reason: String::from("veloz demais para atracar"),
        };
        let bytes = bincode::serialize(&refused).unwrap();
        let decoded: DockResult = bincode::deserialize(&bytes).unwrap();
        assert!(!decoded.success);
        assert!(!decoded.docked);
    }

    #[test]
    fn port_storage_snapshot_roundtrips_through_bincode() {
        let message = PortStorageSnapshot {
            region: String::from("Porto da Serra"),
            lines: vec![
                StorageLine {
                    item: ItemDefinitionId::new(),
                    item_name: String::from("Madeira"),
                    quantity: 25,
                },
                StorageLine {
                    item: ItemDefinitionId::new(),
                    item_name: String::from("Casco Reforçado"),
                    quantity: 1,
                },
            ],
        };
        let bytes = bincode::serialize(&message).unwrap();
        assert_eq!(
            bincode::deserialize::<PortStorageSnapshot>(&bytes).unwrap(),
            message
        );
    }

    /// MF-043 estendeu `ShipState` com `port_cooldown_secs` e
    /// `starboard_cooldown_secs`. Em Alpha, cliente e servidor são deploy
    /// juntos, então o caminho "cliente antigo ↔ servidor novo" não
    /// acontece. Este teste documenta o limite: bytes truncados (simulando
    /// servidor antigo falando com cliente novo) falham de forma
    /// recuperável, sem panic nem corrupção silenciosa.
    #[test]
    fn ship_state_truncated_bytes_fail_cleanly_not_silently() {
        let full = ShipState {
            ship_id: 1,
            kind: mareforge_domain_ships::ShipKind::SmallMerchant,
            x: 0.0,
            y: 0.0,
            heading: 0.0,
            speed: 0.0,
            cargo_weight: 0,
            hp: 100,
            max_hp: 100,
            max_speed: 0.0,
            weapon_damage: 0,
            weapon_range: 0.0,
            port_cooldown_secs: 2.5,
            starboard_cooldown_secs: 1.5,
            is_npc: false,
            cargo_capacity: 100,
        };
        let bytes = bincode::serialize(&full).expect("encode");
        // Trunca 8 bytes (dois f32): simula cliente novo lendo servidor antigo.
        let truncated = &bytes[..bytes.len() - 8];
        let decoded: Result<ShipState, _> = bincode::deserialize(truncated);
        assert!(
            decoded.is_err(),
            "truncation must surface as Err, not panic"
        );
    }

    #[test]
    fn client_hello_roundtrips() {
        let message = ClientHello::current("jogador-abc");
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: ClientHello = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, message);
        assert_eq!(decoded.identity, "jogador-abc");
    }

    #[test]
    fn server_welcome_rejects_old_version() {
        let message = ServerWelcome {
            protocol_version: 99,
            accepted: false,
        };
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: ServerWelcome = bincode::deserialize(&bytes).unwrap();
        assert!(!decoded.accepted);
        assert_ne!(decoded.protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn ship_input_roundtrips_and_clamps_are_server_side() {
        let message = ShipInput {
            throttle: 0.75,
            turn: -0.5,
        };
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: ShipInput = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn world_snapshot_roundtrips_with_aoi_entities() {
        let message = WorldSnapshot {
            tick: 42,
            ships: vec![
                ShipState {
                    ship_id: 1,
                    kind: mareforge_domain_ships::ShipKind::SmallMerchant,
                    x: 12.5,
                    y: -3.25,
                    heading: 0.1,
                    speed: 4.0,
                    cargo_weight: 36,
                    hp: 100,
                    max_hp: 100,
                    max_speed: 30.0,
                    weapon_damage: 20,
                    weapon_range: 50.0,
                    port_cooldown_secs: 0.0,
                    starboard_cooldown_secs: 2.0,
                    is_npc: false,
                    cargo_capacity: 100,
                },
                ShipState {
                    ship_id: 2,
                    kind: mareforge_domain_ships::ShipKind::Corsair,
                    x: 0.0,
                    y: 0.0,
                    heading: 3.0,
                    speed: 0.0,
                    cargo_weight: 0,
                    hp: 70,
                    max_hp: 70,
                    max_speed: 40.0,
                    weapon_damage: 25,
                    weapon_range: 55.0,
                    port_cooldown_secs: 0.0,
                    starboard_cooldown_secs: 0.0,
                    is_npc: false,
                    cargo_capacity: 40,
                },
            ],
            projectiles: vec![ProjectileState {
                projectile_id: 7,
                x: 1.0,
                y: 2.0,
                heading: 1.5,
            }],
            wrecks: vec![WreckState {
                wreck_id: 9,
                x: 30.0,
                y: -10.0,
                stack_count: 2,
            }],
        };
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: WorldSnapshot = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, message);
        assert_eq!(decoded.wrecks[0].stack_count, 2);
    }

    #[test]
    fn loot_messages_roundtrip() {
        let loot = LootWreck { wreck_id: 9 };
        let bytes = bincode::serialize(&loot).unwrap();
        assert_eq!(bincode::deserialize::<LootWreck>(&bytes).unwrap(), loot);

        let result = LootResult {
            wreck_id: 9,
            success: true,
        };
        let bytes = bincode::serialize(&result).unwrap();
        assert_eq!(bincode::deserialize::<LootResult>(&bytes).unwrap(), result);
    }

    #[test]
    fn fire_broadside_roundtrips_both_sides() {
        for side in [
            mareforge_domain_combat::weapon::BroadsideSide::Port,
            mareforge_domain_combat::weapon::BroadsideSide::Starboard,
        ] {
            let message = FireBroadside { side };
            let bytes = bincode::serialize(&message).unwrap();
            let decoded: FireBroadside = bincode::deserialize(&bytes).unwrap();
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn ship_destroyed_roundtrips() {
        let message = ShipDestroyed { ship_id: 3 };
        let bytes = bincode::serialize(&message).unwrap();
        let decoded: ShipDestroyed = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn zone_changed_roundtrips_with_tier_and_name() {
        for tier in [
            mareforge_domain_world::RiskTier::Protected,
            mareforge_domain_world::RiskTier::Frontier,
            mareforge_domain_world::RiskTier::Lawless,
        ] {
            let message = ZoneChanged {
                ship_id: 7,
                tier,
                zone_name: String::from("Rota da Costa"),
            };
            let bytes = bincode::serialize(&message).unwrap();
            let decoded: ZoneChanged = bincode::deserialize(&bytes).unwrap();
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn node_messages_roundtrip() {
        let state = NodeState {
            node_id: 4,
            x: -430.0,
            y: 20.0,
            resource_name: String::from("Madeira"),
            stock: 50,
            max_stock: 60,
        };
        let snapshot = NodesSnapshot {
            nodes: vec![
                state.clone(),
                NodeState {
                    node_id: 5,
                    x: 0.0,
                    y: 900.0,
                    resource_name: String::from("Coral Negro"),
                    stock: 0,
                    max_stock: 30,
                },
            ],
        };
        let bytes = bincode::serialize(&snapshot).unwrap();
        let decoded: NodesSnapshot = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, snapshot);
        assert_eq!(decoded.nodes[1].stock, 0);

        let updated = NodeUpdated { node: state };
        let bytes = bincode::serialize(&updated).unwrap();
        assert_eq!(
            bincode::deserialize::<NodeUpdated>(&bytes).unwrap(),
            updated
        );

        let gather = GatherNode { node_id: 4 };
        let bytes = bincode::serialize(&gather).unwrap();
        assert_eq!(bincode::deserialize::<GatherNode>(&bytes).unwrap(), gather);

        let result = GatherResult {
            node_id: 4,
            success: true,
            gathered: 10,
        };
        let bytes = bincode::serialize(&result).unwrap();
        assert_eq!(
            bincode::deserialize::<GatherResult>(&bytes).unwrap(),
            result
        );
    }

    #[test]
    fn craft_messages_roundtrip_with_station_and_ingredients() {
        let entry = RecipeEntry {
            recipe_id: 3,
            display_name: String::from("Corsair"),
            station: mareforge_domain_crafting::recipe::StationKind::Dock,
            ship_build: true,
            output_name: String::from("Corsair"),
            output_quantity: 1,
            ingredients: vec![
                IngredientLine {
                    name: String::from("Minério"),
                    quantity: 40,
                },
                IngredientLine {
                    name: String::from("Coral Negro"),
                    quantity: 10,
                },
            ],
        };
        let snapshot = RecipesSnapshot {
            recipes: vec![entry],
        };
        let bytes = bincode::serialize(&snapshot).unwrap();
        let decoded: RecipesSnapshot = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, snapshot);
        assert_eq!(
            decoded.recipes[0].station,
            mareforge_domain_crafting::recipe::StationKind::Dock
        );

        let intent = CraftItem { recipe_id: 3 };
        let bytes = bincode::serialize(&intent).unwrap();
        assert_eq!(bincode::deserialize::<CraftItem>(&bytes).unwrap(), intent);

        let result = CraftResult {
            recipe_id: 3,
            success: false,
        };
        let bytes = bincode::serialize(&result).unwrap();
        assert_eq!(bincode::deserialize::<CraftResult>(&bytes).unwrap(), result);
    }
}
