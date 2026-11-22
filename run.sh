#!/usr/bin/env bash
set -e

EXAMPLE="${1:-02_platformer}"
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UBE_BIN="${DIR}/../bin/ube"

# Normalize example path/name
NAME="$(basename "${EXAMPLE}")"
if [ ! -d "${DIR}/examples/${NAME}" ]; then
    echo "Error: example '${NAME}' not found under ${DIR}/examples/"
    echo "Available examples:"
    ls -1 "${DIR}/examples" | grep -v 'README\.md'
    exit 1
fi

WASM="${DIR}/${NAME}.wasm"

echo "==> Building ${NAME}..."
"${UBE_BIN}" build "${DIR}/examples/${NAME}/main.ube" --target=wasi2 -o "${WASM}"

echo "==> Running ${NAME} with groot-host..."
cargo run --manifest-path "${DIR}/host/Cargo.toml" -- "${WASM}"
