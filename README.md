# mareForge

**Playable Alpha** of an open-source Rust naval multiplayer sandbox.

mareForge is built around a simple pressure test: player-made value must be
physically moved through a dangerous world. Ships carry the economy, regional
cargo creates reasons to sail, and combat can make cargo and ships change hands.
This is a small playable slice, not a production MMO.

## Why it is different

- **Player-driven economy:** resources, equipment, and ships are intended to
  enter the economy through gathering, crafting, and trade.
- **Cargo has a location:** wealth is carried by ships rather than abstracted
  into a global inventory.
- **Risk is visible:** protected, frontier, and lawless zones define where PvP
  and full loot apply.
- **The ship is the build:** capacity and combat capability come from the ship
  and its equipped components, rather than character classes or XP levels.

## Core loop

```text
gather -> craft -> equip -> load cargo -> sail -> fight -> loot or lose -> sell
```

The current slice includes two ports, regional storage and market flows, a
high-risk island, naval NPCs, and three ship types: `Small Merchant`, `Patrol`,
and `Corsair`.

## Playable Alpha

The human playtest runs a server and client together. It starts at Porto da
Serra and provides keyboard-driven sailing, docking, storage, gathering,
crafting, loadout, market selling, naval combat, and wreck looting.

## Play now

Requirements: Rust 1.80 or newer.

```sh
cargo run --bin mareforge_playtest --release
```

The client and server use `127.0.0.1:5000` by default. To use another port:

```sh
MAREFORGE_PORT=5001 cargo run --bin mareforge_playtest --release
```

For the complete 14-step checklist and bug-reporting guidance, see
[`docs/PLAYTEST.md`](docs/PLAYTEST.md).

Essential controls: `WASD` sail, `E` dock or undock, `G` gather, `Q`/`R` fire
broadside weapons, and `F` loot a wreck. The checklist covers the complete UI
flow.

## Test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Works today

- Rust workspace with Bevy client/server and Lightyear networking.
- Server-authoritative ship, combat, gathering, crafting, storage, loadout,
  market, wreck, and loot messages.
- 2D top-down naval movement with protected, frontier, and lawless zones.
- Server simulation at 30 Hz, snapshots at 20 Hz, and grid-based interest
  management.
- Domain crates for items, crafting, economy, ships, combat, and world rules.
- PostgreSQL/sqlx persistence code and an immutable economic ledger design.

## Architecture

mareForge is a modular Rust workspace built with Rust, Bevy, Lightyear, and
PostgreSQL/sqlx. Domain rules live in focused crates such as
`domain-items`, `domain-crafting`, `domain-economy`, `domain-ships`,
`domain-combat`, and `domain-world`; `protocol` defines replicated state and
intents; `shared` contains common IDs and errors. The server remains the
authority for competitive and economic state.

The technical decisions are recorded in [`docs/adr/README.md`](docs/adr/README.md).

## Principles

The design is anchored by five rules:

1. Useful value is made by players.
2. The ship is both courier and target.
3. Risk and opportunity share a frontier.
4. The server is the law.
5. You are what you sail.

Read the full design rationale in [`docs/vision.md`](docs/vision.md).

## Roadmap

The next work is evidence-led: use the Alpha loop with small groups, observe
whether transport creates routes, ambushes, escorts, losses, loot transfer,
re-crafting, and price differences, then improve the systems that generate
those interactions. The vertical slice targets 2-10 players; scale, guild
territory, factions, quests, boarding, mobile, monetization, and other excluded
features are deliberately outside the current proof.

## Contributing

Contributions are welcome across Rust/gameplay, networking/backend, design/UI,
pixel art/sound, and docs/testing/balance. Please open an issue before a large
architectural change so its scope can be discussed first.

## Open source

Contributions should preserve the economic and server-authority principles.
External assets must be registered under [`docs/assets/registry.md`](docs/assets/registry.md)
and follow its license policy. Assets have their own licenses; required notices
are tracked in [`docs/assets/ATTRIBUTION.md`](docs/assets/ATTRIBUTION.md). Start
with a reproducible playtest report or a focused change that can be tested in
the workspace.

## Support

There are no official support, funding, crowdfunding, or community channels
published yet. Future funding may help pay for servers, art, audio, and
infrastructure. Real money must never buy gameplay power. For now, use the
repository's issue and pull-request workflows.

## Inspirations

The project takes design inspiration from **Albion Online** and **Son Korsan**.
mareForge is independent and is not affiliated with either project.

## License

MIT OR Apache-2.0. See [`LICENSE`](LICENSE).
