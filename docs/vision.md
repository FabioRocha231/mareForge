# Visão do mareForge

> Albion Online encontra Son Korsan. A economia sandbox mais implacável do gênero
> acontece em cima de um convés.

Este documento é a âncora de design do projeto. ADRs registram decisões
técnicas; este registro afirma **o que o jogo é**. Toda feature daqui pra frente
deve fortalecer ao menos um pilar e violar nenhum.

---

## A tese

Economia player-driven só funciona quando transportar riqueza custa sangue. É
por isso que Albion tem full loot e zonas vermelhas: sem risco, todo recurso
vale o mesmo em todo lugar e o mercado morre.

O mareForge leva essa tese para o mar. O transporte não é um jogador a pé com
uma mula numa estrada — é um **navio de madeira lento, cheio de carga,
navegando mar aberto**. E do outro lado do horizonte tem um corsário sabendo
disso.

## Os pilares

### 1. Tudo que vale algo foi fabricado por um jogador

Nenhum item útil nasce de NPC. Toda riqueza do servidor segue o caminho
coleta → refino → fabricação, conduzida por jogadores. O catálogo de itens é o
coração do jogo, e por isso validação de inventário, receita e transação é
**fail-closed**: item desconhecido é erro, nunca "assume ok".

**No código:** `domain-items`, `domain-crafting`, `domain-economy`.
**Nos ADRs:** 0010 (ledger imutável, transação atômica).

### 2. O navio é o courier e o dungeon

Em Albion, o courier carrega riqueza. Em Son Korsan, o navio é o alvo. No
mareForge é a mesma coisa: **a carga é a riqueza móvel do servidor e o casco
que a carrega é afundável**. Full loot naval não é feature de PvP — é o
regulador econômico central. O `SmallMerchant` existe para mover valor; o
`Corsair` existe para taxá-lo. Um não faz sentido sem o outro.

**No código:** `domain-ships` (`SmallMerchant`, `Patrol`, `Corsair` — os três
tipos do vertical slice já contam essa história).
**Nos ADRs:** 0007 (física naval própria), 0008 (tick 30 Hz).

### 3. Risco e riqueza vivem na mesma fronteira

Porto é seguro e barato. Mar aberto é perigoso e rico. O gradiente é explícito
e sem surpresas: quem navega para alto mar aceitou o pacto. Consequência
econômica direta: preços regionais divergem porque mover mercadoria custa
risco — e essa divergência é o combustível do mercador, o gameplay mais
honesto que um MMO sandbox pode oferecer.

**Nos ADRs:** 0009 (interest management — mar aberto com muitos navios é o
cenário alvo).

### 4. O servidor é a lei

Jogo competitivo com economia real não tolera cliente confiável, item
duplicado ou rollback de ledger. Autoridade radical no servidor não é
formalismo técnico — é o que torna "propriedade" uma palavra com significado
no mundo do jogo.

**Nos ADRs:** 0003 (server authority), 0004 (PostgreSQL ACID), 0011
(versionamento de protocolo).

### 5. Você é o que você navega

Não existe classe, não existe nível de personagem. Capacidade é propriedade:
o navio que você comanda e o equipamento nos slots definem o que você é capaz
de fazer. Progressão é construir/comprar/navios melhores, não moer XP.

**No código:** `ShipDefinition` + slots + `EquippedComponents` → `ShipStats`.
**Nos ADRs:** 0006 (regras de negócio fora do ECS, definições ricas).

## O que o mareForge NÃO é

- **Não é theme park.** Sem corrente de quests como espinha dorsal. O mundo
  apresenta sistemas; os jogadores geram as histórias.
- **Não tem grind de personagem.** Nada do que importa é preso a XP. Quem
  constrói é propriedade e conhecimento do mundo, não tempo de login.
- **Não instancia PvE.** O oceano é um mundo compartilhado. Não existe "sua
  cópia do mar".
- **Não esconde o risco.** Gradiente de zona é contrato público. Proteção
  "surpresa" em zona de risco é quebra de design, não polimento.
- **Não vende poder.** Nenhuma decisão de monetização pode comprar o que o
  Pilar 1 obriga jogadores a fabricar.

## Regra de decisão

Quando uma feature futura entrar em conflito com um pilar, o pilar vence — e
quando entrar em conflito com dois, o Pilar 2 vence, porque a tese inteira do
jogo está nele: **o transporte arriscado de riqueza fabricada por jogadores é
o coração do mareForge.**

Se uma feature precisa de uma exceção divina para não quebrar a economia, a
feature está errada. O conserto é no design do risco, nunca na exceção.

## Mapeamento código ↔ pilares

| Pilar | Crates | ADRs |
|---|---|---|
| 1. Fabricado por jogadores | `domain-items`, `domain-crafting`, `domain-economy` | 0010 |
| 2. Navio courier/dungeon | `domain-ships` | 0007, 0008 |
| 3. Risco na fronteira | (zonas e mercado regional — Phase futura) | 0009 |
| 4. Servidor é a lei | `server`, `protocol`, `shared` | 0003, 0004, 0011 |
| 5. Você é o que navega | `domain-ships` (defs/slots/stats) | 0006 |

## Onde estamos

Phase 0 fechada: domínios puros e testados, servidor com loop de tick real,
ADRs técnicos assinados. A visão acima é o contrato para a Phase 1 em diante:
equipar o `EquippedComponent` com definições ricas de item (Pilar 5), dar
corpo ao catálogo de itens (Pilar 1) e, do primeiro combate naval em diante,
fazer o Pilar 2 valer — afundou, perdeu tudo.
