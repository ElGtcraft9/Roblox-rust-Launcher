#!/usr/bin/env bash
# Publica una versión de RustLauncher en GitHub Releases.
#
# 1. Sube la versión en Cargo.toml y haz commit.
# 2. Ejecuta:  herramientas/publicar.sh [notas.md]
#
# Compila Windows y Linux, firma cada ejecutable con minisign (el launcher
# solo acepta actualizaciones firmadas con esta clave) y crea la release.
# La clave privada NO está en el repositorio: por defecto se busca en
# ~/.config/rustlauncher-firma/minisign.key (o en $RUSTLAUNCHER_CLAVE).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
CLAVE="${RUSTLAUNCHER_CLAVE:-$HOME/.config/rustlauncher-firma/minisign.key}"
SALIDA="target/publicar/v$VERSION"
NOTAS="${1:-}"

[ -f "$CLAVE" ] || { echo "No encuentro la clave privada en $CLAVE" >&2; exit 1; }
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Hay cambios sin confirmar: haz commit antes de publicar." >&2; exit 1
fi
if gh release view "v$VERSION" >/dev/null 2>&1; then
    echo "La versión v$VERSION ya está publicada: sube la versión en Cargo.toml." >&2; exit 1
fi

echo "== Pruebas y compilación de la versión $VERSION"
cargo test --release
cargo build --release
cargo build --release --target x86_64-pc-windows-gnu

rm -rf "$SALIDA" && mkdir -p "$SALIDA"
cp target/release/rust-launcher "$SALIDA/RustLauncher-linux"
cp target/x86_64-pc-windows-gnu/release/rust-launcher.exe "$SALIDA/RustLauncher-windows.exe"

echo "== Firma"
# El comentario firmado incluye versión y plataforma: el launcher lo comprueba.
minisign -S -s "$CLAVE" -m "$SALIDA/RustLauncher-linux" -t "rust-launcher $VERSION linux"
minisign -S -s "$CLAVE" -m "$SALIDA/RustLauncher-windows.exe" -t "rust-launcher $VERSION windows"
(cd "$SALIDA" && sha256sum RustLauncher-linux RustLauncher-windows.exe > SHA256SUMS)

echo "== Publicación"
git push
if [ -n "$NOTAS" ]; then
    gh release create "v$VERSION" "$SALIDA"/* --target "$(git rev-parse HEAD)" --title "RustLauncher $VERSION" --notes-file "$NOTAS"
else
    gh release create "v$VERSION" "$SALIDA"/* --target "$(git rev-parse HEAD)" --title "RustLauncher $VERSION" --generate-notes
fi
echo "Publicada: v$VERSION"
