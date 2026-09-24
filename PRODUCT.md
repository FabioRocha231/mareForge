# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

Native desktop game client (Rust + Bevy 0.15), not a web page. Windows is the release target; macOS and Linux are development platforms.

## Users

- Players of a naval MMO sandbox in its public alpha, on PC.
- Experience levels are mixed. Veterans of the genre (Albion Online, EVE, Sea of Thieves) want to skip guidance; newcomers need to be led to their first profitable voyage. *(Inferred from the request "o jogo não está intuitivo" and from the handoff; the user has not confirmed it.)*
- Confirmed feedback: one alpha tester ("Sargita") had to work out how to play on their own. The game states controls, but never what to do or why.

## Product Purpose

Marvyr is an open-source naval MMO sandbox whose thesis is that **player-made wealth must be physically carried across a dangerous sea**. Ships are the courier and the target; cargo can be lost; regional prices diverge because moving goods costs risk.

Success for the alpha: a new player understands the core loop (gather, craft, load, sail, sell, or lose it all) and makes a first profitable voyage without outside help.

## Positioning

"Albion Online meets Son Korsan." Full-loot naval transport sits at the center of a player-driven economy. The ship, not a character class, is the build. The source is `docs/vision.md`, which is the binding design anchor.

## Operating Context

- **Session shape:** a player logs in with a Marvyr account, spawns at Porto da Serra, and sails a 2D top-down ocean shared with other players and NPCs.
- **Zones:** protected, frontier, and lawless.
- **Things to reach:** ports with storage, market, crafting and hiring; resource nodes; wrecks; sea events (tempest, treasure fleet, kraken, contested tide); hidden islands with treasure maps.

## Capabilities and Constraints

- **Server authority:** the server is authoritative. The client only renders and sends intents. UI must never imply the client decides outcomes.
- **Input:** keyboard + mouse, **gamepad**, and small laptop screens (1366×768) are all required targets (confirmed).
- **Languages:** **PT-BR and English** are both required (confirmed). The UI currently ships PT-BR only, and its default font renders ASCII only, so accented text currently has to be folded to ASCII.
- **Assets:** licensed assets must be CC0 (see `docs/assets/registry.md`, which fails closed on licenses). Current art is the Scallywag pixel-art pack (ships, water/islands, fort) plus small Marvyr-made UI sprites and a procedural sea shader (`assets/shaders/sea.wgsl`).
- **No image generation** is available in this workflow; new visuals must be procedural (shaders, meshes, gizmos) or CC0 assets.

## Brand Commitments

- **Name:** Marvyr (renamed from MareForge). The tagline in use is "O transporte arriscado de riqueza fabricada por jogadores".
- **Pixel-art ship and island sprites** (Scallywag, CC0) are the incumbent world art.
- **Pillars:** the five pillars in `docs/vision.md` bind every feature. The most relevant here: no useful item comes from NPCs, and zone risk is public and never hidden.

## Evidence on Hand

- Screenshots of the current client: the login screen, in-game at Porto da Serra, and the server-unavailable screen, all captured during MV-061 smoke tests.
- `session-summary.json` playtest reports written by the server.
- There are no testimonials, player counts, or reviews; none may be invented.

## Product Principles

1. **Risk is legible.** A player should always see where they are safe, what they carry, and what they can lose.
2. **Teach by sailing.** Guidance is attached to the real world at the moment it matters. There is no separate tutorial mode and no quest chain as the game's spine (the vision says "not a theme park").
3. **The sea is the protagonist.** The HUD serves the voyage and recedes when nothing needs attention.
4. **Every input path is a first-class path.** Anything doable on keyboard + mouse is doable on a gamepad, and readable on a small laptop.

## Accessibility & Inclusion

- Text must be readable at 1366×768.
- Every action must be reachable by gamepad as well as keyboard + mouse.
- UI must work in PT-BR and English, including accented characters.
