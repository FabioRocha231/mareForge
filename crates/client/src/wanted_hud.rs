//! Feed de eventos e selo de procurado (MF-059), bevy_ui em espaço de tela.
//! O servidor decide tudo (notoriedade, faixa, recompensa); aqui só se
//! mostra: `WorldEvent` vira uma linha no feed da direita que some em ~6 s,
//! `ReputationUpdate` vira o selo sob o painel do navio.

use bevy::prelude::*;
use lightyear::prelude::ClientReceiveMessage;
use mareforge_protocol::{
    DockResult, ReputationUpdate, WorldEvent, WorldEventKind, TIER_PROCURADO, TIER_SUSPEITO,
};

use crate::hud::{spawn_faded_panel, SeaHud};
use crate::ui;

/// Linhas visíveis no feed; a mais antiga sai quando chega uma nova.
const FEED_MAX: usize = 5;
/// Fade in, fim do hold, fim do fade out (s).
const FEED_FADE: (f32, f32, f32) = (0.3, 5.2, 6.0);
/// Logo abaixo do painel do navio (topo-esquerda).
const BADGE_TOP: f32 = 128.0;

/// Última ficha do PRÓPRIO capitão vinda do servidor.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct MyReputation(pub Option<ReputationUpdate>);

#[derive(Component)]
pub struct KillFeed;

#[derive(Component)]
pub struct FeedEntry;

#[derive(Component)]
pub struct WantedBadge;

#[derive(Component)]
pub struct WantedBadgeText;

pub struct WantedHudPlugin;

impl Plugin for WantedHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MyReputation>()
            .add_systems(Startup, setup_wanted_hud)
            .add_systems(
                Update,
                (
                    receive_reputation,
                    receive_world_events,
                    feed_dock_refusals,
                    update_wanted_badge,
                ),
            );
    }
}

pub fn setup_wanted_hud(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(ui::MARGIN),
            top: Val::Px(ui::MARGIN),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::End,
            row_gap: Val::Px(4.0),
            ..default()
        },
        SeaHud,
        KillFeed,
    ));
    commands
        .spawn((
            ui::panel(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(ui::MARGIN),
                top: Val::Px(BADGE_TOP),
                display: Display::None,
                ..default()
            }),
            SeaHud,
            WantedBadge,
        ))
        .with_children(|badge| {
            badge.spawn((ui::text("", 13.0, ui::DANGER), WantedBadgeText));
        });
}

/// Texto e cor do selo; `None` = Honrado (sem selo).
pub fn badge_line(reputation: &ReputationUpdate) -> Option<(String, Color)> {
    match reputation.tier {
        TIER_PROCURADO => Some((
            format!("PROCURADO - cabeca: {}g", reputation.bounty),
            ui::DANGER,
        )),
        TIER_SUSPEITO => Some((
            format!("SUSPEITO - notoriedade {}", reputation.notoriety),
            ui::AMBER,
        )),
        _ => None,
    }
}

fn event_color(kind: WorldEventKind) -> Color {
    match kind {
        WorldEventKind::Kill => ui::GOLD,
        WorldEventKind::Alert => ui::AMBER,
        WorldEventKind::Bounty => ui::DANGER,
    }
}

fn receive_reputation(
    mut events: EventReader<ClientReceiveMessage<ReputationUpdate>>,
    mut mine: ResMut<MyReputation>,
) {
    if let Some(event) = events.read().last() {
        mine.0 = Some(*event.message());
    }
}

fn push_feed_line(
    commands: &mut Commands,
    feed: Entity,
    entries: &[Entity],
    text: &str,
    color: Color,
) {
    // Cheio: a linha mais antiga (primeiro filho) sai.
    if entries.len() >= FEED_MAX {
        commands.entity(entries[0]).despawn_recursive();
    }
    spawn_faded_panel(
        commands,
        feed,
        FEED_FADE,
        color,
        FeedEntry,
        &[(text, 13.0, color)],
    );
}

fn feed_entries(
    feed: &Query<(Entity, Option<&Children>), With<KillFeed>>,
) -> Option<(Entity, Vec<Entity>)> {
    let (entity, children) = feed.get_single().ok()?;
    Some((
        entity,
        children
            .map(|c| c.iter().copied().collect())
            .unwrap_or_default(),
    ))
}

fn receive_world_events(
    mut commands: Commands,
    mut events: EventReader<ClientReceiveMessage<WorldEvent>>,
    feed: Query<(Entity, Option<&Children>), With<KillFeed>>,
) {
    let Some((feed, mut entries)) = feed_entries(&feed) else {
        return;
    };
    for event in events.read() {
        let message = event.message();
        push_feed_line(
            &mut commands,
            feed,
            &entries,
            &message.text,
            event_color(message.kind),
        );
        if entries.len() >= FEED_MAX {
            entries.remove(0);
        }
    }
}

/// Porto da coroa recusou o Procurado: o motivo vai para o feed.
fn feed_dock_refusals(
    mut commands: Commands,
    mut events: EventReader<ClientReceiveMessage<DockResult>>,
    feed: Query<(Entity, Option<&Children>), With<KillFeed>>,
) {
    let Some((feed, entries)) = feed_entries(&feed) else {
        return;
    };
    if let Some(result) = events
        .read()
        .map(|event| event.message())
        .filter(|result| !result.success && result.reason.starts_with("Procurado"))
        .last()
    {
        push_feed_line(&mut commands, feed, &entries, &result.reason, ui::DANGER);
    }
}

fn update_wanted_badge(
    mine: Res<MyReputation>,
    mut badge: Query<(&mut Node, &mut BorderColor), With<WantedBadge>>,
    mut text: Query<(&mut Text, &mut TextColor), With<WantedBadgeText>>,
) {
    if !mine.is_changed() {
        return;
    }
    let line = mine.0.as_ref().and_then(badge_line);
    for (mut node, mut border) in &mut badge {
        node.display = if line.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if let Some((_, color)) = &line {
            border.0 = *color;
        }
    }
    if let Some((value, color)) = line {
        for (mut text, mut text_color) in &mut text {
            text.0 = value.clone();
            text_color.0 = color;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::schedule::Schedule;

    use super::*;

    fn rep(notoriety: u32, tier: u8, bounty: u64) -> ReputationUpdate {
        ReputationUpdate {
            notoriety,
            tier,
            bounty,
        }
    }

    #[test]
    fn badge_shows_only_for_suspects_and_wanted() {
        assert!(badge_line(&rep(40, 0, 0)).is_none());
        let (text, _) = badge_line(&rep(150, TIER_SUSPEITO, 0)).unwrap();
        assert_eq!(text, "SUSPEITO - notoriedade 150");
        let (text, color) = badge_line(&rep(320, TIER_PROCURADO, 640)).unwrap();
        assert_eq!(text, "PROCURADO - cabeca: 640g");
        assert_eq!(color, ui::DANGER);
        assert!(text.is_ascii());
    }

    #[test]
    fn badge_node_follows_my_reputation() {
        let mut world = World::new();
        world.init_resource::<MyReputation>();
        let mut setup = Schedule::default();
        setup.add_systems(setup_wanted_hud);
        setup.run(&mut world);

        let mut schedule = Schedule::default();
        schedule.add_systems(update_wanted_badge);
        world.resource_mut::<MyReputation>().0 = Some(rep(320, TIER_PROCURADO, 640));
        schedule.run(&mut world);

        let mut nodes = world.query_filtered::<&Node, With<WantedBadge>>();
        assert_eq!(nodes.single(&world).display, Display::Flex);
        let mut texts = world.query_filtered::<&Text, With<WantedBadgeText>>();
        assert_eq!(texts.single(&world).0, "PROCURADO - cabeca: 640g");

        world.resource_mut::<MyReputation>().0 = Some(rep(0, 0, 0));
        schedule.run(&mut world);
        assert_eq!(nodes.single(&world).display, Display::None);
    }
}
