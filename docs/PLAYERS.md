# Marvyr — guia rápido para jogadores

## Instalar

1. Baixe o pacote do Marvyr para o seu sistema (Windows, Linux ou macOS) na
   página do jogo no itch.io (ou pelo app do itch, que já atualiza sozinho).
   Os passos abaixo são do Windows; Linux e macOS logo em seguida.
2. Extraia o zip **inteiro** para uma pasta. Não rode de dentro do zip: o jogo
   precisa da pasta `assets` ao lado do executável.
3. Abra `Marvyr.exe`. Se o SmartScreen avisar, clique em
   **Mais informações → Executar assim mesmo** (o alpha ainda não é assinado).

**Linux**: extraia o `.tar.gz` e rode `./Marvyr` na pasta. Precisa de driver
de vídeo com Vulkan.

**macOS** (Apple Silicon ou Intel): extraia o zip e arraste `Marvyr.app` para
Aplicativos. Na primeira vez, clique com o botão direito → **Abrir** (o alpha
ainda não é notarizado). Se aparecer "está danificado", rode no Terminal
`xattr -dr com.apple.quarantine /Applications/Marvyr.app`.

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

Aperte **F1** (ou **Esc** no mar) para abrir o livreto do marujo: ele lista
todos os controles, explica os rótulos do mar e tem as opções. O bilhete de
ação no rodapé sempre mostra a tecla do que dá para fazer agora (atracar,
coletar, saquear, abordar, cavar).

Na primeira viagem, um guia de seis passos acompanha você: zarpar, coletar,
atracar, vender ou guardar, fabricar e levar carga a outro porto. Dá para
recomeçar o guia em **Livreto → Opções**.

Controle (gamepad) funciona: analógico esquerdo ou direcional para velas e
leme, **A** para a ação do bilhete, **LB/RB** para os canhões e **Start** para
o livreto.

## Idioma

O jogo tem português e inglês. Aperte **L** no mar para trocar, ou use o botão
na tela de entrada. A escolha fica salva.
