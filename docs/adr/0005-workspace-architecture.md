## Status
proposed

## Context

O projeto precisa separar domínios sem pagar o custo operacional de microservices.

## Decision

Usar um workspace Rust multi-crate com `protocol`, `shared`, `domain-*`, `server`, `client` e `tools`, com regras estritas de dependência.

## Alternatives considered

- Monorepo único: sem fronteiras de compilação.
- Microservices: overhead operacional alto.
- Monólito com módulos internos: sem compile-time enforcement.

## Consequences

- Fronteiras validadas em compile time.
- Testes isolados por crate.
- Mais arquivos de manifesto.
- Pronto para extração futura de crates.
