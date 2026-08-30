## Status
proposed

## Context

O ECS não deve abrigar regras de negócio. Testes de domínio precisam rodar sem render.

## Decision

Manter lógica de negócio em crates `domain-*` sem dependência de Bevy. O ECS armazena estado replicado e roteia chamadas para funções de domínio.

## Alternatives considered

- Lógica dentro de systems: acoplamento com Bevy.
- Arquitetura reativa externa: complexidade adicional.
- ECS para tudo: testes pesados e acoplados à engine.

## Consequences

- Testes rápidos e determinísticos de domínio.
- Maior número de crates.
- Boundary explícita entre domínio e engine.
