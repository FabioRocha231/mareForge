## Status
proposed

## Context

O jogo é competitivo e o cliente não é uma fonte confiável de estado. A trust boundary é o cliente.

## Decision

O servidor é autoritativo para posição, dano, inventário e economia. O cliente envia intenções como input commands e recebe resultados replicados.

## Alternatives considered

- Cliente autoritativo: inviável para jogo competitivo.
- Autoridade mista por categoria: risco de inconsistência entre estados.
- Lockstep puro: lento para 10 jogadores.

## Consequences

- Latência percebida é mitigada com client prediction.
- Servidor precisa validar todas as intenções.
- Testes anti-cheat ficam centralizados no servidor.
