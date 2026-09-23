//! Sinalizacao de risco (PRD §10, MF-017): a travessia de fronteira deve ser
//! impossivel de ignorar. O servidor define a zona real (`ZoneChanged`); aqui
//! a UI apenas representa: indicador permanente no HUD (atualizado via
//! `hud::update_zone_panel`), banner momentaneo ao entrar e placa de PvP
//! com fade.

use bevy::ecs::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::*;
use marvyr_domain_world::RiskTier;
use marvyr_protocol::ZoneChanged;

use crate::hud::{spawn_pvp_warning, spawn_zone_banner, PvpWarningAnchor, ZoneBannerAnchor};

/// Zona atual do navio do jogador, segundo o servidor.
#[derive(Resource, Debug, Clone, Default)]
pub struct CurrentZone(pub Option<ServerZone>);

#[derive(Debug, Clone)]
pub struct ServerZone {
    pub tier: RiskTier,
    pub name: String,
}

/// O aviso grande de primeira entrada em PvP ja foi exibido nesta sessao
/// (§10: uma vez por sessao, nao a cada travessia).
#[derive(Resource, Default)]
pub struct PvpWarningShown(pub bool);

pub struct ZonePlugin;

impl Plugin for ZonePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentZone>()
            .init_resource::<PvpWarningShown>()
            .add_systems(Update, (handle_zone_changed, update_zone_hud));
    }
}

/// O servidor eh a lei: so escrevemos o que ele mandou.
fn handle_zone_changed(
    mut events: EventReader<ClientReceiveMessage<ZoneChanged>>,
    mut current: ResMut<CurrentZone>,
    mut shown: ResMut<PvpWarningShown>,
    mut commands: Commands,
    banner_anchor: Query<Entity, With<ZoneBannerAnchor>>,
    warning_anchor: Query<Entity, With<PvpWarningAnchor>>,
) {
    for event in events.read() {
        let zone = event.message();
        let previous_tier = current.0.as_ref().map(|previous| previous.tier);
        info!(zone = %zone.zone_name, tier = ?zone.tier, "servidor confirmou a zona");

        // MF-057B: banner momentaneo com nome da zona ao entrar.
        if let Ok(anchor) = banner_anchor.get_single() {
            // Um banner por vez: a zona nova substitui a anterior.
            commands.entity(anchor).despawn_descendants();
            spawn_zone_banner(&mut commands, anchor, &zone.zone_name);
        }

        // MF-057C: placa de PvP com fade na primeira entrada em aguas de
        // risco da sessao.
        let entering_pvp =
            zone.tier.is_pvp() && !shown.0 && !previous_tier.is_some_and(|tier| tier.is_pvp());
        if entering_pvp {
            shown.0 = true;
            warn!("primeira entrada em aguas de risco nesta sessao");
            if let Ok(anchor) = warning_anchor.get_single() {
                spawn_pvp_warning(&mut commands, anchor);
            }
        }

        current.0 = Some(ServerZone {
            tier: zone.tier,
            name: zone.zone_name.clone(),
        });
    }
}

/// Rotulo publico de risco para usos externos (mercado, port screen).
pub fn risk_tag(tier: RiskTier) -> &'static str {
    match tier {
        RiskTier::Protected => "PvP desativado",
        RiskTier::Frontier => "PvP ATIVO - full loot",
        RiskTier::Lawless => "PvP ATIVO - full loot",
    }
}

/// Alteracao visual da agua por tier (§10). Mantida: o mar inteiro muda de
/// matiz ao atravessar fronteira, dando feedback imediato alem do banner.
fn update_zone_hud(zone: Res<CurrentZone>, mut clear_color: ResMut<ClearColor>) {
    if !zone.is_changed() {
        return;
    }
    let Some(zone) = zone.0.as_ref() else {
        return;
    };
    let water = match zone.tier {
        RiskTier::Protected => Color::srgb(0.03, 0.11, 0.20),
        RiskTier::Frontier => Color::srgb(0.07, 0.09, 0.16),
        RiskTier::Lawless => Color::srgb(0.18, 0.04, 0.06),
    };
    clear_color.0 = water;
}
