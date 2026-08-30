## Status
proposed

## Context

Combate naval é lento e o mundo é essencialmente 2D. Física rígida completa não é necessária para o estágio atual.

## Decision

Implementar física custom 2D com heading, velocidade escalar e colisão axial para navios. Avian só será adotada se complexidade futura justificar.

## Alternatives considered

- Avian 2D: overhead para a necessidade atual.
- Rapier 3D: complexidade extra desnecessária.
- Kinematic simples sem colisão: insuficiente para combate.

## Consequences

- Implementação leve e determinística.
- Sem rigid bodies.
- ADR permanece aberto para upgrade se necessário.
