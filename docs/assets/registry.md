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

*(vazio — primeira asset vai aqui)*
