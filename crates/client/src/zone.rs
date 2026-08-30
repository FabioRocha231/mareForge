//! Sinalização de risco (PRD §10, MF-017): a travessia de fronteira deve ser
//! impossível de ignorar. O servidor define a zona real (`ZoneChanged`); aqui
//! a UI apenas representa — indicador permanente, tinta da água por tier e o
//! aviso de primeira entrada em PvP da sessão.

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::*;
use mareforge_domain_world::RiskTier;
use mareforge_protocol::ZoneChanged;

/// Zona atual do navio do jogador, segundo o servidor.
#[derive(Resource, Debug, Clone, Default)]
pub struct CurrentZone(pub Option<ServerZone>);

#[derive(Debug, Clone)]
pub struct ServerZone {
    pub tier: RiskTier,
    pub name: String,
}

/// O aviso de primeira entrada em PvP já foi exibido nesta sessão (§10:
/// uma vez por sessão, não a cada travessia).
#[derive(Resource, Default)]
pub struct PvpWarningShown(pub bool);

/// Indicador permanente de risco no topo da tela.
#[derive(Component)]
pub struct ZoneReadout;

/// O aviso grande de entrada em PvP; some sozinho após alguns segundos.
#[derive(Component)]
pub struct PvpWarningText;

pub struct ZonePlugin;

impl Plugin for ZonePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentZone>()
            .init_resource::<PvpWarningShown>()
            .add_systems(Startup, setup_zone_hud)
            .add_systems(
                Update,
                (handle_zone_changed, update_zone_hud, expire_pvp_warning),
            );
    }
}

fn setup_zone_hud(mut commands: Commands) {
    commands.spawn((
        Text2d::new("—"),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.85, 0.85)),
        Transform::from_xyz(0.0, 155.0, 0.0),
        ZoneReadout,
    ));
}

/// O servidor é a lei: só escrevemos o que ele mandou.
fn handle_zone_changed(
    mut events: EventReader<ClientReceiveMessage<ZoneChanged>>,
    mut current: ResMut<CurrentZone>,
    mut shown: ResMut<PvpWarningShown>,
    mut commands: Commands,
) {
    for event in events.read() {
        let zone = event.message();
        let previous_tier = current.0.as_ref().map(|previous| previous.tier);
        info!(zone = %zone.zone_name, tier = ?zone.tier, "servidor confirmou a zona");

        // §10: primeira entrada em PvP da sessão ganha o aviso grande.
        let entering_pvp =
            zone.tier.is_pvp() && !shown.0 && !previous_tier.is_some_and(|tier| tier.is_pvp());
        if entering_pvp {
            shown.0 = true;
            warn!("primeira entrada em águas de risco nesta sessão");
            commands.spawn((
                Text2d::new(
                    "Você está entrando em águas de risco.\n\nSeu navio, equipamentos e carga poderão ser perdidos.",
                ),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.45, 0.35)),
                Transform::from_xyz(0.0, 30.0, 0.0),
                PvpWarningText,
            ));
        }

        current.0 = Some(ServerZone {
            tier: zone.tier,
            name: zone.zone_name.clone(),
        });
    }
}

/// Indicador permanente + alteração visual da água por tier (§10).
fn update_zone_hud(
    zone: Res<CurrentZone>,
    mut clear_color: ResMut<ClearColor>,
    mut readouts: Query<(&mut Text2d, &mut TextColor), With<ZoneReadout>>,
) {
    if !zone.is_changed() {
        return;
    }
    let Some(zone) = zone.0.as_ref() else {
        return;
    };
    let (risk_tag, ink) = match zone.tier {
        RiskTier::Protected => ("PvP desativado", Color::srgb(0.55, 0.9, 0.6)),
        RiskTier::Frontier => ("PvP ATIVO · full loot", Color::srgb(0.98, 0.75, 0.35)),
        RiskTier::Lawless => ("PvP ATIVO · full loot", Color::srgb(1.0, 0.35, 0.3)),
    };
    let water = match zone.tier {
        RiskTier::Protected => Color::srgb(0.04, 0.14, 0.24),
        RiskTier::Frontier => Color::srgb(0.08, 0.11, 0.19),
        RiskTier::Lawless => Color::srgb(0.20, 0.05, 0.07),
    };

    if let Ok((mut label, mut color)) = readouts.get_single_mut() {
        label.0 = format!("{} — {risk_tag}", zone.name);
        *color = TextColor(ink);
    }
    clear_color.0 = water;
}

/// O aviso de PvP se apaga sozinho.
fn expire_pvp_warning(
    time: Res<Time>,
    mut alive: Local<f32>,
    mut commands: Commands,
    warnings: Query<Entity, With<PvpWarningText>>,
) {
    let Ok(entity) = warnings.get_single() else {
        *alive = 0.0;
        return;
    };
    *alive += time.delta_secs();
    if *alive >= 6.0 {
        commands.entity(entity).despawn();
        *alive = 0.0;
    }
}
