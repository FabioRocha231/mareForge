## Status
proposed

## Context

Combate naval é lento, mas simulação e rede precisam equilibrar responsividade e banda.

## Decision

Usar simulação de servidor a 30 Hz, snapshots de rede a 20 Hz e render desacoplado com interpolação.

## Alternatives considered

- 60 Hz de simulação: CPU alta e banda alta.
- 10 Hz de simulação: lag visível.
- Tick variável por região: complexidade sem necessidade atual.

## Consequences

- Latência percebida aceitável até aproximadamente 100ms de RTT.
- Banda orçada abaixo de 20 KB/s por cliente.
- Valores ajustáveis depois com benchmarks.
