#!/usr/bin/env bash
set -euo pipefail

BIN_NAME="epub-translator-google"
INSTALL_DIR="${HOME}/.local/bin"
DEST_BIN="${INSTALL_DIR}/${BIN_NAME}"

if [[ -f "${DEST_BIN}" || -L "${DEST_BIN}" ]]; then
    rm -f "${DEST_BIN}"
    echo "Desinstalação concluída: ${DEST_BIN} foi removido com sucesso."
else
    echo "O binário ${DEST_BIN} não está instalado."
fi
