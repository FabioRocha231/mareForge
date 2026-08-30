## Status
proposed

## Context

Economia e personagens são dados críticos e precisam de transações ACID. O time quer queries tipadas e integração madura com o ecossistema Rust.

## Decision

Usar PostgreSQL com sqlx para queries verificadas em compile time e migrations versionadas.

## Alternatives considered

- SQLite: limite de concorrência para o jogo online.
- MongoDB: sem ACID forte para ledger.
- Sled: imaturidade para uso em produção.
- Redis como primário: efêmero por natureza.

## Consequences

- Dependência de PostgreSQL em dev e produção.
- Schema migrations passam a ser obrigatórias.
- Ganho em integridade referencial.
