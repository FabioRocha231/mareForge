//! Guarda de arquitetura (ADR-0005/0006, MF-033): os crates de domínio e o
//! protocolo são puros — nada de transporte, banco, runtime assíncrono ou
//! ECS neles. O boundary de persistência vive no server (persist.rs).

use std::path::PathBuf;

fn crate_manifest(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates")
        .join(name)
        .join("Cargo.toml")
}

fn assert_stays_pure(crate_name: &str, forbidden: &[&str]) {
    let manifest = crate_manifest(crate_name);
    let content = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|error| panic!("lendo {}: {error}", manifest.display()));
    for dependency in forbidden {
        assert!(
            !content.contains(dependency),
            "{crate_name} deve permanecer puro: dependência '{dependency}' proibida (ADR-0006)"
        );
    }
}

#[test]
fn domain_and_protocol_crates_stay_pure() {
    for crate_name in [
        "shared",
        "protocol",
        "domain-items",
        "domain-economy",
        "domain-crafting",
        "domain-ships",
        "domain-combat",
        "domain-world",
    ] {
        assert_stays_pure(crate_name, &["sqlx", "tokio", "bevy", "lightyear"]);
    }
}

/// MF-033: quem fala com Postgres é o server (persist.rs) — e só ele.
#[test]
fn postgres_knowledge_lives_only_in_the_server() {
    for crate_name in [
        "shared",
        "protocol",
        "domain-items",
        "domain-economy",
        "domain-crafting",
        "domain-ships",
        "domain-combat",
        "domain-world",
        "client",
    ] {
        let manifest = crate_manifest(crate_name);
        let content = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|error| panic!("lendo {}: {error}", manifest.display()));
        assert!(
            !content.contains("sqlx") && !content.contains("postgres"),
            "{crate_name} não pode conhecer sqlx/postgres (ADR-0004, MF-033)"
        );
    }
}
