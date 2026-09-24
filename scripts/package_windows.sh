#!/usr/bin/env bash
# Empacota o cliente do Marvyr para Windows (fonte única do pacote; o
# workflow de release só chama este script).
#
# Uso: [VERSION=0.1.0-alpha.1] [TARGET=x86_64-pc-windows-msvc] \
#      [PACKAGE_SERVER=host:porta] [PACKAGE_AUTH_URL=https://...] bash scripts/package_windows.sh [--skip-build]
#
# Saída: dist/windows/ (pasta pronta para o butler) e
#        dist/Marvyr-v<versão>-windows-x86_64.zip.
#
# assets/dev NÃO entra no pacote: `grep '"dev/' crates/client/src` não acha
# nenhuma referência em runtime (a pasta só tem .gitkeep). Se o cliente passar
# a carregar algo de dev/, inclua-a em ASSET_DIRS.
set -euo pipefail

cd "$(dirname "$0")/.."

SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=1 ;;
        *) echo "argumento desconhecido: $arg (uso: $0 [--skip-build])" >&2; exit 2 ;;
    esac
done

VERSION="${VERSION:-$(git describe --tags --always)}"
VERSION="${VERSION#v}"
TARGET="${TARGET:-x86_64-pc-windows-msvc}"
ASSET_DIRS=(marvyr external shaders)
OUT="dist/windows"
ZIP="dist/Marvyr-v${VERSION}-windows-x86_64.zip"

MARVYR_BUILD_SHA="$(git rev-parse --short HEAD)"
MARVYR_VERSION_LABEL="$VERSION"
MARVYR_PUBLIC_BUILD=1
export MARVYR_BUILD_SHA MARVYR_VERSION_LABEL MARVYR_PUBLIC_BUILD

# Aceita CRLF (checkout no Windows com autocrlf).
PROTOCOL="$(sed -n 's/^pub const PROTOCOL_VERSION: u16 = \([0-9][0-9]*\);.*/\1/p' crates/protocol/src/lib.rs)"
if [[ -z "$PROTOCOL" ]]; then
    echo "não achei PROTOCOL_VERSION em crates/protocol/src/lib.rs" >&2
    exit 1
fi

if [[ -z "${MARVYR_DEFAULT_SERVER:-}" && -z "${PACKAGE_SERVER:-}" ]]; then
    echo "aviso: build público sem MARVYR_DEFAULT_SERVER nem PACKAGE_SERVER;" \
        "o jogo vai abrir em 'Servidor do Marvyr não configurado' até o jogador criar marvyr.toml" >&2
fi

echo "==> Marvyr ${VERSION} (build ${MARVYR_BUILD_SHA}, protocolo ${PROTOCOL}, alvo ${TARGET})"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    cargo build --release --locked -p marvyr-client --target "$TARGET"
fi

EXE="target/${TARGET}/release/marvyr-client.exe"
if [[ ! -f "$EXE" ]]; then
    echo "executável não encontrado: $EXE" >&2
    exit 1
fi

rm -rf "$OUT" "$ZIP"
mkdir -p "$OUT/assets"

cp "$EXE" "$OUT/Marvyr.exe"
for dir in "${ASSET_DIRS[@]}"; do
    cp -R "assets/$dir" "$OUT/assets/$dir"
done
find "$OUT/assets" -name .gitkeep -delete
cp LICENSE "$OUT/LICENSE"
cp docs/assets/ATTRIBUTION.md "$OUT/ATTRIBUTION.md"

printf 'Marvyr %s\nbuild %s\nprotocol %s\n' "$VERSION" "$MARVYR_BUILD_SHA" "$PROTOCOL" > "$OUT/VERSION"

if [[ -z "${MARVYR_DEFAULT_SERVER:-}" && -n "${PACKAGE_SERVER:-}" ]]; then
    printf 'server = "%s"\n' "$PACKAGE_SERVER" > "$OUT/marvyr.toml"
fi
if [[ -z "${MARVYR_DEFAULT_AUTH_URL:-}" && -n "${PACKAGE_AUTH_URL:-}" ]]; then
    printf 'auth_url = "%s"\n' "$PACKAGE_AUTH_URL" >> "$OUT/marvyr.toml"
fi

cat > "$OUT/README.txt" <<EOF
Marvyr ${VERSION} - alpha
==========================

Como jogar
----------
1. Extraia o zip inteiro para uma pasta (ex.: Documentos\\Marvyr).
   Não rode direto de dentro do zip: o jogo precisa da pasta "assets" ao lado.
2. Abra Marvyr.exe. Se o Windows SmartScreen reclamar, clique em
   "Mais informações" > "Executar assim mesmo" (o build ainda não é assinado).
3. Na primeira vez, crie sua conta na tela inicial. Nas próximas, é só entrar.

Onde ficam seus dados
---------------------
Identidade e sessão ficam em %APPDATA%\\Marvyr
(cole isso na barra do Explorador de Arquivos).
Apagar essa pasta faz o jogo esquecer o login neste computador.

Log (para reportar bugs)
------------------------
Abra um Prompt de Comando nesta pasta e rode:
    Marvyr.exe > marvyr.log 2>&1
Envie o marvyr.log junto com o arquivo VERSION ao reportar o problema.

Jogar em outro servidor
-----------------------
Crie um arquivo marvyr.toml nesta pasta (ao lado do Marvyr.exe) com:
    server = "endereco-do-servidor:5000"
    auth_url = "https://endereco-da-autenticacao"
Também dá para usar a linha de comando:
    Marvyr.exe --server host:5000 --auth-url https://...

Licenças: LICENSE e ATTRIBUTION.md nesta pasta.
EOF

echo "==> compactando ${ZIP}"
zip_abs="$(pwd)/${ZIP}"
if command -v 7z >/dev/null 2>&1; then
    (cd "$OUT" && 7z a -tzip -bso0 "$zip_abs" ./*)
elif command -v zip >/dev/null 2>&1; then
    (cd "$OUT" && zip -qr "$zip_abs" .)
elif command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -Command \
        "Compress-Archive -Path '${OUT}/*' -DestinationPath '${ZIP}' -Force"
else
    echo "nenhum compactador encontrado (7z, zip ou PowerShell)" >&2
    exit 1
fi

echo "==> pronto: ${OUT}/ e ${ZIP}"
