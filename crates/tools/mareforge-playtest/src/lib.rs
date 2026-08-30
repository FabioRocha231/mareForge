pub use mareforge_client::playtest::{disable_dev_automation, PLAYTEST_BANNER};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playtest_runs_without_panicking() {
        assert!(PLAYTEST_BANNER.contains("HUMAN PLAYTEST"));
        assert!(PLAYTEST_BANNER.contains("spawn"));
        assert!(PLAYTEST_BANNER.contains("dock (E)"));
        assert!(PLAYTEST_BANNER.contains("All dev automation is OFF"));
        assert!(PLAYTEST_BANNER.contains("Press ESC to quit."));
    }

    #[test]
    fn playtest_disables_inherited_automation_env() {
        std::env::set_var("MAREFORGE_AUTODOCK", "1");
        std::env::set_var("MAREFORGE_AUTOEQUIP", "1");

        disable_dev_automation();

        assert!(std::env::var_os("MAREFORGE_AUTODOCK").is_none());
        assert!(std::env::var_os("MAREFORGE_AUTOEQUIP").is_none());
    }
}
