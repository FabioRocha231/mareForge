## Status
proposed

## Context

O servidor não deve replicar todas as entidades para todos os clientes. A banda disponível é limitada.

## Decision

Usar grid espacial no servidor com chunks de 256m. O cliente recebe entidades dos chunks visíveis mais um anel de borda. Entidades distantes permanecem como last-known-state até expirar.

## Alternatives considered

- Replicar tudo: inviável por banda.
- Circle AOI dinâmica: custo de manutenção maior.
- Quadtree sob demanda: overhead sem ganho no início.

## Consequences

- Banda previsível.
- Tamanho do chunk é uma métrica a validar.
- Implementação simples.
