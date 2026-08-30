## Status
proposed

## Context

Economia é crítica e duplicação de itens ou dinheiro seria fatal. Toda mutação precisa ser auditável e consistente.

## Decision

Manter ledger imutável de transações com saldos e inventário derivados. Transações usam `SELECT ... FOR UPDATE` em linhas críticas e `BEGIN/COMMIT` explícito.

## Alternatives considered

- UPDATE direto sem ledger: sem auditabilidade.
- Event sourcing completo: overhead para o estágio atual.
- Two-phase commit: complexidade desnecessária.

## Consequences

- Auditabilidade completa das mutações.
- Performance depende de índices.
- Recálculo de saldos sempre é possível a partir do ledger.
