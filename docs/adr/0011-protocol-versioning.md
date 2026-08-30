## Status
proposed

## Context

Cliente e servidor evoluem em ritmos diferentes. Incompatibilidade de protocolo pode quebrar sessões sem diagnóstico claro.

## Decision

Incluir `protocol_version: u16` no handshake. O servidor rejeita mismatch com erro claro. Novas mensagens usam discriminadores versionados.

## Alternatives considered

- Versionamento por feature flag: complexidade.
- Ausência de versionamento: quebra silenciosa.
- Versionamento por string: parse lento e desnecessário.

## Consequences

- Contratos explícitos entre cliente e servidor.
- Migrações exigem bump coordenado.
- Facilita deploy progressivo.
