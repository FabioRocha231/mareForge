# Deploy e release do Marvyr

Guia operacional do alpha público: servidor no Dokploy, cliente Windows via
GitHub Actions + itch.io.

## Topologia (Dokploy)

Um projeto Dokploy **Marvyr** com três serviços na mesma rede interna:

| Serviço | Origem | Exposição |
|---|---|---|
| `marvyr-server` | Application, `Dockerfile.server` (raiz do repo) | UDP `5000` publicado direto no host (**fora do Traefik**) |
| `marvyr-auth` | Application, `Dockerfile.auth` (raiz do repo) | HTTPS via Traefik: `auth.marvyr.game` → container `8080` |
| `marvyr-db` | Database PostgreSQL | só rede interna; **nunca** publicar `5432` |

```
jogador ──UDP 5000──────────────▶ marvyr-server ─┐
jogador ──HTTPS 443─▶ Traefik ──▶ marvyr-auth ───┼──▶ marvyr-db (rede interna)
                                                 │
                     MARVYR_JWT_SECRET compartilhado
```

### Porta UDP do servidor

O Traefik do Dokploy só roteia HTTP/TCP, então o jogo **não** passa por ele.
Em `marvyr-server` → *Advanced* → *Ports*: publicada `5000`, alvo `5000`,
protocolo **UDP** (modo de publicação `host` se disponível, para o IP do
cliente chegar intacto). Não configure domínio para esse app.

### DNS

- `A play.marvyr.game` → IP público da VPS (endereço que os clientes usam, `play.marvyr.game:5000`).
- `A auth.marvyr.game` → mesmo IP (Traefik emite o certificado Let's Encrypt).

### Firewall da VPS

| Porta | Abrir? |
|---|---|
| `5000/udp` | sim (jogo) |
| `80/tcp`, `443/tcp` | sim (Traefik/ACME) |
| `22/tcp` | sim, de preferência restrito ao seu IP |
| `3000/tcp` (painel Dokploy) | só se necessário; melhor via domínio HTTPS |
| `5432/tcp` | **nunca** |

Atenção: o Docker escreve regras de iptables que ignoram o `ufw`. Por isso a
única defesa real para o Postgres é **não publicar a porta**.

## Variáveis de ambiente

### `marvyr-server`

| Variável | Valor em produção | Notas |
|---|---|---|
| `MARVYR_PORT` | `5000` | porta UDP (padrão 5000) |
| `MARVYR_ENV` | `production` | `development` só local |
| `MARVYR_DATABASE_URL` | `postgres://marvyr:<senha>@marvyr-db:5432/marvyr` | fail-closed: se definida e o banco não abrir, o servidor sai |
| `MARVYR_JWT_SECRET` | 64 caracteres hex | ≥ 32 bytes, **igual** ao do auth; obrigatório em produção |
| `MARVYR_ALLOW_ANON` | *(não definir)* | `1` aceita identidade anônima — só dev, **nunca** em produção |
| `MARVYR_MAX_CLIENTS` | `64` | padrão 64 |
| `RUST_LOG` | `info,marvyr_server=info` | |
| `MARVYR_REPORT_DIR` | `/data/reports` | `session-summary.json` é gravado aqui no desligamento |
| `MARVYR_SEA_EVENT` | *(não definir)* | `tempest`/`fleet`/`kraken`/`tide` força um evento de mar no boot — só teste |

Monte um volume persistente em `/data` (relatórios de sessão).

### `marvyr-auth`

| Variável | Valor em produção | Notas |
|---|---|---|
| `MARVYR_AUTH_ADDR` | `0.0.0.0:8080` | padrão |
| `MARVYR_JWT_SECRET` | mesmo do servidor | |
| `MARVYR_DATABASE_URL` | mesma string do servidor | obrigatório; as migrations rodam no boot |
| `MARVYR_TOKEN_TTL_SECS` | `604800` | validade do token (padrão 7 dias) |
| `MARVYR_TRUST_PROXY` | `1` atrás do Traefik | usa a entrada mais à direita do `X-Forwarded-For` (a que o Traefik anexa) no limite de tentativas; supõe **um** proxy na frente |
| `RUST_LOG` | `info` | |

### Cliente (`Marvyr.exe`)

Prioridade, da mais forte para a mais fraca:

1. CLI: `--server host:porta`, `--auth-url URL`
2. Ambiente: `MARVYR_SERVER_HOST` + `MARVYR_PORT`, `MARVYR_AUTH_URL`
3. Arquivo `marvyr.toml` ao lado do exe: `server = "host:porta"`, `auth_url = "https://..."`
4. Padrões gravados no build: `MARVYR_DEFAULT_SERVER`, `MARVYR_DEFAULT_AUTH_URL`
5. `localhost` — **só** em build de dev

Variáveis de **build** (lidas pelo `cargo build`, não em runtime):

| Variável | Uso |
|---|---|
| `MARVYR_PUBLIC_BUILD=1` | build público: sem fallback para localhost (mostra "Servidor do Marvyr não configurado") |
| `MARVYR_DEFAULT_SERVER` / `MARVYR_DEFAULT_AUTH_URL` | endereços padrão gravados no exe |
| `MARVYR_BUILD_SHA` / `MARVYR_VERSION_LABEL` | metadados exibidos pelo jogo |

Dados do jogador: `%APPDATA%\Marvyr` (Windows), `~/Library/Application Support/Marvyr` (macOS), `~/.local/share/Marvyr` (Linux).

## Segredo JWT

```sh
openssl rand -hex 32
```

Cole o mesmo valor em `MARVYR_JWT_SECRET` de `marvyr-server` e `marvyr-auth`
(Environment do Dokploy, nunca no repositório). Trocar o segredo invalida todas
as sessões: reinicie os dois serviços juntos.

## PostgreSQL

- Serviço Database do Dokploy, sem porta externa. Os apps conectam por `marvyr-db:5432`.
- As migrations rodam sozinhas no boot do servidor (embutidas via `sqlx::migrate!`).
- **Backups**: em *Backups* do banco, agende um backup diário (ex.: `0 4 * * *`)
  para um destino S3 (*Settings → S3 Destinations*) com retenção de pelo menos
  7 diários. Sem S3, faça `pg_dump` diário por cron para fora da VPS.
- Teste a restauração ao menos uma vez antes de abrir o alpha.

## Operação

### Reinício ordenado

O servidor persiste o estado ao receber `SIGTERM` (`STOPSIGNAL SIGTERM` no
Dockerfile). O Docker espera 10 s por padrão antes do `SIGKILL`; aumente o
grace period do serviço (Swarm `StopGracePeriod`, ex.: 30 s) se o desligamento
passar disso.

1. Avise os jogadores (reinício em N minutos).
2. Dokploy → `marvyr-server` → *Stop* (ou *Deploy* para uma nova versão).
3. Confira no log que o estado foi salvo e que `session-summary.json` foi escrito em `/data/reports`.
4. *Start*/aguarde o deploy e confirme a linha de escuta na porta 5000.

Nunca use `docker kill` / `SIGKILL` em produção: perde o que não foi persistido.

### Logs

- Dokploy → app → aba *Logs* (tempo real).
- Na VPS: `docker service logs -f --tail 200 <serviço>` (Swarm) ou `docker logs -f --tail 200 <container>`.
- Mais detalhe temporário: `RUST_LOG=debug,marvyr_server=debug` e redeploy (volte ao normal depois).

## Release do cliente

1. Garanta que `main` está verde no CI.
2. Configure uma vez no GitHub (*Settings → Secrets and variables → Actions*):
   - variáveis: `MARVYR_DEFAULT_SERVER` (`play.marvyr.game:5000`),
     `MARVYR_DEFAULT_AUTH_URL` (`https://auth.marvyr.game`),
     `ITCH_TARGET` (`usuario/marvyr`; vazio desativa o itch);
   - secret: `BUTLER_API_KEY` (`butler login` → chave em itch.io → *API keys*);
   - environment `itch` com *required reviewers* para aprovar o envio.
3. Crie e envie a tag:
   ```sh
   git tag v0.1.0-alpha.1
   git push origin v0.1.0-alpha.1
   ```
4. O workflow `Release` roda `checks` → `windows` (executa
   `scripts/package_windows.sh`, publica o artefato `Marvyr-windows` e anexa o
   zip a um GitHub Release) → `itch` (após aprovação, `butler push` do
   `dist/windows` no canal `windows`).
5. Sem tag: *Actions → Release → Run workflow* com `version` gera só o artefato.

Empacotar localmente (Windows com Git Bash): `bash scripts/package_windows.sh`.
O script é a fonte única do conteúdo do zip: `Marvyr.exe`, `assets/`
(`marvyr`, `external`, `shaders`; `dev` fica fora), `LICENSE`,
`ATTRIBUTION.md`, `VERSION` e `README.txt`.

Ferramenta de playtest em dev (não distribuída): `cargo run --bin marvyr_playtest --release`.

## Testes de aceitação manuais

Antes de anunciar uma versão:

- [ ] **Local**: servidor + cliente na mesma máquina conectam, criam conta e jogam.
- [ ] **LAN**: cliente em outra máquina da rede conecta via `--server <ip-lan>:5000`.
- [ ] **Internet**: cliente fora da rede conecta em `play.marvyr.game:5000` com o build público, sem argumentos.
- [ ] **3 clientes** simultâneos se veem e interagem sem desync visível.
- [ ] **Persistência no reinício**: jogar, fazer *Stop/Start* do servidor, reconectar e encontrar navio/inventário/ouro como antes.
- [ ] **Máquina limpa**: extrair o zip num Windows sem Rust/VS instalados, abrir `Marvyr.exe`, criar conta e jogar.
- [ ] **Versão incompatível**: cliente com `PROTOCOL_VERSION` diferente recebe mensagem clara, sem crash.
- [ ] **Servidor offline**: com o servidor parado, o cliente mostra erro de conexão legível e não trava.
