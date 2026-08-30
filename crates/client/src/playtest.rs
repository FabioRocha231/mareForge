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
