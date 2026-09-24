//! Tema e peças comuns da UI em espaço de tela (bevy_ui). Nada aqui é filho
//! da câmera: zoom/projeção não afetam o HUD nem a tela de porto.

use bevy::prelude::*;

pub const PANEL_BG: Color = Color::srgba(0.05, 0.09, 0.14, 0.82);
pub const PANEL_BORDER: Color = Color::srgb(0.78, 0.62, 0.32);
pub const TEXT: Color = Color::srgb(0.95, 0.91, 0.80);
pub const TEXT_DIM: Color = Color::srgb(0.66, 0.66, 0.62);
pub const GOLD: Color = Color::srgb(0.98, 0.80, 0.35);
pub const DANGER: Color = Color::srgb(0.92, 0.33, 0.28);
pub const OK_GREEN: Color = Color::srgb(0.42, 0.82, 0.48);
pub const AMBER: Color = Color::srgb(0.95, 0.70, 0.28);
pub const BAR_TRACK: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
pub const BUTTON_BG: Color = Color::srgba(0.10, 0.16, 0.24, 0.90);
pub const BUTTON_SELECTED: Color = Color::srgba(0.36, 0.28, 0.12, 0.95);
pub const MARGIN: f32 = 16.0;

/// Botão com realce ao passar o mouse; `base` é a cor em repouso.
#[derive(Component, Clone, Copy)]
pub struct UiButton {
    pub base: Color,
}

pub struct UiThemePlugin;

impl Plugin for UiThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, button_hover);
    }
}

fn button_hover(
    mut buttons: Query<(&Interaction, &UiButton, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, button, mut bg) in &mut buttons {
        bg.0 = match interaction {
            Interaction::Pressed => PANEL_BORDER.with_alpha(0.55),
            Interaction::Hovered => button.base.mix(&PANEL_BORDER, 0.30),
            Interaction::None => button.base,
        };
    }
}

/// Painel padrão: fundo marinho translúcido, borda latão fina, cantos 5px.
pub fn panel(node: Node) -> impl Bundle {
    (
        Node {
            border: UiRect::all(Val::Px(1.0)),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            ..node
        },
        BackgroundColor(PANEL_BG),
        BorderColor(PANEL_BORDER),
        BorderRadius::all(Val::Px(5.0)),
    )
}

/// A fonte pixel do HUD só tem ASCII: acentos e pontuação tipográfica
/// (inclusive os que chegam do servidor) viram equivalentes legíveis.
pub fn fold(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => out.push('a'),
            'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => out.push('A'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'É' | 'È' | 'Ê' | 'Ë' => out.push('E'),
            'í' | 'ì' | 'î' | 'ï' => out.push('i'),
            'Í' | 'Ì' | 'Î' | 'Ï' => out.push('I'),
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => out.push('o'),
            'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => out.push('O'),
            'ú' | 'ù' | 'û' | 'ü' => out.push('u'),
            'Ú' | 'Ù' | 'Û' | 'Ü' => out.push('U'),
            'ç' => out.push('c'),
            'Ç' => out.push('C'),
            '·' | '•' => out.push('*'),
            '—' | '–' => out.push('-'),
            '…' => out.push_str("..."),
            other => out.push(other),
        }
    }
    out
}

pub fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(fold(&value.into())),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}

pub fn button(node: Node, base: Color) -> impl Bundle {
    (
        Button,
        Node {
            border: UiRect::all(Val::Px(1.0)),
            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..node
        },
        BackgroundColor(base),
        BorderColor(PANEL_BORDER.with_alpha(0.45)),
        BorderRadius::all(Val::Px(4.0)),
        UiButton { base },
    )
}

/// Barra horizontal: trilho escuro com um filho de preenchimento (`fill`)
/// cuja largura é percentual. Retorna nada; o filho carrega `fill`.
pub fn spawn_bar(parent: &mut ChildBuilder, width: f32, color: Color, fill: impl Bundle) {
    parent
        .spawn((
            Node {
                width: Val::Px(width),
                height: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(BAR_TRACK),
            BorderRadius::all(Val::Px(3.0)),
        ))
        .with_children(|track| {
            track.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(color),
                BorderRadius::all(Val::Px(3.0)),
                fill,
            ));
        });
}

/// Fração 0..=1 como largura de barra.
pub fn bar_width(fraction: f32) -> Val {
    Val::Percent(fraction.clamp(0.0, 1.0) * 100.0)
}

/// Fade in → hold → fade out, usado por banner, aviso PvP e toasts.
/// Guarda a cor base do fundo/borda para aplicar alpha proporcional.
#[derive(Component, Clone, Copy)]
pub struct UiFade {
    pub elapsed: f32,
    pub fade_in: f32,
    pub hold_until: f32,
    pub end: f32,
    pub bg: Color,
    pub border: Color,
}

impl UiFade {
    pub fn new(fade_in: f32, hold_until: f32, end: f32, bg: Color, border: Color) -> Self {
        Self {
            elapsed: 0.0,
            fade_in,
            hold_until,
            end,
            bg,
            border,
        }
    }

    pub fn alpha(&self) -> f32 {
        let t = self.elapsed;
        if t < self.fade_in {
            t / self.fade_in
        } else if t < self.hold_until {
            1.0
        } else if t < self.end {
            1.0 - (t - self.hold_until) / (self.end - self.hold_until)
        } else {
            0.0
        }
    }
}

pub fn tick_ui_fades(
    time: Res<Time>,
    mut commands: Commands,
    mut fades: Query<(
        Entity,
        &mut UiFade,
        &mut BackgroundColor,
        &mut BorderColor,
        &Children,
    )>,
    mut texts: Query<&mut TextColor>,
) {
    for (entity, mut fade, mut bg, mut border, children) in &mut fades {
        fade.elapsed += time.delta_secs();
        let alpha = fade.alpha();
        bg.0 = fade.bg.with_alpha(fade.bg.alpha() * alpha);
        border.0 = fade.border.with_alpha(fade.border.alpha() * alpha);
        for child in children.iter() {
            if let Ok(mut color) = texts.get_mut(*child) {
                color.0.set_alpha(alpha);
            }
        }
        if fade.elapsed >= fade.end {
            commands.entity(entity).despawn_recursive();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_ramps_in_holds_and_ramps_out() {
        let mut fade = UiFade::new(0.4, 2.4, 3.2, PANEL_BG, PANEL_BORDER);
        assert_eq!(fade.alpha(), 0.0);
        fade.elapsed = 0.2;
        assert!((fade.alpha() - 0.5).abs() < 1e-5);
        fade.elapsed = 1.0;
        assert_eq!(fade.alpha(), 1.0);
        fade.elapsed = 2.8;
        assert!((fade.alpha() - 0.5).abs() < 1e-5);
        fade.elapsed = 3.5;
        assert_eq!(fade.alpha(), 0.0);
    }

    #[test]
    fn bar_width_is_clamped_percentage() {
        assert_eq!(bar_width(0.25), Val::Percent(25.0));
        assert_eq!(bar_width(1.7), Val::Percent(100.0));
        assert_eq!(bar_width(-1.0), Val::Percent(0.0));
    }
}

#[cfg(test)]
mod fold_tests {
    #[test]
    fn fold_keeps_ascii_and_strips_accents() {
        assert_eq!(
            super::fold("Capitão · Reparo concluído — ok…"),
            "Capitao * Reparo concluido - ok..."
        );
        assert_eq!(super::fold("MUNICAO [C]"), "MUNICAO [C]");
    }
}
