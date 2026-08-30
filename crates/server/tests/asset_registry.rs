//! MF-046: asset guard runs from `cargo test --workspace` so CI cannot
//! silently accept unregistered files under assets/external or assets/mareforge.

use std::path::Path;
use std::process::Command;

#[test]
fn all_assets_are_registered() {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check_unregistered_assets.sh");
    let output = Command::new(&script)
        .output()
        .unwrap_or_else(|error| panic!("asset guard não executou: {error}"));
    assert!(
        output.status.success(),
        "asset guard falhou:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
