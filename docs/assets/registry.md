# Asset Registry

Toda asset externa usada no mareForge vive aqui. Nada entra em
`assets/external/` ou `assets/mareforge/` sem estar registrado.

## Política de Licença (fail-closed)
- **CC0 / Public Domain:** permitido.
- **CC-BY / CC-BY-SA:** revisão EXPLÍCITA do maintainer antes de usar.
  Attribution required fica no `ATTRIBUTION.md`.
- **Licença custom:** revisão EXPLÍCITA; sem aprovação, proibido.
- **Sem licença clara / "free for use":** PROIBIDO.

## Como registrar uma asset

1. Coloque o arquivo em `assets/external/<nome>.png` (ou em `assets/mareforge/` se for uma adaptação nossa).
2. Adicione uma entrada na tabela abaixo com:
   - nome do arquivo
   - autor original
   - URL de origem
   - licença
   - data de obtenção (YYYY-MM-DD)
   - attribution required (true/false)
   - arquivos utilizados (paths)
   - alterações realizadas (ou "nenhuma")
3. Se `attribution required = true`, adicione também ao `docs/assets/ATTRIBUTION.md`.
4. Commit. Sem aprovação de um maintainer, nada mergeia em main.

## Registro

| Pack | Autor | URL | Licença | Attribution required | Arquivos utilizados | Modificações | Data de inclusão |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Scallywag - Ships | Pixel Carvel (comissionado); distribuído por Foozle | https://foozlecc.itch.io/scallywag-ships | CC0 1.0 | false | `assets/external/scallywag/ships/ships-tiles.png` | Renomeado de `Ships tiles.png` ao extrair o tilesheet oficial; conteúdo inalterado. | 2026-08-30 |
| Scallywag - Water and Islands | Pixel Carvel (comissionado); distribuído por Foozle | https://foozlecc.itch.io/scallywag-water-islands | CC0 1.0 | false | `assets/external/scallywag/water-islands/water-island-tiles.png` | Renomeado de `Water and Island tiles.png` ao extrair o tilesheet oficial; conteúdo inalterado. | 2026-08-30 |
| Scallywag - Fort | Pixel Carvel (comissionado); distribuído por Foozle | https://foozlecc.itch.io/scallywag-fort | CC0 1.0 | false | `assets/external/scallywag/fort/fort-tiles.png` | Renomeado de `Fort Tiles.png` ao extrair o tilesheet oficial; conteúdo inalterado. | 2026-08-30 |
| Battle at Sea (sfx) | Thimras | https://opengameart.org/content/battle-at-sea | CC0 1.0 | false | `assets/external/oga-battle-at-sea/cannon_fire_1.ogg`, `assets/external/oga-battle-at-sea/cannon_hit_ship_short.ogg`, `assets/external/oga-battle-at-sea/cannon_miss_1.ogg`, `assets/external/oga-battle-at-sea/ship_destroyed_short.ogg` | nenhuma | 2026-09-23 |
| A Sailor's Chant (música) | Thimras | https://opengameart.org/content/a-sailors-chant | CC0 1.0 | false | `assets/external/oga-sailors-chant/oga_jam_menu_music_loopable_0.ogg` | nenhuma | 2026-09-23 |
| Beach Ocean Waves | jasinski (freesound #18363); enviado por qubodup | https://opengameart.org/content/beach-ocean-waves | CC0 1.0 | false | `assets/external/oga-beach-ocean-waves/waves_loop.ogg` | `wave_01`..`wave_04` (.flac) concatenados, mono 22.05 kHz com fade de 0.3 s nas pontas para o loop, reencodado em Ogg Vorbis estéreo 44.1 kHz (ffmpeg) | 2026-09-23 |
| Short Wind Sound | remaxim | https://opengameart.org/content/short-wind-sound | CC0 1.0 | false | `assets/external/oga-short-wind/wind_loop.ogg` | Convertido para mono 22.05 kHz com fade de 0.5 s nas pontas para o loop, reencodado em Ogg Vorbis estéreo 44.1 kHz (ffmpeg) | 2026-09-23 |
| Kenney RPG Audio | Kenney | https://kenney.nl/assets/rpg-audio | CC0 1.0 | false | `assets/external/kenney-rpg-audio/creak1.ogg`, `assets/external/kenney-rpg-audio/handleCoins.ogg` | nenhuma | 2026-09-23 |
| Kenney Impact Sounds | Kenney | https://kenney.nl/assets/impact-sounds | CC0 1.0 | false | `assets/external/kenney-impact-sounds/impactBell_heavy_000.ogg` | nenhuma | 2026-09-23 |
| Kenney Interface Sounds | Kenney | https://kenney.nl/assets/interface-sounds | CC0 1.0 | false | `assets/external/kenney-interface-sounds/click_002.ogg` | nenhuma | 2026-09-23 |

## Assets geradas pelo próprio mareForge

Sprites geradas pelo time do mareForge para preencher HUD, combate e mar. O conteúdo é nosso, dedicado ao domínio público (CC0 1.0); sem attribution externa, então `ATTRIBUTION.md` permanece vazio para esta seção.

| Pack | Autor | URL | Licença | Attribution required | Arquivos utilizados | Modificações | Data de inclusão |
| --- | --- | --- | --- | --- | --- | --- | --- |
| mareForge UI (MF-057A) | Equipe mareForge | (gerado internamente) | CC0 1.0 | false | `assets/mareforge/ui/panel_ship.png`, `assets/mareforge/ui/panel_zone.png`, `assets/mareforge/ui/panel_cooldowns.png`, `assets/mareforge/ui/panel_prompt.png`, `assets/mareforge/ui/panel_warning.png`, `assets/mareforge/ui/panel_port.png`, `assets/mareforge/ui/icon_ship.png`, `assets/mareforge/ui/icon_hp.png`, `assets/mareforge/ui/icon_cargo.png`, `assets/mareforge/ui/icon_gold.png`, `assets/mareforge/ui/icon_warn.png`, `assets/mareforge/ui/icon_skull.png` | nenhuma | 2026-08-31 |
| mareForge Ships presence (MF-057F) | Equipe mareForge | (gerado internamente) | CC0 1.0 | false | `assets/mareforge/ships/shadow.png` | nenhuma | 2026-08-31 |
| mareForge Ocean polish (MF-057D) | Equipe mareForge | (gerado internamente) | CC0 1.0 | false | `assets/mareforge/world/ocean_deep.png`, `assets/mareforge/world/shore_band.png` | nenhuma | 2026-08-31 |
| mareForge Combat feel (MF-057J) | Equipe mareForge | (gerado internamente) | CC0 1.0 | false | `assets/mareforge/effects/muzzle_flash.png`, `assets/mareforge/effects/smoke.png`, `assets/mareforge/effects/wake.png` | nenhuma | 2026-08-31 |
