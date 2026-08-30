## Status
proposed

## Context

O jogo precisa de replicação autoritativa com prediction e reconciliation. A camada de networking deve ser escrita em Rust e integrar com Bevy.

Alternativas muito baixo nível exigiriam implementar prediction e reconciliação internamente.

## Decision

Usar Lightyear como camada de networking no servidor e no cliente.

## Alternatives considered

- Renet: mais baixo nível e exigiria implementar prediction.
- Laminar: muito baixo nível para o escopo atual.
- QUIC custom: esforço grande para a necessidade atual.
- WebSockets puros: sem suporte nativo a canal unreliable.

## Consequences

- Atalhos para servidor autoritativo com prediction.
- API ainda instável entre versões.
- Exige abstração fina para permitir troca futura.
