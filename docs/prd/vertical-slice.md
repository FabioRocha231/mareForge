# mareForge — PRD do Vertical Slice

**Versão:** 0.1
**Data:** 2026-08-30
**Status:** Ready for implementation planning
**Owner de Game Design e Arquitetura:** gepeto
**Executor principal:** GLM-5.3-Flash

---

# 1. Resumo executivo

O objetivo do primeiro Vertical Slice do mareForge não é provar que conseguimos construir um MMORPG.

O objetivo é provar que a tese econômica central do mareForge gera gameplay.

Essa tese é:

> Riqueza fabricada por jogadores precisa ser transportada fisicamente pelo mundo, e transportar riqueza precisa envolver risco real.

O Vertical Slice deve permitir que jogadores:

**colete → fabrique → transporte → arrisque → combata → perca/saqueie → venda → fabrique novamente**

Se esse ciclo for divertido com 2 a 10 jogadores, o projeto possui uma fundação válida para evoluir para um sandbox MMO.

---

# 2. Pilares protegidos

Toda feature deste PRD deve fortalecer ao menos um dos pilares oficiais.

## Pilar 1 — Tudo que vale algo foi fabricado por um jogador

NPCs não geram equipamento, navio ou recurso útil diretamente.

Recursos entram na economia através de coleta realizada por jogadores.

Equipamentos e navios entram através de crafting realizado por jogadores.

---

## Pilar 2 — O navio é o courier e o dungeon

O navio é simultaneamente:

* personagem;
* build;
* inventário;
* veículo;
* propriedade;
* alvo;
* risco econômico.

Este pilar vence conflitos de design.

---

## Pilar 3 — Risco e riqueza vivem na mesma fronteira

Regiões mais perigosas precisam fornecer oportunidades econômicas melhores.

O jogador nunca deve descobrir o nível de risco depois de entrar.

---

## Pilar 4 — O servidor é a lei

Estado econômico e competitivo é server-authoritative.

Nunca confiar no client para:

* dano;
* posição definitiva;
* loot;
* crafting;
* ownership;
* saldo;
* mercado;
* transferência de item.

---

## Pilar 5 — Você é o que você navega

Não existe classe tradicional nem nível de personagem.

Capacidade vem do navio + equipamentos instalados.

---

# 3. Objetivo do Vertical Slice

Devemos conseguir colocar entre 2 e 10 jogadores em um pequeno mundo persistente e observar espontaneamente situações como:

* alguém identifica uma rota lucrativa;
* alguém começa a transportar carga;
* outro jogador percebe essa rota;
* surge um corsário;
* comerciantes passam a buscar rotas alternativas;
* outro jogador usa Patrol para escoltar;
* navios são destruídos;
* itens desaparecem da economia;
* loot muda de proprietário;
* crafting volta a ser necessário;
* preços entre portos divergem.

Quando o próprio sistema começa a gerar essas histórias, o mareForge está funcionando.

---

# 4. O que NÃO estamos tentando provar

Ficam explicitamente fora do Vertical Slice:

* milhares de jogadores;
* guerra de guildas;
* conquista funcional de territórios;
* factions;
* reputation/crime system;
* quests extensas;
* narrativa;
* personagens andando em ilhas;
* boarding;
* player housing;
* dezenas de navios;
* skill tree;
* XP de personagem;
* seguro;
* fast travel;
* transporte automático;
* marketplace global;
* armazenamento global;
* mobile;
* PvE instanciado;
* dungeons;
* seasons;
* battle pass;
* monetização;
* cosméticos complexos.

---

# 5. Perspectiva de jogo

O Vertical Slice será essencialmente:

**2D top-down naval.**

Não implementar personagem terrestre.

Enquanto o jogador está no mundo, sua representação jogável é o navio.

Portos funcionam inicialmente como áreas/interface de serviços, não como cidades caminháveis.

### Pilares

Fortalece 2 e 5.

---

# 6. Mundo inicial

O primeiro mundo deverá ter:

* 2 portos;
* 1 ilha/região de alto risco;
* águas protegidas;
* fronteira PvP;
* alto-mar de maior recompensa.

Estrutura conceitual:

```text
                 HIGH-RISK ISLAND
                recursos escassos
                      │
                      │
                 LAWLESS SEA
                    /   \
                   /     \
                  /       \
            FRONTIER     FRONTIER
               /             \
              /               \
         PORT A ──────────── PORT B
          │                     │
     SAFE WATERS           SAFE WATERS
```

O mapa deve formar um triângulo econômico.

---

# 7. Especialização regional

Port A e Port B não devem ser economicamente idênticos.

Exemplo inicial:

## Port A

Maior abundância de madeira.

Possui:

* Workbench;
* Dock.

## Port B

Maior abundância de minério.

Possui:

* Anvil;
* Dock.

## High-Risk Island

Possui recurso raro necessário para equipamentos melhores.

Não possui armazenamento seguro.

Não possui mercado seguro.

A intenção é obrigar recursos a circular.

### Pilares

Fortalece 1, 2 e 3.

---

# 8. Zonas de risco

O Vertical Slice possuirá três tiers.

Identificadores sugeridos:

```text
RiskTier::Protected
RiskTier::Frontier
RiskTier::Lawless
```

---

## Protected

PvP:

**desativado.**

Disponibilidade:

* recursos básicos;
* portos;
* crafting;
* storage;
* market.

Recompensa econômica:

baixa.

---

## Frontier

PvP:

**ativado com full loot.**

Disponibilidade:

* recursos básicos;
* recursos intermediários;
* rotas comerciais.

Recompensa:

média.

---

## Lawless

PvP:

**ativado com full loot.**

Disponibilidade:

* recursos raros;
* melhores oportunidades econômicas.

Recompensa:

alta.

---

# 9. Regra importante sobre zonas

Não existirão duas versões de full loot.

Entrou em região PvP:

**afundou, perdeu tudo que estava no navio.**

A diferença entre Frontier e Lawless está na:

* geografia;
* distância;
* recursos;
* tráfego;
* oportunidade econômica.

Não em regras secretas de proteção.

### Pilar

Fortalece diretamente o Pilar 3.

---

# 10. Sinalização de risco

A transição de zona deve ser impossível de ignorar.

O client deve mostrar:

* alteração visual da água/borda;
* nome da zona;
* indicador permanente de risco;
* aviso ao cruzar Protected → PvP;
* mapa identificando os tiers.

Na primeira entrada em PvP da sessão:

```text
Você está entrando em águas de risco.

Seu navio, equipamentos e carga poderão ser perdidos.
```

O servidor define a zona real.

A UI apenas representa.

---

# 11. Os três navios

Os três `ShipKind` existentes permanecem como únicos navios econômicos do Vertical Slice:

```text
SmallMerchant
Patrol
Corsair
```

---

# 12. SmallMerchant

Função:

**transportador.**

Prioridades:

* maior cargo;
* eficiência logística;
* capacidade defensiva suficiente para tentar fugir.

Trade-offs:

* menor pressão ofensiva;
* pior perseguição.

O gameplay do Merchant não é vencer batalhas.

É escolher batalhas que não acontecerão.

### Pilares

2 e 5.

---

# 13. Patrol

Função:

**controle de área e escolta.**

Prioridades:

* HP;
* equilíbrio;
* capacidade de enfrentar Corsair.

Trade-offs:

* menos cargo que Merchant;
* menos mobilidade ofensiva que Corsair.

Deve criar naturalmente gameplay de escolta.

### Pilares

2 e 5.

---

# 14. Corsair

Função:

**interceptação.**

Prioridades:

* velocidade;
* perseguição;
* dano.

Trade-offs:

* menor cargo;
* menor sustentabilidade que Patrol.

Um Corsair precisa conseguir pegar um Merchant.

Mas não deve desejar enfrentar um Patrol equivalente sem considerar o risco.

### Pilares

2 e 5.

---

# 15. Triângulo de função

Não buscar pedra-papel-tesoura rígido.

Buscar incentivos.

```text
SmallMerchant
    │
    │ cria oportunidade
    ▼
Corsair
    │
    │ cria demanda por proteção
    ▼
Patrol
    │
    │ reduz liberdade do Corsair
    └───────────────┐
                    │
                Merchant
```

---

# 16. Equipamentos

Os slots existentes permanecem:

```text
Hull
Sail
Weapon
Aux
```

No Vertical Slice, equipamentos devem modificar pelo menos:

* speed;
* max_hp;
* cargo_capacity;
* weapon_damage;
* weapon_range.

A dívida atual em que modificadores vivem diretamente em `EquippedComponent` deve ser eliminada durante a evolução da Phase 1.

A definição rica do equipamento deverá pertencer ao catálogo de itens.

`EquippedComponent` deve apontar para a definição.

### Pilar

Fortalece diretamente o Pilar 5.

---

# 17. Movimento naval

Movimento deve priorizar sensação de peso sem simulação naval realista.

Modelo:

* posição 2D;
* heading;
* velocidade escalar;
* aceleração;
* desaceleração;
* turn rate.

Não utilizar física rígida completa.

Inputs básicos:

```text
Throttle
Turn
```

O navio não deve conseguir girar instantaneamente no próprio eixo em velocidade máxima.

Movimento precisa gerar decisões de posicionamento.

---

# 18. Combate naval

O primeiro sistema de combate deve ser simples.

Não implementar:

* crew simulation;
* boarding;
* armor penetration complexa;
* dezenas de munições;
* habilidades mágicas.

O Vertical Slice precisa apenas de:

* weapon range;
* cooldown;
* firing arc;
* damage;
* projectile;
* HP;
* destruction.

---

# 19. Broadside

Recomendação de gameplay:

armamento principal deve favorecer disparo lateral.

Isso cria combate de posicionamento naval em vez de simplesmente perseguir mirando para frente.

Inicialmente:

* port broadside;
* starboard broadside;
* cooldown compartilhado ou independente conforme playtest;
* projectile server-authoritative.

O sistema deverá permanecer simples o suficiente para ajuste rápido.

---

# 20. Projectile

O projectile precisa possuir apenas:

```text
position
direction
speed
damage
owner_ship
lifetime
```

Servidor simula impacto.

Client interpola/apresenta.

Sem rigid body.

---

# 21. Destruição

Quando:

```text
current_hp <= 0
```

o servidor determina:

```text
ShipDestroyed
```

O jogador derrotado perde:

* ship hull;
* equipamentos equipados;
* cargo;
* consumíveis embarcados.

Nada embarcado permanece com o personagem.

Isso é a definição de full loot do mareForge.

### Pilar

Pilar 2.

---

# 22. Sink e loot não são a mesma coisa

Full loot significa:

> o derrotado perdeu tudo.

Isso não significa:

> o vencedor recebe tudo.

Parte dos itens deve desaparecer.

Isso evita crescimento infinito de estoque.

---

# 23. Regra inicial de destruição

Valores de tuning iniciais:

## Ship hull

**100% destruído.**

Nunca aparece em wreck.

---

## Equipped components

Aproximadamente:

**50% sobrevivem para wreck.**

**50% são destruídos.**

A seleção deve ser decidida pelo servidor.

---

## Cargo fungível

Inicialmente:

**80% sobrevive.**

**20% é destruído.**

Valores são configuração de balanceamento, não constantes espalhadas no código.

---

# 24. Determinismo

A resolução de loot deve ser reproduzível para testes e auditoria.

Sugestão:

```text
DestructionEventId
```

serve como seed da resolução de equipamentos.

A mesma entrada + seed deve gerar o mesmo resultado.

Nenhum RNG importante depende do client.

---

# 25. DestructionOutcome

A regra deve existir como domínio puro.

Conceito:

```text
resolve_ship_destruction(
    ship,
    equipment,
    cargo,
    policy,
    seed
) -> DestructionOutcome
```

Resultado conceitual:

```text
DestructionOutcome {
    destroyed_ship,
    destroyed_items,
    wreck_items,
}
```

Essa função não depende de Bevy.

### Pilares

2 e 4.

---

# 26. Wreck

Itens sobreviventes aparecem em um `Wreck`.

Wreck possui:

```text
WreckId
position
source_ship
exclusive_looter
exclusive_until
public_until
items
```

Configuração inicial:

* 45 segundos exclusivos ao killer;
* depois free-for-all;
* desaparece após 5 minutos.

Se grupos forem implementados no futuro, exclusividade poderá pertencer ao grupo.

Não implementar groups agora.

---

# 27. Loot

Loot não vai diretamente para conta ou storage.

O jogador precisa:

1. chegar ao wreck;
2. interagir;
3. possuir capacidade de carga;
4. transferir fisicamente o item para seu navio;
5. navegar de volta.

Portanto:

**ganhar a batalha não encerra o risco.**

O corsário que acabou de roubar carga agora virou transportador.

### Pilar

Fortalece fortemente o Pilar 2.

---

# 28. Cargo

Cargo possui limite de peso derivado de:

```text
ShipStats.cargo_capacity
```

Peso é calculado através das definições dos itens.

Operações devem falhar se ultrapassarem a capacidade.

Nunca clamp silenciosamente.

Resultado esperado:

```text
CargoCapacityExceeded
```

---

# 29. Localização de item

Hoje ownership e localização ainda estão muito próximos.

Precisamos separar conceitualmente:

**quem é dono** de **onde está**.

Proposta:

```text
ItemLocation
```

variantes conceituais:

```text
PortStorage(RegionId)
ShipCargo(ShipInstanceId)
MarketEscrow(MarketOrderId)
Wreck(WreckId)
```

No futuro podem existir outras.

---

# 30. Regra fundamental de storage

Storage é regional.

Itens armazenados em Port A não aparecem em Port B.

Não existe:

```text
GlobalStorage
```

Mover item entre portos exige:

```text
PortStorage
→ ShipCargo
→ oceano
→ ShipCargo
→ PortStorage
```

### Pilares

2 e 3.

---

# 31. Moeda

Gold permanece inicialmente uma carteira global do personagem.

Gold:

* não precisa ser carregado fisicamente;
* não é perdido quando um navio afunda.

O risco econômico está nos bens materiais embarcados.

Isso reduz complexidade sem quebrar a tese do transporte.

---

# 32. Recursos

Vertical Slice deve começar com catálogo pequeno.

Meta:

* 3 recursos crus principais;
* 1 recurso raro;
* alguns intermediários;
* poucos equipamentos.

Nomes são tuning/conteúdo e não contrato arquitetural.

Exemplo:

```text
Timber
IronOre
Fiber
RareMineral
```

---

# 33. Resource nodes

Recursos existem como nodes no mundo.

Node possui conceitualmente:

```text
ResourceNodeId
item_definition
remaining_quantity
gather_time
respawn_time
zone
```

Coleta exige:

* jogador em alcance;
* navio válido;
* espaço de cargo;
* node disponível.

---

# 34. Gathering

Fluxo:

```text
Approach node
→ Interact
→ gather channel
→ server validates
→ resource enters ShipCargo
```

Se cargo estiver cheio:

falha.

Não envia item para storage automaticamente.

### Pilares

1 e 2.

---

# 35. Respawn de recursos

Resource nodes possuem quantidade finita.

Após esgotados:

respawn após intervalo configurável.

Não precisamos implementar ecologia complexa.

O objetivo é gerar circulação pelo mapa.

---

# 36. Crafting

As estações já existentes permanecem como base:

```text
None
Workbench
Anvil
Dock
```

Não adicionar novos tipos até existir necessidade concreta.

---

# 37. Crafting de itens

Workbench e Anvil transformam recursos em:

* materiais intermediários;
* Hull equipment;
* Sail equipment;
* Weapon equipment;
* Aux equipment.

Crafting permanece fail-closed.

Qualquer:

* receita desconhecida;
* item desconhecido;
* station inválida;
* input insuficiente;

gera erro.

Nunca fallback.

### Pilares

1 e 4.

---

# 38. Construção de navios

O loop econômico não fecha se navios destruídos não precisarem ser reconstruídos.

Portanto o Vertical Slice DEVE suportar fabricação de navio.

Ship construction ocorre no:

```text
Dock
```

e consome recursos produzidos por jogadores.

Não modelar navio como `ItemDefinition`.

`ShipInstance` continua sendo entidade própria.

A camada de crafting deve ganhar uma operação específica de construção ou uma abstração de output capaz de produzir `ShipInstance`.

A implementação exata deve preservar boundaries de domínio.

---

# 39. Bootstrap do ambiente

Existe um problema inevitável de gênese:

para fabricar o primeiro navio, alguém precisa conseguir jogar.

O Vertical Slice NÃO resolverá onboarding de produção.

Test accounts podem receber através de seed/dev tooling:

* SmallMerchant inicial;
* Gold inicial;
* pequena quantidade de recursos.

Esse seed existe apenas para teste.

Não constitui uma mecânica oficial do jogo.

A solução definitiva para onboarding será decidida posteriormente.

---

# 40. Mercado regional

O mercado do Vertical Slice será:

**regional e sell-only.**

Não implementar buy orders ainda.

Isso aproveita o modelo atual e reduz o escopo.

---

# 41. Sell order

Jogador em Port A pode colocar item de Port A à venda.

Conceito:

```text
CreateSellOrder {
    market_region,
    item,
    quantity,
    unit_price,
}
```

O servidor valida tudo.

---

# 42. Market escrow

Quando sell order é criada:

item sai do storage disponível e entra em:

```text
MarketEscrow(order_id)
```

Isso impede:

* usar item enquanto vende;
* duplicação;
* vender item inexistente.

Criação da order + movimentação para escrow deve ser atômica.

---

# 43. Compra

Outro jogador presente na mesma região executa a order.

Operação precisa atomicamente:

1. validar order;
2. lockar order;
3. validar saldo;
4. remover dinheiro do comprador;
5. transferir dinheiro líquido ao vendedor;
6. transferir item;
7. atualizar filled quantity/status;
8. gravar ledger.

Não realizar qualquer metade da operação.

### Pilar

4.

---

# 44. Mercado não é global

Port A não cruza order com Port B.

`RegionId` é parte obrigatória da market order.

O preço pode divergir.

Isso é gameplay.

---

# 45. Market visibility

No Vertical Slice:

jogador só interage com o mercado do porto onde está.

Não implementar trading remoto.

Remote price intelligence poderá existir futuramente como sistema separado.

---

# 46. Listing fee

Primeiro currency sink:

**1% do valor total anunciado.**

Cobrado ao criar a order.

Não reembolsável.

Configuração:

```text
market_listing_fee_bps
```

---

# 47. Transaction tax

Segundo currency sink:

**3% do valor executado.**

Descontado dos proceeds do seller.

Configuração:

```text
market_transaction_tax_bps
```

Os números são tuning inicial.

---

# 48. Faucets

O Vertical Slice utilizará duas categorias.

## Development bootstrap

Gold inicial para contas de teste.

Não é design de produção.

## NPC bounty

Quando PvE entrar:

destruição de NPC poderá gerar Gold.

NPC NÃO gera:

* resource útil;
* equipment;
* ship;
* crafting material.

Isso preserva Pilar 1.

---

# 49. Item sinks

Principais:

* hull destruído;
* equipamentos destruídos no naufrágio;
* percentual de cargo perdido;
* futuramente durability/repair.

Item sink é diferente de currency sink.

Ambos precisam ser medidos separadamente.

---

# 50. Sessão do Merchant — 1 hora

Exemplo:

### 0–10

Abrir market local.

Avaliar storage.

Escolher mercadoria.

Equipar SmallMerchant.

### 10–25

Navegar para região produtora.

Coletar/comprar.

### 25–40

Transportar.

Avaliar risco e rotas.

### 40–50

Chegar ao porto destino.

Guardar ou vender.

### 50–60

Preparar viagem seguinte.

Sensação desejada:

> "Descobri uma oportunidade."

---

# 51. Sessão do Corsair — 1 hora

### 0–10

Equipar Corsair.

Escolher região.

### 10–25

Patrulhar rotas.

### 25–40

Encontrar e perseguir alvo.

### 40–45

Combate.

### 45–50

Loot.

### 50–60

Transportar loot para segurança.

Sensação desejada:

> "Eu sabia onde os comerciantes estavam passando."

---

# 52. Sessão do Crafter — 1 hora

### 0–15

Analisar oferta.

### 15–30

Comprar/coletar recursos.

### 30–40

Refinar/fabricar.

### 40–50

Criar equipamentos/navios.

### 50–60

Vender localmente ou preparar transporte para outro mercado.

Sensação:

> "Aqui existe demanda pelo que eu fabrico."

---

# 53. Patrol

Patrol não precisa ter um loop isolado artificial.

Seu gameplay nasce quando merchants oferecem:

* proteção;
* cooperação;
* escolta.

No Vertical Slice não precisamos implementar contratos formais de escolta.

Jogadores podem cooperar organicamente.

---

# 54. PvE

PvE não é requisito para provar a tese principal.

Ele pode entrar após PvP funcional.

NPC ship inicialmente precisa apenas:

* navegar;
* adquirir alvo;
* perseguir;
* atacar;
* morrer;
* conceder bounty em Gold.

Sem loot de item útil.

---

# 55. Conquista de território

Territory system NÃO entra no Vertical Slice.

Mas sua direção de design fica registrada.

Ilha controlável no futuro será:

**infraestrutura econômica.**

Pode conceder:

* crafting efficiency;
* market fees;
* dock services;
* resource advantage;
* treasury income.

Território não existe apenas para colocar nome da guilda no mapa.

### Pilares

1, 2 e 3.

---

# 56. Domínios

Crates existentes permanecem.

Adicionar novos bounded contexts somente quando necessários.

Recomendação para o slice:

```text
shared
protocol
domain-items
domain-ships
domain-crafting
domain-economy
domain-world      ← novo quando Phase de mundo iniciar
domain-combat     ← novo quando Phase de combate iniciar
server
client
```

Não criar microservices.

Continuar modular monolith.

---

# 57. domain-world

Responsável por regras puras de:

```text
Region
Zone
RiskTier
RiskPolicy
Port
ResourceNode
```

Não depende de Bevy.

---

# 58. domain-combat

Responsável por:

```text
Damage
Weapon
Cooldown
Projectile rules
ShipDestruction
DestructionOutcome
WreckPolicy
```

Não depende de Bevy.

---

# 59. domain-items

Responsável por:

* ItemDefinition;
* ItemInstance;
* stack;
* equipment definitions;
* peso;
* ItemLocation/custody rules quando apropriado.

Não colocar movimentação de mundo aqui.

---

# 60. domain-economy

Responsável por:

* Money;
* MarketOrder;
* market rules;
* transaction;
* fees;
* ledger semantics.

Market permanece regional.

---

# 61. ECS boundary

Bevy ECS deve possuir apenas:

* estado runtime;
* componentes replicados;
* posição;
* visual/runtime state;
* routing para funções de domínio.

Exemplo incorreto:

```text
Bevy system decide quanto loot sobrevive.
```

Exemplo correto:

```text
Bevy system detecta HP zero
→ chama domain-combat
→ recebe DestructionOutcome
→ server aplica ownership transaction
→ ECS replica resultado
```

---

# 62. Networking

A rede continua usando server-authoritative architecture.

O `protocol` atualmente vazio começa a ganhar responsabilidades a partir da Phase multiplayer.

Client envia intenção.

Servidor envia realidade.

---

# 63. Commands iniciais

Conceitualmente:

```text
ShipInput
FireWeapon
Interact
GatherResource
Dock
Undock
TransferItem
CraftItem
ConstructShip
CreateSellOrder
CancelSellOrder
BuySellOrder
LootWreck
```

---

# 64. Estado/eventos de servidor

Conceitualmente:

```text
ShipSpawned
ShipState
ProjectileSpawned
DamageApplied
ShipDestroyed
WreckSpawned
WreckUpdated
CargoUpdated
ZoneChanged
ResourceNodeUpdated
CraftCompleted
MarketOrderUpdated
TransactionExecuted
```

Protocol types não devem depender de representação visual.

---

# 65. Tick

Contrato existente:

```text
simulation = 30 Hz
network snapshots = 20 Hz
render = desacoplado
```

A implementação atual de 10 Hz deve ser corrigida quando o loop multiplayer real for montado.

Não alterar ADR.

---

# 66. AOI

Manter decisão existente:

* grid 256m;
* chunks visíveis;
* um anel adicional;
* last-known-state para entidade distante.

Não otimizar antes de benchmark.

---

# 67. Persistência

Estado persistente:

* character;
* owned ships;
* equipment;
* item ownership;
* item location;
* regional storage;
* wallet;
* market orders;
* ledger.

Estado efêmero:

* projectiles;
* cooldown runtime;
* temporary wreck visual state;
* interpolation buffers.

Wreck economic contents precisam ser recuperáveis/consistentes durante o período em que existirem.

---

# 68. Ledger

Eventos econômicos importantes precisam ser auditáveis.

Exemplos:

```text
ItemGathered
ItemCrafted
ShipConstructed
ItemMovedToCargo
ItemMovedToStorage
ItemEscrowed
MarketTradeExecuted
ItemDestroyed
ShipDestroyed
ItemMovedToWreck
ItemLooted
CurrencyMinted
CurrencyBurned
```

Os nomes finais podem mudar.

A semântica não.

---

# 69. Fail-closed

Qualquer estado desconhecido economicamente relevante deve falhar.

Exemplos:

```text
UnknownItem
UnknownShipDefinition
UnknownRecipe
UnknownRegion
UnknownZone
UnknownMarket
UnknownWreck
UnknownResourceNode
UnknownStation
```

Nenhum default mágico.

---

# 70. Concorrência

Operações econômicas críticas precisam sobreviver a:

* double click;
* retries;
* requests simultâneas;
* disconnect;
* reconnect;
* duas pessoas tentando comprar a última unidade;
* duas pessoas tentando lootear o mesmo wreck.

Apenas uma operação vence.

---

# 71. Observabilidade econômica

Medir desde o Vertical Slice:

```text
gold_minted
gold_burned
items_gathered
items_crafted
items_destroyed
ships_constructed
ships_destroyed
market_volume
market_tax_burned
listing_fee_burned
cargo_value_destroyed
cargo_value_looted
```

---

# 72. Observabilidade de gameplay

Também medir:

```text
players_per_zone
pvp_engagements
ship_losses_by_kind
average_trip_duration
average_cargo_value
merchant_deaths
corsair_deaths
wrecks_looted
route_usage
```

---

# 73. Hipóteses que queremos validar

## H1

Diferença regional de recursos gera transporte.

## H2

Transporte com full loot gera predadores.

## H3

Predadores criam demanda por escolta.

## H4

Destruição gera demanda recorrente por crafting.

## H5

Merchant, Corsair e Patrol apresentam identidades diferentes sem classes.

## H6

Jogadores alteram rota quando carregam mais riqueza.

---

# 74. Critérios de sucesso de produto

O Vertical Slice é considerado promissor se playtests mostrarem pelo menos:

* jogadores realizando viagens motivadas por diferença econômica;
* jogadores evitando regiões quando carga é valiosa;
* PvP ocorrendo em rotas economicamente relevantes;
* loot sendo transportado novamente após PvP;
* itens destruídos gerando nova demanda;
* diferença perceptível entre os três navios;
* preços regionalmente distintos.

Não precisamos de balanceamento perfeito.

Precisamos de comportamento emergente.

---

# 75. Critérios técnicos

O Vertical Slice deve:

* suportar 2–10 jogadores;
* manter servidor autoritativo;
* simular a 30 Hz;
* replicar sem enviar o mundo inteiro;
* não permitir duplicação conhecida;
* preservar ledger;
* suportar reconnect sem criar item;
* ter regras econômicas testáveis sem Bevy;
* manter workspace verde.

---

# 76. Definition of Done de qualquer task GLM

Antes de devolver uma implementação:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

precisam passar.

Se algum comando não puder passar, GLM deve explicar exatamente por quê.

---

# 77. Regra para o GLM

GLM pode tomar decisões locais de implementação.

GLM NÃO pode unilateralmente alterar:

* arquitetura de crates;
* ADR;
* protocolo fundamental;
* regra econômica;
* full loot;
* market model;
* risk model;
* ownership;
* persistência crítica.

Ao encontrar necessidade desse tipo:

**STOP → RFC para arquiteto.**

---

# 78. Sequência de implementação

## Phase 1 — Ship Foundation

Objetivo:

navio divertido de controlar localmente.

Inclui:

* rich equipment definitions;
* ship stats;
* movimento;
* câmera;
* collision básica;
* SmallMerchant/Patrol/Corsair.

---

## Phase 2 — Multiplayer Authority

Objetivo:

dois jogadores veem e controlam navios através do servidor.

Inclui:

* 30 Hz;
* protocol;
* input;
* replication;
* interpolation;
* AOI;
* basic reconnect.

---

## Phase 3 — Combat

Objetivo:

jogadores conseguem afundar um ao outro.

Inclui:

* weapons;
* broadside;
* projectiles;
* cooldown;
* damage;
* destruction.

---

## Phase 4 — Cargo + Full Loot

Objetivo:

combate passa a possuir consequência econômica.

Inclui:

* cargo;
* weight;
* ItemLocation;
* DestructionOutcome;
* wreck;
* loot;
* ownership transfer.

Este é o primeiro momento em que o Pilar 2 realmente existe.

---

## Phase 5 — World Risk

Objetivo:

criar geografia econômica.

Inclui:

* regions;
* ports;
* zones;
* Protected;
* Frontier;
* Lawless;
* risk UI.

---

## Phase 6 — Resource Economy

Objetivo:

jogadores criarem matéria-prima.

Inclui:

* resource nodes;
* gathering;
* respawn;
* regional resource distribution.

---

## Phase 7 — Crafting Economy

Objetivo:

transformar recursos em propriedade.

Inclui:

* equipment;
* intermediate materials;
* ship construction;
* stations.

---

## Phase 8 — Regional Market

Objetivo:

criar arbitragem.

Inclui:

* regional storage;
* sell orders;
* escrow;
* purchase;
* listing fee;
* transaction tax.

---

## Phase 9 — Persistence + Ledger Hardening

Objetivo:

sobreviver a restart/reconnect/concurrency.

Inclui:

* persistence;
* transactional mutation;
* ledger;
* recovery;
* economic audit tests.

---

## Phase 10 — Vertical Slice Integration

Objetivo:

fechar:

```text
collect
→ craft
→ transport
→ fight
→ lose/loot
→ sell
→ repeat
```

---

# 79. Fila inicial para o GLM

## MF-001 — Rich Equipment Definitions

Mover modifiers econômicos/combatentes para definições ricas em `domain-items`.

Eliminar modifiers como verdade persistida dentro de `EquippedComponent`.

**Pilar:** 5.

Dependência: nenhuma.

---

## MF-002 — Pure Ship Motion Model

Criar modelo puro para heading, throttle, speed e turn.

Sem Bevy nas regras.

**Pilares:** 2, 5.

Dependência: MF-001 opcional.

---

## MF-003 — Client Local Ship Controller

Integrar motion ao client Bevy.

Placeholder visual aceitável.

**Pilares:** 2, 5.

Dependência: MF-002.

---

## MF-004 — Server Tick Alignment

Substituir smoke 10 Hz pelo contrato de 30 Hz no loop real.

Preservar `TickLimit` para testes.

**Pilar:** 4.

Dependência: início da Phase 2.

---

## MF-005 — Protocol Foundation

Criar protocol version + handshake + `ShipInput`.

Seguir ADR-0011.

**Pilar:** 4.

Dependência: MF-002.

---

## MF-006 — Authoritative Ship Replication

Client envia input.

Server calcula movimento.

Client recebe estado.

**Pilares:** 2, 4.

Dependência: MF-004, MF-005.

---

## MF-007 — AOI Grid

Aplicar chunks 256m + border ring.

**Pilar:** 4.

Dependência: MF-006.

---

## MF-008 — domain-combat Bootstrap

Criar crate puro com weapon/damage/cooldown primitives.

**Pilares:** 2, 4.

Dependência: nenhuma.

---

## MF-009 — Broadside Combat

Port/starboard fire + projectile simulation.

**Pilares:** 2, 5.

Dependência: MF-006, MF-008.

---

## MF-010 — Ship Destruction

HP zero → `ShipDestroyed`.

Sem loot ainda.

**Pilar:** 2.

Dependência: MF-009.

---

## MF-011 — Item Location Model

Introduzir localização física de propriedade.

Suportar:

* port;
* cargo;
* escrow;
* wreck.

**Pilares:** 2, 4.

Dependência: nenhuma.

---

## MF-012 — Cargo Capacity

Carga baseada em peso e ShipStats.

Fail-closed.

**Pilares:** 2, 5.

Dependência: MF-011.

---

## MF-013 — Full Loot Resolution

Implementar `DestructionOutcome`.

Hull 100% sink.

Equipment/cargo conforme policy configurável.

**Pilares:** 1, 2, 4.

Dependência: MF-010, MF-011.

---

## MF-014 — Wreck

Spawn econômico + runtime.

Exclusive window.

FFA.

Expiration.

**Pilares:** 2, 4.

Dependência: MF-013.

---

## MF-015 — Loot Transfer

Wreck → ShipCargo.

Atômico e capacity-aware.

**Pilares:** 2, 4.

Dependência: MF-012, MF-014.

---

## MF-016 — domain-world Bootstrap

Region, Zone, RiskTier, RiskPolicy, Port.

**Pilar:** 3.

Dependência: nenhuma.

---

## MF-017 — Risk Zone Integration

Server calcula zona.

Client recebe mudança.

UI sinaliza risco.

**Pilares:** 3, 4.

Dependência: MF-016, MF-006.

---

## MF-018 — Resource Nodes

Nodes server-authoritative.

**Pilares:** 1, 3, 4.

Dependência: MF-016.

---

## MF-019 — Gathering

Node → ShipCargo.

**Pilares:** 1, 2.

Dependência: MF-012, MF-018.

---

## MF-020 — Regional Resource Distribution

Port A / Port B / Lawless possuem disponibilidade distinta.

**Pilares:** 2, 3.

Dependência: MF-018.

---

## MF-021 — Equipment Crafting

Recursos → equipment.

Usar stations existentes.

**Pilares:** 1, 5.

Dependência: MF-001.

---

## MF-022 — Ship Construction

Dock + recursos → ShipInstance.

**Pilares:** 1, 2, 5.

Dependência: crafting foundation.

---

## MF-023 — Regional Port Storage

Storage separado por RegionId.

Sem global storage.

**Pilares:** 2, 3.

Dependência: MF-011, MF-016.

---

## MF-024 — Market Escrow

Sell order move item para escrow atomicamente.

**Pilares:** 3, 4.

Dependência: MF-011, domain-economy existente.

---

## MF-025 — Regional Sell Market

Listar e comprar sell orders somente da região.

**Pilares:** 2, 3, 4.

Dependência: MF-024.

---

## MF-026 — Market Sinks

Listing fee + transaction tax.

Ledger obrigatório.

**Pilares:** 1, 4.

Dependência: MF-025.

---

## MF-027 — Persistence Hardening

Persistir ownership/location/market/wreck-critical-state.

**Pilar:** 4.

Dependência: domínios estabilizados.

---

## MF-028 — Concurrency Tests

Testar:

* double buy;
* double loot;
* double craft;
* retry;
* disconnect.

**Pilar:** 4.

Dependência: economy + loot.

---

## MF-029 — Economic Telemetry

Instrumentar faucets/sinks/volume/loss.

**Pilares:** 1, 2, 3.

Dependência: economy.

---

## MF-030 — End-to-End Vertical Slice

Provar:

```text
Player A gathers
→ crafts
→ transports
→ Player B attacks
→ ship sinks
→ loot transfers
→ Player B returns
→ sells
→ economy records everything
```

**Pilares:** todos.

Dependência: todas as essenciais anteriores.

---

# 80. Primeira milestone jogável

Antes de implementar economia, precisamos conseguir abrir duas instâncias do client e observar:

```text
Player A
      \
       → authoritative server
      /
Player B
```

ambos:

* movendo navios;
* vendo o outro;
* com movimento consistente;
* sem client controlar verdade.

Isso é milestone:

**M1 — Two Ships at Sea.**

---

# 81. Segunda milestone

```text
Two Ships at Sea
→ combat
→ one sinks
```

Nome:

**M2 — First Blood.**

---

# 82. Terceira milestone

```text
ship sinks
→ wreck
→ loot
→ cargo changes owner
```

Nome:

**M3 — Risk Has Value.**

Aqui nasce o mareForge de verdade.

---

# 83. Quarta milestone

```text
gather
→ craft
→ cargo
→ transport
```

Nome:

**M4 — Wealth Moves.**

---

# 84. Quinta milestone

```text
Port A price != Port B price
→ player sees opportunity
→ transports
→ profit
```

Nome:

**M5 — The Market Breathes.**

---

# 85. Vertical Slice completo

Quando:

```text
M1
+
M2
+
M3
+
M4
+
M5
```

estiverem funcionando em uma única sessão multiplayer:

**Vertical Slice 1 está concluído.**

---

# 86. Regra final de produto

Sempre que alguém sugerir uma feature, perguntar:

> Isso cria riqueza?

> Isso move riqueza?

> Isso coloca riqueza em risco?

> Isso destrói riqueza?

> Isso cria motivo para outro jogador interagir?

Se a resposta for "nenhuma":

provavelmente não pertence às prioridades atuais do mareForge.

O objetivo não é construir muito conteúdo.

O objetivo é construir sistemas tão bons que os próprios jogadores virem o conteúdo.
