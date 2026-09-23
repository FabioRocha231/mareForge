//! Human playtest entrypoint helpers (MF-048).
//!
//! The banner is shared by the one-shot playtest binary and
//! `mareforge_client --playtest`. Dev automation defaults to off, and this
//! module removes any `MAREFORGE_AUTO*` vars inherited from the shell before
//! the client starts.

pub const PLAYTEST_BANNER: &str = r#"╔════════════════════════════════════════════════════════════╗
║  mareForge — Playable Alpha 0.1 · HUMAN PLAYTEST           ║
╠════════════════════════════════════════════════════════════╣
║  1. spawn → 2. dock (E) → 3. storage → 4. undock           ║
║  5. gather (G) → 6. dock → 7. craft (Port Screen)          ║
║  8. equip (Loadout tab) → 9. load cargo → 10. sail         ║
║  11. fight (Q/R) → 12. loot (F) → 13. dock → 14. sell      ║
║                                                            ║
║  All dev automation is OFF. Use the UI (Tab in Port Screen).║
║  Press ESC to quit.                                        ║
╚════════════════════════════════════════════════════════════╝
"#;

pub fn prepare_playtest() {
    disable_dev_automation();
    println!("{PLAYTEST_BANNER}");
}

pub fn disable_dev_automation() {
    let automation_keys: Vec<_> = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| key.to_string_lossy().starts_with("MAREFORGE_AUTO"))
        .collect();
    for key in automation_keys {
        std::env::remove_var(key);
    }
}

/// Dev tooling (MF-058): `MAREFORGE_SHOT=/caminho/prefixo` salva capturas da
/// própria janela do jogo (`prefixo-1.png`, `-2`, ...) a cada
/// `MAREFORGE_SHOT_EVERY` segundos (padrão 6) e fecha após
/// `MAREFORGE_SHOT_COUNT` capturas (padrão 3). Serve para revisar visual
/// sem capturar a tela inteira de quem está rodando.
pub struct DevScreenshotPlugin {
    pub prefix: String,
    /// `MAREFORGE_SHOT_ZOOM`: zoom inicial da câmera (m por pixel).
    pub zoom: Option<f32>,
    pub every_secs: f32,
    pub count: u32,
}

impl DevScreenshotPlugin {
    pub fn from_env() -> Option<Self> {
        let prefix = std::env::var("MAREFORGE_SHOT").ok()?;
        let number = |key: &str, default: f32| {
            std::env::var(key)
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(default)
        };
        Some(Self {
            prefix,
            zoom: std::env::var("MAREFORGE_SHOT_ZOOM")
                .ok()
                .and_then(|value| value.parse().ok()),
            every_secs: number("MAREFORGE_SHOT_EVERY", 6.0),
            count: number("MAREFORGE_SHOT_COUNT", 3.0) as u32,
        })
    }
}

#[derive(bevy::prelude::Resource)]
struct DevShots {
    prefix: String,
    every_secs: f32,
    count: u32,
    taken: u32,
    elapsed: f32,
}

impl bevy::prelude::Plugin for DevScreenshotPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.insert_resource(DevShots {
            prefix: self.prefix.clone(),
            every_secs: self.every_secs,
            count: self.count,
            taken: 0,
            elapsed: 0.0,
        })
        .add_systems(bevy::prelude::Update, take_dev_shots);
        if let Some(zoom) = self.zoom {
            app.insert_resource(crate::camera::CameraZoom(zoom));
        }
    }
}

fn take_dev_shots(
    mut commands: bevy::prelude::Commands,
    time: bevy::prelude::Res<bevy::prelude::Time>,
    mut shots: bevy::prelude::ResMut<DevShots>,
    mut exit: bevy::prelude::EventWriter<bevy::prelude::AppExit>,
) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};

    shots.elapsed += time.delta_secs();
    if shots.elapsed < shots.every_secs {
        return;
    }
    shots.elapsed = 0.0;
    if shots.taken >= shots.count {
        exit.send(bevy::prelude::AppExit::Success);
        return;
    }
    shots.taken += 1;
    let path = format!("{}-{}.png", shots.prefix, shots.taken);
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}
