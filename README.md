# MareForge

Multi-crate Rust workspace for an ocean MMORPG.

Albion Online meets Son Korsan: a player-driven sandbox economy played out on
sailing ships, with full naval loot. See [the design vision](docs/vision.md)
(PT-BR) — it is the anchor for every feature decision.

## Build

```sh
cargo build --workspace
```

## Test

```sh
cargo test --workspace
```

## Assets

External assets must be registered in
[docs/assets/registry.md](docs/assets/registry.md).

## License

MIT OR Apache-2.0. See [LICENSE](LICENSE).
