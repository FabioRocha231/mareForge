//! Guilda Mercante e Quadro de Contratos no client: abas "Guilda" e
//! "Contratos" da tela de porto (corpo montado aqui) e a linha do contrato
//! ativo no HUD do mar, logo abaixo do painel do navio. Tudo é leitura dos
//! snapshots do servidor; os botões só emitem intents (Pilar 4).

use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use marvyr_protocol::{
    AbandonContract, AcceptContract, ContractLine, ContractResult, ContractsSnapshot, GuildPrices,
    SellToGuild, StorageDepositAll, StorageLine, Undock,
};
use marvyr_shared::ids::ItemDefinitionId;

use crate::hud::SeaHud;
use crate::net::ReliableChannel;
use crate::ui;

#[derive(Resource, Debug, Default)]
pub struct KnownGuildPrices(pub Option<GuildPrices>);

/// Último snapshot de contratos + instante de chegada (contagem local).
#[derive(Resource, Debug, Default)]
pub struct KnownContracts {
    pub snapshot: Option<ContractsSnapshot>,
    pub received_at: f32,
}

#[derive(Resource, Debug, Default)]
pub struct ContractFeedback(pub Option<ContractResult>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildButton {
    Sell(ItemDefinitionId, u32),
    Accept(u32),
    Abandon,
}

#[derive(Component)]
struct ContractHud;

#[derive(Component)]
struct ContractHudText;

pub struct GuildPlugin;

impl Plugin for GuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KnownGuildPrices>()
            .init_resource::<KnownContracts>()
            .init_resource::<ContractFeedback>()
            .add_systems(Startup, spawn_contract_hud)
            .add_systems(
                Update,
                (
                    handle_guild_messages,
                    handle_guild_clicks,
                    update_contract_hud,
                    auto_guild,
                ),
            );
    }
}

fn handle_guild_messages(
    time: Res<Time>,
    mut prices: EventReader<ClientReceiveMessage<GuildPrices>>,
    mut contracts: EventReader<ClientReceiveMessage<ContractsSnapshot>>,
    mut results: EventReader<ClientReceiveMessage<ContractResult>>,
    mut known_prices: ResMut<KnownGuildPrices>,
    mut known_contracts: ResMut<KnownContracts>,
    mut feedback: ResMut<ContractFeedback>,
) {
    for event in prices.read() {
        known_prices.0 = Some(event.message().clone());
    }
    for event in contracts.read() {
        known_contracts.snapshot = Some(event.message().clone());
        known_contracts.received_at = time.elapsed_secs();
    }
    for event in results.read() {
        let result = event.message().clone();
        info!(success = result.success, reason = %result.reason, "contrato");
        feedback.0 = Some(result);
    }
}

fn handle_guild_clicks(
    buttons: Query<(&Interaction, &GuildButton), Changed<Interaction>>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let _ = match *button {
            GuildButton::Sell(item, quantity) => connection_manager
                .send_message::<ReliableChannel, _>(&SellToGuild { item, quantity }),
            GuildButton::Accept(id) => {
                connection_manager.send_message::<ReliableChannel, _>(&AcceptContract { id })
            }
            GuildButton::Abandon => {
                connection_manager.send_message::<ReliableChannel, _>(&AbandonContract)
            }
        };
    }
}

/// Dev (§39): MARVYR_AUTOGUILD=1, atracado, deposita o porão, vende
/// tudo à guilda, aceita a primeira oferta e desatraca — smoke do loop.
fn auto_guild(
    time: Res<Time>,
    docked: Res<crate::net::MyDocked>,
    storage: Res<crate::port_screen::KnownPortStorage>,
    contracts: Res<KnownContracts>,
    mut timer: Local<f32>,
    mut step: Local<u8>,
    mut connection_manager: ResMut<ConnectionManager>,
) {
    if std::env::var_os("MARVYR_AUTOGUILD").is_none() || !docked.0 {
        return;
    }
    *timer += time.delta_secs();
    if *timer < 1.5 {
        return;
    }
    *timer = 0.0;
    let _ = match *step {
        0 => connection_manager.send_message::<ReliableChannel, _>(&StorageDepositAll),
        1 => {
            for line in &storage.0 {
                let _ = connection_manager.send_message::<ReliableChannel, _>(&SellToGuild {
                    item: line.item,
                    quantity: line.quantity,
                });
            }
            Ok(())
        }
        2 => match contracts.snapshot.as_ref().and_then(|s| s.offers.first()) {
            Some(offer) => connection_manager
                .send_message::<ReliableChannel, _>(&AcceptContract { id: offer.id }),
            None => return,
        },
        3 => connection_manager.send_message::<ReliableChannel, _>(&Undock),
        _ => return,
    };
    *step += 1;
}

/// Contrato ativo com o tempo restante descontado localmente.
pub fn active_contract(known: &KnownContracts, now: f32) -> Option<ContractLine> {
    let mut line = known.snapshot.as_ref()?.active.clone()?;
    let elapsed = (now - known.received_at).max(0.0) as u32;
    line.remaining_secs = line.remaining_secs.saturating_sub(elapsed);
    Some(line)
}

/// A fonte padrão só desenha ASCII: tira acentos dos nomes vindos do
/// servidor ("Minério" → "Minerio").
pub fn ascii(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'Á' | 'À' | 'Â' | 'Ã' => 'A',
            'é' | 'ê' => 'e',
            'É' | 'Ê' => 'E',
            'í' => 'i',
            'Í' => 'I',
            'ó' | 'ô' | 'õ' => 'o',
            'Ó' | 'Ô' | 'Õ' => 'O',
            'ú' => 'u',
            'Ú' => 'U',
            'ç' => 'c',
            'Ç' => 'C',
            c => c,
        })
        .collect()
}

fn clock(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn progress_label(line: &ContractLine) -> String {
    if line.hunt {
        format!("abates {}/{}", line.progress, line.target)
    } else {
        String::from("entregue ao atracar no destino")
    }
}

// ===== Aba Guilda =====

#[derive(Debug, Clone, PartialEq)]
pub struct GuildRow {
    pub item: ItemDefinitionId,
    pub name: String,
    pub stored: u32,
    pub in_cargo: u32,
    pub here: u64,
    pub other: u64,
    /// Diferença percentual do outro porto sobre este.
    pub delta_pct: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuildView {
    pub other_port: String,
    pub rows: Vec<GuildRow>,
}

fn quantity_of(lines: &[StorageLine], item: ItemDefinitionId) -> u32 {
    lines
        .iter()
        .filter(|line| line.item == item)
        .map(|line| line.quantity)
        .sum()
}

/// Uma linha por item comprável: preço aqui, no outro porto e o delta.
pub fn guild_view(prices: Option<&GuildPrices>, storage: &[StorageLine], here: &str) -> GuildView {
    let Some(prices) = prices else {
        return GuildView {
            other_port: String::new(),
            rows: Vec::new(),
        };
    };
    let here_index = prices
        .ports
        .iter()
        .position(|port| port == here)
        .unwrap_or(0);
    let other_index = (0..prices.ports.len()).find(|index| *index != here_index);
    GuildView {
        other_port: other_index
            .map(|index| ascii(&prices.ports[index]))
            .unwrap_or_default(),
        rows: prices
            .lines
            .iter()
            .map(|line| {
                let here = line.prices.get(here_index).copied().unwrap_or(0);
                let other = other_index
                    .and_then(|index| line.prices.get(index).copied())
                    .unwrap_or(here);
                GuildRow {
                    item: line.item,
                    name: ascii(&line.item_name),
                    stored: quantity_of(storage, line.item),
                    in_cargo: quantity_of(&prices.cargo, line.item),
                    here,
                    other,
                    delta_pct: if here == 0 {
                        0
                    } else {
                        ((other as f64 - here as f64) * 100.0 / here as f64).round() as i64
                    },
                }
            })
            .collect(),
    }
}

fn delta_color(delta: i64) -> Color {
    match delta {
        d if d > 0 => ui::OK_GREEN,
        d if d < 0 => ui::DANGER,
        _ => ui::TEXT_DIM,
    }
}

fn cell(parent: &mut ChildBuilder, value: impl Into<String>, width: f32, color: Color) {
    parent.spawn((
        ui::text(value, 13.0, color),
        Node {
            width: Val::Px(width),
            ..default()
        },
    ));
}

fn guild_button(parent: &mut ChildBuilder, label: &str, action: GuildButton, color: Color) {
    parent
        .spawn((ui::button(Node::default(), ui::BUTTON_BG), action))
        .with_children(|b| {
            b.spawn(ui::text(label, 13.0, color));
        });
}

pub fn spawn_guild_body(parent: &mut ChildBuilder, view: &GuildView) {
    parent.spawn(ui::text(
        "GUILDA MERCANTE - compra do armazem deste porto (item vendido e destruido)",
        12.0,
        ui::PANEL_BORDER,
    ));
    if view.rows.is_empty() {
        parent.spawn(ui::text(
            "Aguardando precos da guilda...",
            13.0,
            ui::TEXT_DIM,
        ));
        return;
    }
    parent
        .spawn(Node {
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|header| {
            cell(header, "Item", 150.0, ui::TEXT_DIM);
            cell(header, "Armazem", 70.0, ui::TEXT_DIM);
            cell(header, "Porao", 60.0, ui::TEXT_DIM);
            cell(header, "Preco aqui", 80.0, ui::TEXT_DIM);
            cell(header, view.other_port.as_str(), 150.0, ui::TEXT_DIM);
        });
    for row in &view.rows {
        parent
            .spawn(Node {
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|line| {
                cell(line, row.name.as_str(), 150.0, ui::TEXT);
                cell(line, row.stored.to_string(), 70.0, ui::TEXT);
                cell(line, row.in_cargo.to_string(), 60.0, ui::TEXT_DIM);
                cell(line, format!("{}g", row.here), 80.0, ui::GOLD);
                cell(
                    line,
                    format!("{}g ({:+}%)", row.other, row.delta_pct),
                    150.0,
                    delta_color(row.delta_pct),
                );
                if row.stored > 0 {
                    guild_button(line, "Vender 1", GuildButton::Sell(row.item, 1), ui::GOLD);
                    guild_button(line, "10", GuildButton::Sell(row.item, 10), ui::GOLD);
                    guild_button(
                        line,
                        "Tudo",
                        GuildButton::Sell(row.item, row.stored),
                        ui::GOLD,
                    );
                }
            });
    }
    parent.spawn(ui::text(
        "Vender muito derruba o preco; ele se recupera com o tempo. Deposite o porao para vender.",
        11.0,
        ui::TEXT_DIM,
    ));
}

// ===== Aba Contratos =====

#[derive(Debug, Clone, PartialEq)]
pub struct ContractsView {
    pub offers: Vec<ContractLine>,
    pub active: Option<ContractLine>,
}

pub fn contracts_view(known: &KnownContracts, now: f32) -> ContractsView {
    ContractsView {
        offers: known
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.offers.clone())
            .unwrap_or_default(),
        active: active_contract(known, now),
    }
}

pub fn spawn_contracts_body(parent: &mut ChildBuilder, view: &ContractsView) {
    parent.spawn(ui::text("SEU CONTRATO", 12.0, ui::PANEL_BORDER));
    match &view.active {
        Some(active) => {
            parent
                .spawn(Node {
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|line| {
                    line.spawn(ui::text(ascii(&active.title), 14.0, ui::TEXT));
                    line.spawn(ui::text(format!("{}g", active.reward), 14.0, ui::GOLD));
                    line.spawn(ui::text(
                        format!(
                            "{} restantes - {}",
                            clock(active.remaining_secs),
                            progress_label(active)
                        ),
                        13.0,
                        ui::AMBER,
                    ));
                    guild_button(line, "Abandonar", GuildButton::Abandon, ui::DANGER);
                });
        }
        None => {
            parent.spawn(ui::text("Nenhum contrato ativo.", 13.0, ui::TEXT_DIM));
        }
    }
    parent.spawn((
        ui::text("QUADRO DE CONTRATOS", 12.0, ui::PANEL_BORDER),
        Node {
            margin: UiRect::top(Val::Px(10.0)),
            ..default()
        },
    ));
    if view.offers.is_empty() {
        parent.spawn(ui::text(
            "Quadro vazio - volte mais tarde.",
            13.0,
            ui::TEXT_DIM,
        ));
    }
    for offer in &view.offers {
        parent
            .spawn(Node {
                column_gap: Val::Px(10.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|line| {
                line.spawn((
                    ui::text(ascii(&offer.title), 14.0, ui::TEXT),
                    Node {
                        width: Val::Px(470.0),
                        ..default()
                    },
                ));
                cell(line, format!("{}g", offer.reward), 70.0, ui::GOLD);
                cell(
                    line,
                    format!("{} min", offer.duration_secs / 60),
                    60.0,
                    ui::TEXT_DIM,
                );
                if view.active.is_none() {
                    guild_button(line, "Aceitar", GuildButton::Accept(offer.id), ui::OK_GREEN);
                }
            });
    }
    parent.spawn(ui::text(
        "1 contrato por vez. Entrega: a carga vai no porao e pode ser saqueada no caminho.",
        11.0,
        ui::TEXT_DIM,
    ));
}

// ===== HUD do mar =====

/// Linha do contrato ativo sob o painel do navio (topo-esquerda).
fn spawn_contract_hud(mut commands: Commands) {
    commands
        .spawn((
            ui::panel(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(ui::MARGIN),
                top: Val::Px(ui::MARGIN + 132.0),
                display: Display::None,
                ..default()
            }),
            SeaHud,
            ContractHud,
        ))
        .with_children(|panel| {
            panel.spawn((ui::text("", 12.0, ui::AMBER), ContractHudText));
        });
}

fn update_contract_hud(
    time: Res<Time>,
    known: Res<KnownContracts>,
    mut huds: Query<&mut Node, With<ContractHud>>,
    mut texts: Query<&mut Text, With<ContractHudText>>,
) {
    let active = active_contract(&known, time.elapsed_secs());
    let display = if active.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut huds {
        if node.display != display {
            node.display = display;
        }
    }
    let Some(active) = active else {
        return;
    };
    let value = format!(
        "CONTRATO: {}\n{} - {} - {}g",
        ascii(&active.title),
        clock(active.remaining_secs),
        progress_label(&active),
        active.reward
    );
    for mut text in &mut texts {
        if text.0 != value {
            text.0 = value.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use marvyr_protocol::GuildPriceLine;

    use super::*;

    #[test]
    fn guild_view_shows_arbitrage_against_the_other_port() {
        let wood = ItemDefinitionId::new();
        let prices = GuildPrices {
            ports: vec![
                String::from("Porto da Serra"),
                String::from("Porto da Mina"),
            ],
            lines: vec![GuildPriceLine {
                item: wood,
                item_name: String::from("Madeira"),
                prices: vec![6, 16],
            }],
            cargo: vec![StorageLine {
                item: wood,
                item_name: String::from("Madeira"),
                quantity: 3,
            }],
        };
        let storage = vec![StorageLine {
            item: wood,
            item_name: String::from("Madeira"),
            quantity: 40,
        }];

        let view = guild_view(Some(&prices), &storage, "Porto da Serra");
        assert_eq!(view.other_port, "Porto da Mina");
        let row = &view.rows[0];
        assert_eq!((row.here, row.other, row.delta_pct), (6, 16, 167));
        assert_eq!((row.stored, row.in_cargo), (40, 3));

        let view = guild_view(Some(&prices), &storage, "Porto da Mina");
        assert_eq!(view.rows[0].delta_pct, -63);
    }

    #[test]
    fn active_contract_counts_down_locally() {
        let known = KnownContracts {
            snapshot: Some(ContractsSnapshot {
                offers: Vec::new(),
                active: Some(ContractLine {
                    id: 1,
                    title: String::from("Caca"),
                    reward: 300,
                    duration_secs: 600,
                    remaining_secs: 100,
                    progress: 1,
                    target: 2,
                    hunt: true,
                }),
            }),
            received_at: 10.0,
        };
        assert_eq!(active_contract(&known, 40.0).unwrap().remaining_secs, 70);
        assert_eq!(active_contract(&known, 500.0).unwrap().remaining_secs, 0);
        assert_eq!(clock(70), "1:10");
        assert_eq!(
            ascii("Canhão de Bronze, Minério"),
            "Canhao de Bronze, Minerio"
        );
    }
}
