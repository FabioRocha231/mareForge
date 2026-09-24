---
version: 1
slug: "crates-client-src-ui-rs"
primary_target: "crates/client/src/ui.rs"
related_targets: ["crates/client/src/hud.rs","crates/client/src/session.rs"]
---

# Surface: Marvyr game client (HUD, login, onboarding, help)

Mode: Experience. The sea leads and the HUD serves the voyage; the Operate constraints hold (state and risk are always legible).

## Scope

- **Audience:** alpha players, veterans and newcomers alike.
- **Job:** first profitable voyage without outside help.
- **Constraints:**
  - input by keyboard + mouse and gamepad;
  - screens from 1366x768 up;
  - PT-BR + EN;
  - Bevy UI only (no web);
  - no image generation, so assets are procedural or CC0.

## Direction contract

**THESIS:** The HUD is the printed matter of the harbor: notices, bounties and manifests that sail with you. It refuses the category default of dark translucent panels with metal borders.

**OWN-WORLD:**
- Deckled rag-paper slips (warm paper #e9dcc0, ink #16202b) pinned over the living sea, casting a soft offset shadow.
- Wood-type slab display for headlines; a legible slab serif for text.
- Two-color press: ink plus vermilion #c2362b for danger, stamps and misregistered headlines. Sea teal #1f5f73 for safe water, brass #d9a441 for gold.
- Rules are woodcut double lines. Stamps mark zone and state.

**STORY:** The player understands where they are safe, what they carry and what to do next. Every hint is a printed slip with a leader line to the thing it names. The player sails, gathers, sells and graduates as a merchant.

**FIRST VIEWPORT:** The sea fills the screen. Small slips pinned at the corners:
- top-left: ship, hull and cargo;
- top-right: wind and ammunition;
- bottom-center: cannon slip;
- bottom-left: sea status.

The event headline is set in wood type, top-center, below the region stamp. The contextual action slip sits beside the ship with a leader line to its target. The first-voyage objective is one slip under the ship panel. Primary action: the contextual key or button printed on the slip.

**FORM:** Tipografia do Porto, candidate 6 of 7 on the ordered list (harbor woodcut broadsides). Seed key 6e09c968. Raises:
- leader-line slips (sukeban);
- notice-board history (flyer wall);
- F1 bound pamphlet with exact back trail (HyperCard);
- slips fold to a compact packet on small screens (Miura);
- one monumental headline per viewport (alphabet storm).

**FINISH:** unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

## Unresolved

- Server-sent dynamic messages stay PT-BR, with EN only for static strings (ceiling noted in code).

## Amendments (finish review, 2026-09-24)

- The action slip is fixed at bottom-center and the objective slip at bottom-left, not beside the ship or under the ship panel. Forcing reason: the ship moves across the whole viewport, so a slip that follows it collides with the corner slips and the portal tags at 1366x768. Fixed slots are collision-free at every supported size. The leader line keeps the "slip ties to its target" promise. It runs from the ship to the target, because the slip's screen position is constant and the ship is the player's point of attention.
- Slips keep clean 2 px ink borders; the deckled edge (procedural 9-slice torn border) is deferred as ceiling work.
