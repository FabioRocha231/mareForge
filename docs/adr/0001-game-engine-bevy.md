## Status
proposed

## Context

O projeto precisa de uma engine Rust para cliente e servidor autoritativo. A engine precisa oferecer ECS maduro, ecossistema ativo e suporte a execução headless no servidor.

O time quer evitar custo alto de manutenção, mistura de linguagens e dependência de ferramentas com ecossistema menos adequado para um cliente rico.

## Decision

Usar Bevy na versão do último minor estável no momento da execução em cliente e servidor. No servidor, usar Bevy em modo headless, sem plugin de renderização.

## Alternatives considered

- Roll-our-own ECS: custo alto para implementar e manter.
- Godot com GDExtension: mistura linguagens e complica o servidor Rust.
- Fyrox: menos maduro para cliente rico.
- Renúncia a engine: esforço proibitivo para o escopo do jogo.

## Consequences

- ECS forte disponível nas duas plataformas.
- Sem editor visual maduro.
- Dependência de breaking changes entre minors.
- Lock-in em Bevy para partes da arquitetura.
