#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_NAME="epub-translator-google"
TARGET_BIN="${PROJECT_DIR}/target/release/${BIN_NAME}"
INSTALL_DIR="${HOME}/.local/bin"
DEST_BIN="${INSTALL_DIR}/${BIN_NAME}"

if [[ "${1:-}" == "--uninstall" || "${1:-}" == "-u" ]]; then
    if [[ -f "${DEST_BIN}" || -L "${DEST_BIN}" ]]; then
        rm -f "${DEST_BIN}"
        echo "Desinstalação concluída: ${DEST_BIN} foi removido com sucesso."
    else
        echo "O binário ${DEST_BIN} não está instalado."
    fi
    exit 0
fi

echo "[1/3] Compilando binário em modo release (cargo build --release)..."
cargo build --release --manifest-path "${PROJECT_DIR}/Cargo.toml"

if [[ ! -f "${TARGET_BIN}" ]]; then
    echo "Erro: Binário não encontrado em ${TARGET_BIN}" >&2
    exit 1
fi

echo "[2/3] Instalando binário em ${INSTALL_DIR}..."
mkdir -p "${INSTALL_DIR}"
install -m 755 "${TARGET_BIN}" "${DEST_BIN}"

echo "[3/3] Validando instalação..."
if [[ ":${PATH}:" == *":${INSTALL_DIR}:"* ]]; then
    echo "Sucesso: ${BIN_NAME} foi instalado em ${DEST_BIN}."
    echo "O diretório ${INSTALL_DIR} já está presente no seu PATH."
    echo "Para utilizar, execute: ${BIN_NAME} --help"
else
    echo "Sucesso: ${BIN_NAME} foi instalado em ${DEST_BIN}."
    echo "AVISO: ${INSTALL_DIR} não foi detectado no seu PATH atual."
    echo "Para utilizá-lo diretamente no terminal, adicione ao seu ~/.bashrc ou ~/.zshrc:"
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo "Depois recarregue o terminal ou execute: source ~/.bashrc"
fi
