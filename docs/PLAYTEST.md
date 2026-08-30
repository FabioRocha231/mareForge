# PLAYTEST — Playable Alpha 0.1

O build de playtest humano sobe o servidor e o client em um comando. Nenhuma
variavel de automacao dev e ligada: o jogador usa apenas teclado e UI.

## Como iniciar

```sh
cargo run --bin mareforge_playtest --release
```

Alternativa para rodar servidor e client em processos separados:

```sh
cargo run --bin mareforge-server --release
cargo run --bin mareforge-client --release -- --playtest
```

O client conecta automaticamente em `127.0.0.1:5000`. O servidor do playtest
escuta na porta padrao do projeto (`0.0.0.0:5000` no `SERVER_ADDR` do
servidor; `127.0.0.1:5000` no client).

## Checklist de 14 passos

1. **spawn** — ao abrir, o navio nasce na doca do Porto da Serra.
2. **dock (E)** — aproxime do porto e pressione `E` para atracar.
3. **storage** — na Port Screen, use `Tab` ate Storage e `Enter` em
   "Depositar tudo".
4. **undock** — selecione "Desatracar" com `Enter`.
5. **gather (G)** — navegue ate um node com `W/A/S/D` e pressione `G` perto.
6. **dock** — volte ao porto e pressione `E`.
7. **craft (Port Screen)** — abra a aba Crafting com `Tab` e `Enter` na
   receita disponivel.
8. **equip (Loadout tab)** — `Tab` ate Loadout, escolha a linha com
   `[Equipar]` e confirme com `Enter`.
9. **load cargo** — volte a Storage e `Enter` em "Retirar tudo".
10. **sail** — selecione "Desatracar" e navegue com `W/A/S/D`.
11. **fight (Q/R)** — `Q` dispara bombordo, `R` dispara estibordo.
12. **loot (F)** — aproxime de um destroco e pressione `F`.
13. **dock** — volte ao porto e pressione `E`.
14. **sell** — `Tab` ate Market; no painel, `Tab` troca campo, setas escolhem
    item, digitos preenchem quantidade e preco, `Enter` envia a venda.

## O que observar

- Passos 2, 5, 6, 12 e 13: o HUD mostra o prompt contextual antes da tecla.
- Passos 3, 7, 8, 9 e 14: a Port Screen mostra o feedback do servidor
  (`OK`/`ERRO`) no painel ativo.
- Passo 10: o HUD do mar mostra HP, navio, zona, ouro, carga e recargas.
- Passo 11: o combate so funciona fora das aguas protegidas; NPCs navais
  podem atacar em fronteira/lawless.
- `ESC` fora da Port Screen sai do jogo. Na Port Screen, `ESC` desatraca.

## Reportar bugs

Reproduza o passo e capture o terminal que iniciou o playtest. O servidor e o
client logam ali (stdout/stderr do processo pai). No report, descreva o passo
esperado, o que aconteceu, a tecla/UI usada e cole a linha de log relevante.

## Fora do Alpha

Este build prova o loop economico central. Nao fazem parte do Alpha: buy
orders, guildas, conquista de territorio, factions, reputation/crime,
quests, personagens em ilhas, boarding, player housing, fast travel,
transporte automatico, marketplace global, storage global, skill tree,
seguro e monetizacao.
