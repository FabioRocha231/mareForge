# Marvyr — guia rápido para jogadores

## Instalar

1. Baixe o zip do Marvyr para Windows na página do jogo no itch.io
   (ou pelo app do itch, que já atualiza sozinho).
2. Extraia o zip **inteiro** para uma pasta. Não rode de dentro do zip: o jogo
   precisa da pasta `assets` ao lado do executável.
3. Abra `Marvyr.exe`. Se o SmartScreen avisar, clique em
   **Mais informações → Executar assim mesmo** (o alpha ainda não é assinado).

## Conta

Na primeira abertura, crie sua conta na tela inicial. Nas próximas vezes é só
entrar. Sua identidade e sessão ficam em `%APPDATA%\Marvyr`; apagar essa pasta
faz o jogo esquecer o login neste computador.

## Problemas

- **"Servidor do Marvyr não configurado"**: o build não sabe a qual servidor
  conectar. Crie `marvyr.toml` ao lado do `Marvyr.exe` com
  `server = "host:5000"` e `auth_url = "https://..."`.
- **Versão incompatível**: baixe a versão mais recente.
- **Bug**: rode `Marvyr.exe > marvyr.log 2>&1` num Prompt de Comando na pasta
  do jogo e envie o `marvyr.log` junto com o arquivo `VERSION`.

## Controles

Veja a seção de controles no [README](../README.md).
