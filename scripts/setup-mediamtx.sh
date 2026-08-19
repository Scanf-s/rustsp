#!/usr/bin/env bash
# Downloads the latest mediamtx release into ./tools/mediamtx (kept out of git).
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p tools/mediamtx
cd tools/mediamtx

TAG=$(curl -fsSL https://api.github.com/repos/bluenviron/mediamtx/releases/latest | grep -oP '"tag_name":\s*"\K[^"]+')
echo "latest mediamtx: ${TAG}"

ARCHIVE="mediamtx_${TAG}_linux_amd64.tar.gz"
curl -fsSL -o "${ARCHIVE}" "https://github.com/bluenviron/mediamtx/releases/download/${TAG}/${ARCHIVE}"
tar xzf "${ARCHIVE}"
rm "${ARCHIVE}"

echo "OK: $(pwd)/mediamtx"
