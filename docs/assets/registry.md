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
