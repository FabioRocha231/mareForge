//! Persistência do estado do servidor (MF-033/034, ADR-0004/0010).
//!
//! Boundary formal: o servidor fala com [`StateStore`], nunca com Postgres
//! ou arquivo diretamente. Duas implementações:
//!
//! * [`FileStateStore`] — snapshot JSON atômico (dev/smoke/testes).
//! * [`PostgresStateStore`] — o store de produção (ADR-0004): sqlx,
//!   migrations versionadas e cada operação crítica gravada
//!   **atomicamente** (uma transação por persistência — o estado nunca
//!   fica pela metade no banco, então um crash entre duas etapas não
//!   duplica item nem ouro).
//!
//! Nenhum `domain-*` conhece este módulo (ADR-0006): o teste de arquitetura
//! em `tests/architecture.rs` barra sqlx/tokio/bevy fora do server.
//!
//! Concorrência (ADR-0010): o Alpha tem **um** escritor (o servidor) e a
//! unidade atômica é o estado completo dentro de `BEGIN..COMMIT`. Row-level
//! `SELECT .. FOR UPDATE` entra quando existir mais de um processo escritor
//! disputando linhas — mecanismo equivalente, justificado aqui.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::ecs::prelude::Resource;
use mareforge_domain_economy::{LedgerKind, MarketOrder, Money, OrderStatus};
use mareforge_domain_items::{Custody, ItemInstance};
use mareforge_domain_ships::{ShipKind, VesselPresence};
use mareforge_shared::ids::{CharacterId, ShipInstanceId, WreckId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::market::MarketSnapshot;

/// Registro persistido de um navio (MF-035: o navio sobrevive à sessão).
/// `cargo` são as custódias `ShipCargo`; `equipped` as `Equipped` nos slots
/// (MF-039 — o loadout volta como estava, stats são recalculados no restore).
#[derive(Debug, Clone, PartialEq)]
pub struct ShipRecord {
    pub ship_instance: ShipInstanceId,
    pub character: CharacterId,
    pub kind: ShipKind,
    pub hp: u32,
    pub x: f32,
    pub y: f32,
    pub heading: f32,
    pub cargo: Vec<Custody>,
    pub equipped: Vec<Custody>,
    /// MF-049: presença no momento da persistência. Restaurada como está;
    /// se for `AtSea`, o restore normaliza `trip_started_at` para `now`
    /// (não tentamos reconstruir duração anterior — sem dado persistido
    /// para isso). `Docked` mantém `trip_started_at = None`.
    pub presence: VesselPresence,
}

/// Registro persistido de um wreck (MF-027 cont., PRD §67): apenas os
/// metadados do destroço. O conteúdo econômico do baú (`WreckChest`) já
/// vive em `item_instances` filtrado por `ItemLocation::Wreck`; esta
/// tabela guarda só o invólucro para que o wreck reapareça após restart
/// com killer, posição e janela corretos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WreckRecord {
    pub wreck_num: u32,
    pub wreck_id: WreckId,
    pub x: f32,
    pub y: f32,
    pub exclusive_looter: Option<CharacterId>,
    /// Segundos decorridos no momento do spawn, relativos ao boot do
    /// server. Permite recompor o tempo de vida restante após restart.
    pub spawned_at_secs: f64,
}

/// O contrato de persistência do servidor (MF-033). Síncrono de propósito:
/// o loop Bevy chama e espera; implementações bloqueantes (Postgres) rodam
/// em runtime próprio.
pub trait StateStore: Send + Sync {
    /// Estado econômico completo do boot. `None` = mundo novo.
    fn load_market(&self) -> Result<Option<MarketSnapshot>, String>;
    /// Persiste o estado econômico completo, atomicamente.
    fn save_market(&self, snapshot: &MarketSnapshot) -> Result<(), String>;
    /// Navio persistido de um personagem (restore pós-janela de graça).
    fn load_ship(&self, character: CharacterId) -> Result<Option<ShipRecord>, String>;
    /// Persiste o navio (e a carga embarcada) de um personagem.
    fn save_ship(&self, record: &ShipRecord) -> Result<(), String>;
    /// `true` = salvamento periódico aceitável (arquivo de dev);
    /// `false` = persistência por operação crítica (produção).
    fn periodic_saving(&self) -> bool;
    /// Snapshot completo de wrecks ativos. Vazio = nenhum wreck vivo.
    fn load_wreck_snapshot(&self) -> Result<Vec<WreckRecord>, String>;
    /// Persiste o snapshot de wrecks, substituindo o anterior atomicamente.
    fn save_wreck_snapshot(&self, wrecks: &[WreckRecord]) -> Result<(), String>;
    /// Remove um wreck específico (expiração pontual antes do próximo
    /// snapshot completo).
    fn delete_wreck(&self, wreck_num: u32) -> Result<(), String>;
}

/// Snapshot JSON em arquivo (`MAREFORGE_STATE_PATH`), escrita atômica via
/// tmp+rename. Escopo: dev, smoke e testes (MF-033) — NÃO é a persistência
/// de produção. Navios não são persistidos aqui: no modo dev o mundo nasce
/// limpo por sessão.
pub struct FileStateStore {
    path: PathBuf,
}

impl FileStateStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl StateStore for FileStateStore {
    fn load_market(&self) -> Result<Option<MarketSnapshot>, String> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| format!("snapshot ilegível: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn save_market(&self, snapshot: &MarketSnapshot) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(snapshot).map_err(|error| error.to_string())?;
        let mut temp = self.path.clone();
        temp.set_extension("tmp");
        // Escrita atômica via rename: crash no meio não corrompe o arquivo.
        std::fs::write(&temp, bytes)
            .and_then(|()| std::fs::rename(&temp, &self.path))
            .map_err(|error| error.to_string())
    }

    fn load_ship(&self, _character: CharacterId) -> Result<Option<ShipRecord>, String> {
        Ok(None)
    }

    fn save_ship(&self, _record: &ShipRecord) -> Result<(), String> {
        Ok(())
    }

    fn periodic_saving(&self) -> bool {
        true
    }

    fn load_wreck_snapshot(&self) -> Result<Vec<WreckRecord>, String> {
        let mut path = self.path.clone();
        path.set_extension("wrecks.json");
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("wreck snapshot ilegível: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn save_wreck_snapshot(&self, wrecks: &[WreckRecord]) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(wrecks).map_err(|error| error.to_string())?;
        let mut path = self.path.clone();
        path.set_extension("wrecks.json");
        let mut temp = path.clone();
        temp.set_extension("wrecks.tmp");
        // Escrita atômica via rename: o snapshot de wrecks é arquivo
        // separado do mercado; o crash entre as duas escritas deixa cada
        // arquivo consistente (rename é atômico por arquivo).
        std::fs::write(&temp, bytes)
            .and_then(|()| std::fs::rename(&temp, &path))
            .map_err(|error| error.to_string())
    }

    fn delete_wreck(&self, _wreck_num: u32) -> Result<(), String> {
        // No store de arquivo, a remoção individual é aplicada no próximo
        // save_wreck_snapshot completo. Mantemos a no-op para não quebrar
        // o contrato.
        Ok(())
    }
}

/// Store de produção (ADR-0004): PostgreSQL via sqlx. O runtime tokio vive
/// nesta struct; as chamadas são síncronas (block_on) — o servidor Alpha é
/// single-writer e o custo de uma escrita local é de milissegundos.
pub struct PostgresStateStore {
    runtime: tokio::runtime::Runtime,
    pool: sqlx::PgPool,
}

impl PostgresStateStore {
    /// Conecta e roda as migrations versionadas do workspace.
    pub fn connect(url: &str) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let pool = runtime
            .block_on(async {
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(2)
                    .connect(url)
                    .await
            })
            .map_err(|error| format!("conexão postgres falhou: {error}"))?;
        runtime
            .block_on(async { sqlx::migrate!("../../migrations").run(&pool).await })
            .map_err(|error| format!("migrations falharam: {error}"))?;
        Ok(Self { runtime, pool })
    }

    async fn insert_custody(
        tx: &mut sqlx::PgConnection,
        owner: CharacterId,
        custody: &Custody,
    ) -> Result<(), sqlx::Error> {
        let location =
            serde_json::to_value(custody.location).map_err(|error| sqlx::Error::ColumnDecode {
                index: "location".into(),
                source: Box::new(error),
            })?;
        sqlx::query(
            "INSERT INTO item_instances \
             (id, owner_character_id, definition_id, quantity, durability, location) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(custody.instance.id.0)
        .bind(owner.0)
        .bind(custody.instance.definition.0)
        .bind(custody.instance.quantity as i32)
        .bind(custody.instance.durability.map(|d| d as i16))
        .bind(location)
        .execute(&mut *tx)
        .await
        .map(|_| ())
    }

    async fn insert_order(
        tx: &mut sqlx::PgConnection,
        order: &MarketOrder,
        order_num: u32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO market_orders \
             (id, seller_character_id, item_definition_id, quantity, unit_price, \
              region_id, status, created_at, expires_at, order_num, filled_quantity) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (id) DO UPDATE SET \
             quantity = EXCLUDED.quantity, status = EXCLUDED.status, \
             filled_quantity = EXCLUDED.filled_quantity",
        )
        .bind(order.id.0)
        .bind(order.seller.0)
        .bind(order.item.0)
        .bind(order.quantity as i32)
        .bind(order.unit_price.0 as i64)
        .bind(order.region.0)
        .bind(format!("{:?}", order.status).to_lowercase())
        .bind(order.created_at)
        .bind(order.expires_at)
        .bind(order_num as i32)
        .bind(order.filled_quantity as i32)
        .execute(&mut *tx)
        .await
        .map(|_| ())
    }
}

impl StateStore for PostgresStateStore {
    fn load_market(&self) -> Result<Option<MarketSnapshot>, String> {
        self.runtime.block_on(async {
            let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;

            // Identidade persistente: token (name) → CharacterId.
            let identities: std::collections::HashMap<String, CharacterId> =
                sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM characters")
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(|(id, token)| (token, CharacterId(id)))
                    .collect();
            if identities.is_empty() {
                return Ok(None); // banco vazio = mundo novo
            }

            let balances: std::collections::HashMap<CharacterId, Money> =
                sqlx::query_as::<_, (Uuid, i64)>("SELECT character_id, gold FROM wallets")
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(|(id, gold)| (CharacterId(id), Money(gold.max(0) as u64)))
                    .collect();

            // Itens por localização: storage regional e escrow de orders.
            let item_rows =
                sqlx::query_as::<_, (Uuid, Uuid, Uuid, i32, Option<i16>, serde_json::Value)>(
                    "SELECT id, owner_character_id, definition_id, quantity, durability, location \
                 FROM item_instances WHERE location ? 'PortStorage' OR location ? 'MarketEscrow'",
                )
                .fetch_all(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;

            let mut storage: Vec<crate::market::StorageEntry> = Vec::new();
            let mut escrow: Vec<crate::market::EscrowEntry> = Vec::new();
            for (id, owner, definition, quantity, durability, location) in item_rows {
                let instance = ItemInstance {
                    id: mareforge_shared::ids::ItemInstanceId(id),
                    definition: mareforge_shared::ids::ItemDefinitionId(definition),
                    quantity: quantity.max(0) as u32,
                    durability: durability.map(|d| d.max(0) as u16),
                };
                let Ok(location) = serde_json::from_value(location) else {
                    return Err(format!("item {id} com location ilegível no banco"));
                };
                match location {
                    mareforge_domain_items::ItemLocation::PortStorage(region) => {
                        let custody = Custody { instance, location };
                        let owner_id = CharacterId(owner);
                        match storage
                            .iter_mut()
                            .find(|entry| entry.character == owner_id && entry.region == region)
                        {
                            Some(entry) => entry.stacks.push(custody),
                            None => storage.push(crate::market::StorageEntry {
                                character: owner_id,
                                region,
                                stacks: vec![custody],
                            }),
                        }
                    }
                    mareforge_domain_items::ItemLocation::MarketEscrow(order_id) => {
                        let order_num = escrow_order_num(&mut tx, order_id).await?;
                        let custody = Custody { instance, location };
                        match escrow.iter_mut().find(|entry| entry.order_num == order_num) {
                            Some(entry) => entry.stacks.push(custody),
                            None => escrow.push(crate::market::EscrowEntry {
                                order_num,
                                stacks: vec![custody],
                            }),
                        }
                    }
                    _ => {}
                }
            }

            let order_rows = sqlx::query_as::<
                _,
                (
                    Uuid,
                    Uuid,
                    Uuid,
                    i32,
                    i64,
                    Uuid,
                    String,
                    chrono::DateTime<chrono::Utc>,
                    chrono::DateTime<chrono::Utc>,
                    i32,
                    i32,
                ),
            >(
                "SELECT id, seller_character_id, item_definition_id, quantity, unit_price, \
                 region_id, status, created_at, expires_at, order_num, filled_quantity \
                 FROM market_orders",
            )
            .fetch_all(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;

            let mut board = Vec::new();
            let mut order_nums = std::collections::HashMap::new();
            let mut next_order_num = 0u32;
            for (
                id,
                seller,
                item,
                quantity,
                unit_price,
                region,
                status,
                created_at,
                expires_at,
                num,
                filled,
            ) in order_rows
            {
                let Ok(status) = status.parse::<StoredOrderStatus>() else {
                    return Err(format!("order {id} com status desconhecido"));
                };
                let status = status.0;
                board.push(MarketOrder {
                    id: mareforge_shared::ids::MarketOrderId(id),
                    seller: CharacterId(seller),
                    item: mareforge_shared::ids::ItemDefinitionId(item),
                    quantity: quantity.max(0) as u32,
                    unit_price: Money(unit_price.max(0) as u64),
                    region: mareforge_shared::ids::RegionId(region),
                    status,
                    created_at,
                    expires_at,
                    filled_quantity: filled.max(0) as u32,
                });
                order_nums.insert(num as u32, mareforge_shared::ids::MarketOrderId(id));
                next_order_num = next_order_num.max(num as u32 + 1);
            }

            // Ledger append-only: reconstruído na ordem do seq.
            let ledger_rows = sqlx::query_as::<_, (i64, String, i64, String)>(
                "SELECT seq, kind, delta_money, memo FROM ledger_entries \
                 WHERE seq IS NOT NULL ORDER BY seq",
            )
            .fetch_all(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
            let mut ledger = mareforge_domain_economy::Ledger::default();
            for (seq, kind, amount, memo) in ledger_rows {
                let Ok(kind) = kind.parse::<StoredLedgerKind>() else {
                    return Err(format!("ledger seq {seq} com kind desconhecido"));
                };
                ledger.record(kind.0, Money(amount.max(0) as u64), memo);
            }

            tx.commit().await.map_err(|error| error.to_string())?;
            Ok(Some(MarketSnapshot {
                identities,
                balances,
                storage,
                escrow,
                board,
                order_nums,
                next_order_num,
                ledger,
            }))
        })
    }

    fn save_market(&self, snapshot: &MarketSnapshot) -> Result<(), String> {
        self.runtime.block_on(async {
            // Uma transação por persistência (ADR-0010): ou o banco reflete
            // a operação inteira, ou não reflete nada — crash no meio não
            // duplica item nem ouro.
            let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;

            for (token, character) in &snapshot.identities {
                let email = format!("char-{}@local.dev", character.0.simple());
                sqlx::query(
                    "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, '') \
                     ON CONFLICT (id) DO NOTHING",
                )
                .bind(character.0)
                .bind(&email)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                sqlx::query(
                    "INSERT INTO characters (id, account_id, name, region_id, last_port_region_id) \
                     VALUES ($1, $1, $2, $3, $3) ON CONFLICT (id) DO UPDATE SET \
                     name = EXCLUDED.name, last_seen_at = now()",
                )
                .bind(character.0)
                .bind(token)
                .bind(Uuid::nil())
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                let gold = snapshot
                    .balances
                    .get(character)
                    .map(|money| money.0 as i64)
                    .unwrap_or(0);
                sqlx::query(
                    "INSERT INTO wallets (character_id, gold) VALUES ($1, $2) \
                     ON CONFLICT (character_id) DO UPDATE SET gold = EXCLUDED.gold",
                )
                .bind(character.0)
                .bind(gold)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
            }

            // Estado mutável é substituído inteiro dentro da transação;
            // storage/escrow/orders nunca ficam pela metade.
            sqlx::query("DELETE FROM item_instances")
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
            sqlx::query("DELETE FROM market_orders")
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;

            for entry in &snapshot.storage {
                for custody in &entry.stacks {
                    Self::insert_custody(&mut tx, entry.character, custody)
                        .await
                        .map_err(|error| error.to_string())?;
                }
            }
            for entry in &snapshot.escrow {
                // O dono das custódias em escrow é o vendedor da order; sem
                // order no board, o escrow é órfão e não entra no banco.
                let Some(order_id) = snapshot.order_nums.get(&entry.order_num) else {
                    continue;
                };
                let Some(order) = snapshot.board.iter().find(|order| &order.id == order_id) else {
                    continue;
                };
                for custody in &entry.stacks {
                    Self::insert_custody(&mut tx, order.seller, custody)
                        .await
                        .map_err(|error| error.to_string())?;
                }
            }
            for order in &snapshot.board {
                let order_num = snapshot
                    .order_nums
                    .iter()
                    .find(|(_, id)| **id == order.id)
                    .map(|(num, _)| *num)
                    .unwrap_or(0);
                Self::insert_order(&mut tx, order, order_num)
                    .await
                    .map_err(|error| error.to_string())?;
            }

            // Ledger: append-only no banco também — regravar a mesma entrada
            // (mesmo seq) é no-op; só entradas novas entram.
            for entry in snapshot.ledger.entries() {
                let kind = format!("{:?}", entry.kind).to_lowercase();
                sqlx::query(
                    "INSERT INTO ledger_entries \
                     (id, transaction_id, character_id, delta_money, kind, seq, memo) \
                     VALUES ($1, $2, NULL, $3, $4, $5, $6) ON CONFLICT (seq) DO NOTHING",
                )
                .bind(Uuid::new_v4())
                .bind(Uuid::nil())
                .bind(entry.amount.0 as i64)
                .bind(kind)
                .bind(entry.seq as i64)
                .bind(&entry.memo)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
            }

            tx.commit().await.map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn load_ship(&self, character: CharacterId) -> Result<Option<ShipRecord>, String> {
        self.runtime.block_on(async {
            let Some((id, kind, hp, x, y, heading, presence)) =
                sqlx::query_as::<_, (Uuid, String, i32, f64, f64, f64, String)>(
                    "SELECT id, ship_kind, current_hp, position_x, position_y, heading, presence \
                 FROM ship_instances WHERE character_id = $1 LIMIT 1",
                )
                .bind(character.0)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| error.to_string())?
            else {
                return Ok(None);
            };
            let Ok(kind) = kind.parse::<StoredShipKind>() else {
                return Err(format!("navio {id} com ship_kind desconhecido"));
            };
            let presence: VesselPresence = serde_json::from_str(&presence)
                .map_err(|error| format!("navio {id} com presence ilegível: {error}"))?;

            let rows = sqlx::query_as::<_, (Uuid, Uuid, i32, Option<i16>, serde_json::Value)>(
                "SELECT id, definition_id, quantity, durability, location FROM item_instances \
                 WHERE location ->> 'ShipCargo' = $2 \
                 OR location -> 'Equipped' ->> 'ship' = $2",
            )
            .bind(character.0)
            .bind(id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|error| error.to_string())?;

            let mut cargo = Vec::new();
            let mut equipped = Vec::new();
            for (item_id, definition, quantity, durability, location) in rows {
                let Ok(location) = serde_json::from_value(location) else {
                    return Err(format!("item {item_id} com location ilegível"));
                };
                let custody = Custody {
                    instance: ItemInstance {
                        id: mareforge_shared::ids::ItemInstanceId(item_id),
                        definition: mareforge_shared::ids::ItemDefinitionId(definition),
                        quantity: quantity.max(0) as u32,
                        durability: durability.map(|d| d.max(0) as u16),
                    },
                    location,
                };
                if matches!(
                    custody.location,
                    mareforge_domain_items::ItemLocation::Equipped { .. }
                ) {
                    equipped.push(custody);
                } else {
                    cargo.push(custody);
                }
            }

            Ok(Some(ShipRecord {
                ship_instance: ShipInstanceId(id),
                character,
                kind: kind.0,
                hp: hp.max(0) as u32,
                x: x as f32,
                y: y as f32,
                heading: heading as f32,
                cargo,
                equipped,
                presence,
            }))
        })
    }

    fn save_ship(&self, record: &ShipRecord) -> Result<(), String> {
        self.runtime.block_on(async {
            let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;
            let presence = serde_json::to_string(&record.presence)
                .map_err(|error| format!("presence ilegível: {error}"))?;
            sqlx::query(
                "INSERT INTO ship_instances \
                 (id, character_id, definition_id, ship_kind, equipped_components, \
                  current_hp, current_region_id, position_x, position_y, heading, \
                  presence) \
                 VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, $3, $6, $7, $8, $9) \
                 ON CONFLICT (id) DO UPDATE SET current_hp = EXCLUDED.current_hp, \
                 position_x = EXCLUDED.position_x, position_y = EXCLUDED.position_y, \
                 heading = EXCLUDED.heading, presence = EXCLUDED.presence, \
                 updated_at = now()",
            )
            .bind(record.ship_instance.0)
            .bind(record.character.0)
            .bind(Uuid::nil())
            .bind(format!("{:?}", record.kind))
            .bind(record.hp as i32)
            .bind(record.x as f64)
            .bind(record.y as f64)
            .bind(record.heading as f64)
            .bind(presence)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;

            // A carga embarcada substitui inteira (itens do navio somem e
            // renascem do estado atual — dentro da mesma transação).
            sqlx::query("DELETE FROM item_instances WHERE location ->> 'ShipCargo' = $1")
                .bind(record.ship_instance.0.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
            for custody in &record.cargo {
                Self::insert_custody(&mut tx, record.character, custody)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            tx.commit().await.map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn periodic_saving(&self) -> bool {
        false
    }

    fn load_wreck_snapshot(&self) -> Result<Vec<WreckRecord>, String> {
        self.runtime.block_on(async {
            let rows = sqlx::query_as::<
                _,
                (i32, Uuid, f64, f64, Option<Uuid>, f64),
            >(
                "SELECT wreck_num, wreck_id, position_x, position_y,                  exclusive_looter, spawned_at_secs FROM wrecks",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|error| error.to_string())?;

            rows.into_iter()
                .map(|(num, id, x, y, looter, spawned)| {
                    Ok(WreckRecord {
                        wreck_num: num.max(0) as u32,
                        wreck_id: WreckId(id),
                        x: x as f32,
                        y: y as f32,
                        exclusive_looter: looter.map(CharacterId),
                        spawned_at_secs: spawned.max(0.0),
                    })
                })
                .collect()
        })
    }

    fn save_wreck_snapshot(&self, wrecks: &[WreckRecord]) -> Result<(), String> {
        self.runtime.block_on(async {
            // Mesma semântica do save_market: DELETE+INSERT dentro de uma
            // transação. O snapshot é pequeno (uma linha por wreck ativo).
            let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;
            sqlx::query("DELETE FROM wrecks")
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
            for wreck in wrecks {
                sqlx::query(
                    "INSERT INTO wrecks                      (wreck_num, wreck_id, position_x, position_y,                       exclusive_looter, spawned_at_secs)                      VALUES ($1, $2, $3, $4, $5, $6)                      ON CONFLICT (wreck_num) DO UPDATE SET                      wreck_id = EXCLUDED.wreck_id,                      position_x = EXCLUDED.position_x,                      position_y = EXCLUDED.position_y,                      exclusive_looter = EXCLUDED.exclusive_looter,                      spawned_at_secs = EXCLUDED.spawned_at_secs",
                )
                .bind(wreck.wreck_num as i32)
                .bind(wreck.wreck_id.0)
                .bind(wreck.x as f64)
                .bind(wreck.y as f64)
                .bind(wreck.exclusive_looter.map(|c| c.0))
                .bind(wreck.spawned_at_secs)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
            }
            tx.commit().await.map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn delete_wreck(&self, wreck_num: u32) -> Result<(), String> {
        self.runtime.block_on(async {
            sqlx::query("DELETE FROM wrecks WHERE wreck_num = $1")
                .bind(wreck_num as i32)
                .execute(&self.pool)
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }
}

/// Wrapper para parse do kind de ledger armazenado ("mint"/"burn"/"trade").
struct StoredLedgerKind(LedgerKind);

impl std::str::FromStr for StoredLedgerKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mint" => Ok(Self(LedgerKind::Mint)),
            "burn" => Ok(Self(LedgerKind::Burn)),
            "trade" => Ok(Self(LedgerKind::Trade)),
            "npcbounty" => Ok(Self(LedgerKind::NpcBounty)),
            _ => Err(()),
        }
    }
}

/// Wrapper para parse do ShipKind armazenado ("SmallMerchant", ...).
struct StoredShipKind(ShipKind);

impl std::str::FromStr for StoredShipKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        <ShipKind as serde::Deserialize>::deserialize(serde::de::value::StrDeserializer::<
            serde::de::value::Error,
        >::new(s))
        .map(Self)
        .map_err(|_| ())
    }
}

/// Wrapper para parse do status armazenado ("open", "partial", ...).
struct StoredOrderStatus(OrderStatus);

impl std::str::FromStr for StoredOrderStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Self(OrderStatus::Open)),
            "partial" => Ok(Self(OrderStatus::Partial)),
            "filled" => Ok(Self(OrderStatus::Filled)),
            "cancelled" => Ok(Self(OrderStatus::Cancelled)),
            "expired" => Ok(Self(OrderStatus::Expired)),
            _ => Err(()),
        }
    }
}

/// Número de protocolo de uma order em escrow (u32::MAX se a order não está
/// mais no board — os itens órfãos não agrupam com nenhuma escrow ativa).
async fn escrow_order_num(
    tx: &mut sqlx::PgConnection,
    order_id: mareforge_shared::ids::MarketOrderId,
) -> Result<u32, String> {
    let row: Option<(i32,)> = sqlx::query_as("SELECT order_num FROM market_orders WHERE id = $1")
        .bind(order_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    Ok(row.map(|(num,)| num as u32).unwrap_or(u32::MAX))
}

/// Resource Bevy com o store ativo da sessão (`None` = dev puro, sem
/// persistência — mundo descartável).
#[derive(Resource, Clone, Default)]
pub struct StoreHandle(pub Option<Arc<dyn StateStore>>);

impl StoreHandle {
    /// Persiste o mercado, logando falha (não entra em pânico: a memória é
    /// a verdade da sessão; o store é a âncora de sobrevivência).
    pub fn save_market_quiet(&self, snapshot: &MarketSnapshot) {
        if let Some(store) = &self.0 {
            if let Err(error) = store.save_market(snapshot) {
                tracing::warn!(error = %error, "falha ao persistir estado econômico");
            }
        }
    }

    /// Persiste o snapshot de wrecks ativos, logando falha (mesma
    /// filosofia do save_market_quiet: a memória é a verdade da sessão,
    /// o store é a âncora).
    pub fn save_wreck_quiet(&self, wrecks: &[WreckRecord]) {
        if let Some(store) = &self.0 {
            if let Err(error) = store.save_wreck_snapshot(wrecks) {
                tracing::warn!(error = %error, "falha ao persistir wrecks");
            }
        }
    }
}

/// Fábrica a partir do ambiente (MF-033): Postgres de produção, arquivo de
/// dev, ou nada. Banco configurado e inacessível é erro de boot (fail-closed,
/// §69) — silenciosamente degradar persistência é como não tê-la.
pub fn store_from_env() -> StoreHandle {
    if let Ok(url) = std::env::var("MAREFORGE_DATABASE_URL") {
        match PostgresStateStore::connect(&url) {
            Ok(store) => {
                tracing::info!("persistência: PostgreSQL (ADR-0004)");
                return StoreHandle(Some(Arc::new(store)));
            }
            Err(error) => {
                panic!("MAREFORGE_DATABASE_URL configurado mas o banco não abriu: {error}");
            }
        }
    }
    if let Some(path) = std::env::var_os("MAREFORGE_STATE_PATH") {
        tracing::info!(
            path = %path.to_string_lossy(),
            "persistência: arquivo de dev (não é produção)"
        );
        return StoreHandle(Some(Arc::new(FileStateStore::new(PathBuf::from(path)))));
    }
    tracing::info!("persistência: nenhuma (dev puro, mundo descartável)");
    StoreHandle(None)
}
