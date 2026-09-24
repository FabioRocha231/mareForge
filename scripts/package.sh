#!/usr/bin/env bash
# Empacota o cliente do Marvyr para Windows, Linux ou macOS (fonte única do
# pacote; o workflow de release só chama este script).
#
# Uso: [VERSION=0.1.0-alpha.1] [PACKAGE_SERVER=host:porta] [PACKAGE_AUTH_URL=https://...] \
#      bash scripts/package.sh <windows|linux|macos> [--skip-build]
#
# Saída: dist/<plataforma>/ (pasta pronta para o butler) e o arquivo
#        dist/Marvyr-v<versão>-<plataforma>-<arquitetura>.<zip|tar.gz>.
#
# O cliente procura `assets/` e `marvyr.toml` na pasta do executável; no
# macOS os assets ficam em Marvyr.app/Contents/Resources.
#
# assets/dev NÃO entra no pacote: `grep '"dev/' crates/client/src` não acha
# nenhuma referência em runtime (a pasta só tem .gitkeep). Se o cliente passar
# a carregar algo de dev/, inclua-a em ASSET_DIRS.
#
# Compatível com o bash 3.2 do macOS: nada de ${var,,} nem arrays vazios.
set -euo pipefail

cd "$(dirname "$0")/.."

PLATFORM=""
SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        windows|linux|macos) PLATFORM="$arg" ;;
        --skip-build) SKIP_BUILD=1 ;;
        *) echo "argumento desconhecido: $arg (uso: $0 <windows|linux|macos> [--skip-build])" >&2; exit 2 ;;
    esac
done
if [[ -z "$PLATFORM" ]]; then
    echo "uso: $0 <windows|linux|macos> [--skip-build]" >&2
    exit 2
fi

VERSION="${VERSION:-$(git describe --tags --always)}"
VERSION="${VERSION#v}"
ASSET_DIRS=(marvyr external shaders)
OUT="dist/${PLATFORM}"

case "$PLATFORM" in
    windows)
        TARGETS=(x86_64-pc-windows-msvc)
        ARCH="x86_64"
        BIN_DIR="$OUT"
        RES_DIR="$OUT"
        EXE_NAME="Marvyr.exe"
        ARCHIVE="dist/Marvyr-v${VERSION}-windows-${ARCH}.zip"
        ;;
    linux)
        TARGETS=(x86_64-unknown-linux-gnu)
        ARCH="x86_64"
        BIN_DIR="$OUT"
        RES_DIR="$OUT"
        EXE_NAME="Marvyr"
        ARCHIVE="dist/Marvyr-v${VERSION}-linux-${ARCH}.tar.gz"
        ;;
    macos)
        # Binário universal: Apple Silicon e Intel no mesmo .app.
        TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
        ARCH="universal"
        BIN_DIR="$OUT/Marvyr.app/Contents/MacOS"
        # Assets em Resources: o selo da assinatura cobre arquivos lá sem
        # xattrs; em MacOS/ eles viram "código" e o zip quebra a assinatura.
        RES_DIR="$OUT/Marvyr.app/Contents/Resources"
        EXE_NAME="Marvyr"
        ARCHIVE="dist/Marvyr-v${VERSION}-macos-${ARCH}.zip"
        ;;
esac

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

echo "==> Marvyr ${VERSION} (build ${MARVYR_BUILD_SHA}, protocolo ${PROTOCOL}, ${PLATFORM} ${ARCH})"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    for target in "${TARGETS[@]}"; do
        cargo build --release --locked -p marvyr-client --target "$target"
    done
fi

binary_of() {
    if [[ "$PLATFORM" == windows ]]; then
        echo "target/$1/release/marvyr-client.exe"
    else
        echo "target/$1/release/marvyr-client"
    fi
}
for target in "${TARGETS[@]}"; do
    if [[ ! -f "$(binary_of "$target")" ]]; then
        echo "executável não encontrado: $(binary_of "$target")" >&2
        exit 1
    fi
done

rm -rf "$OUT" "$ARCHIVE"
mkdir -p "$BIN_DIR" "$RES_DIR/assets"

if [[ "$PLATFORM" == macos ]]; then
    lipo -create -output "$BIN_DIR/$EXE_NAME" \
        "$(binary_of aarch64-apple-darwin)" "$(binary_of x86_64-apple-darwin)"
else
    cp "$(binary_of "${TARGETS[0]}")" "$BIN_DIR/$EXE_NAME"
fi
chmod +x "$BIN_DIR/$EXE_NAME"

for dir in "${ASSET_DIRS[@]}"; do
    cp -R "assets/$dir" "$RES_DIR/assets/$dir"
done
find "$RES_DIR/assets" -name .gitkeep -delete
cp LICENSE "$OUT/LICENSE"
cp docs/assets/ATTRIBUTION.md "$OUT/ATTRIBUTION.md"

printf 'Marvyr %s\nbuild %s\nprotocol %s\nplatform %s %s\n' \
    "$VERSION" "$MARVYR_BUILD_SHA" "$PROTOCOL" "$PLATFORM" "$ARCH" > "$RES_DIR/VERSION"

if [[ -z "${MARVYR_DEFAULT_SERVER:-}" && -n "${PACKAGE_SERVER:-}" ]]; then
    printf 'server = "%s"\n' "$PACKAGE_SERVER" > "$BIN_DIR/marvyr.toml"
fi
if [[ -z "${MARVYR_DEFAULT_AUTH_URL:-}" && -n "${PACKAGE_AUTH_URL:-}" ]]; then
    printf 'auth_url = "%s"\n' "$PACKAGE_AUTH_URL" >> "$BIN_DIR/marvyr.toml"
fi

if [[ "$PLATFORM" == macos ]]; then
    cat > "$OUT/Marvyr.app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Marvyr</string>
    <key>CFBundleDisplayName</key><string>Marvyr</string>
    <key>CFBundleIdentifier</key><string>io.github.fabiorocha231.marvyr</string>
    <key>CFBundleExecutable</key><string>${EXE_NAME}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleVersion</key><string>${MARVYR_BUILD_SHA}</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
    # ponytail: assinatura ad-hoc (sem Developer ID). Apple Silicon exige pelo
    # menos isto para abrir; notarização entra quando houver conta da Apple.
    codesign --force --deep --sign - "$OUT/Marvyr.app"
fi

case "$PLATFORM" in
    windows)
        RUN_STEPS='1. Extraia o zip inteiro para uma pasta (ex.: Documentos\Marvyr).
   Não rode direto de dentro do zip: o jogo precisa da pasta "assets" ao lado.
2. Abra Marvyr.exe. Se o Windows SmartScreen reclamar, clique em
   "Mais informações" > "Executar assim mesmo" (o build ainda não é assinado).'
        DATA_DIR='%APPDATA%\Marvyr
(cole isso na barra do Explorador de Arquivos)'
        LOG_CMD='Abra um Prompt de Comando nesta pasta e rode:
    Marvyr.exe > marvyr.log 2>&1'
        TOML_DIR='nesta pasta (ao lado do Marvyr.exe)'
        CLI='Marvyr.exe'
        ;;
    linux)
        RUN_STEPS='1. Extraia o arquivo:  tar xzf Marvyr-*-linux-x86_64.tar.gz
2. Rode ./Marvyr dentro da pasta extraída.
   Precisa de driver de vídeo com Vulkan e das bibliotecas libasound2 e
   libudev1 (já vêm na maioria das distros).'
        DATA_DIR='~/.local/share/Marvyr (ou $XDG_DATA_HOME/Marvyr)'
        LOG_CMD='Num terminal nesta pasta, rode:
    ./Marvyr > marvyr.log 2>&1'
        TOML_DIR='nesta pasta (ao lado do Marvyr)'
        CLI='./Marvyr'
        ;;
    macos)
        RUN_STEPS='1. Extraia o zip e arraste Marvyr.app para a pasta Aplicativos.
2. Na primeira vez, clique com o botão direito em Marvyr.app > Abrir
   (o build ainda não é notarizado pela Apple). Se o macOS disser que o app
   "está danificado", rode no Terminal:
       xattr -dr com.apple.quarantine /Applications/Marvyr.app'
        DATA_DIR='~/Library/Application Support/Marvyr'
        LOG_CMD='No Terminal, rode:
    /Applications/Marvyr.app/Contents/MacOS/Marvyr > ~/marvyr.log 2>&1'
        TOML_DIR='em Marvyr.app/Contents/MacOS (botão direito > Mostrar Conteúdo do Pacote)'
        CLI='/Applications/Marvyr.app/Contents/MacOS/Marvyr'
        ;;
esac

cat > "$OUT/README.txt" <<EOF
Marvyr ${VERSION} - alpha
==========================

Como jogar
----------
${RUN_STEPS}
3. Na primeira vez, crie sua conta na tela inicial. Nas próximas, é só entrar.
   F1 abre o livreto do marujo com todos os controles.

Onde ficam seus dados
---------------------
Identidade e sessão ficam em ${DATA_DIR}.
Apagar essa pasta faz o jogo esquecer o login neste computador.

Log (para reportar bugs)
------------------------
${LOG_CMD}
Envie o marvyr.log junto com o arquivo VERSION ao reportar o problema.

Jogar em outro servidor
-----------------------
Crie um arquivo marvyr.toml ${TOML_DIR} com:
    server = "endereco-do-servidor:5000"
    auth_url = "https://endereco-da-autenticacao"
Também dá para usar a linha de comando:
    ${CLI} --server host:5000 --auth-url https://...

Licenças: LICENSE e ATTRIBUTION.md nesta pasta.
EOF

echo "==> compactando ${ARCHIVE}"
archive_abs="$(pwd)/${ARCHIVE}"
if [[ "$PLATFORM" == linux ]]; then
    tar -czf "$archive_abs" -C "$OUT" .
elif command -v 7z >/dev/null 2>&1 && [[ "$PLATFORM" == windows ]]; then
    (cd "$OUT" && 7z a -tzip -bso0 "$archive_abs" ./*)
elif command -v zip >/dev/null 2>&1; then
    # -y guarda symlinks como symlinks (o .app assinado não pode virar cópia).
    (cd "$OUT" && zip -qry "$archive_abs" .)
elif command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -Command \
        "Compress-Archive -Path '${OUT}/*' -DestinationPath '${ARCHIVE}' -Force"
else
    echo "nenhum compactador encontrado (7z, zip ou PowerShell)" >&2
    exit 1
fi

echo "==> pronto: ${OUT}/ e ${ARCHIVE}"
