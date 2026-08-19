#!/usr/bin/env bash
# Runs the RTSP server (mediamtx) in the foreground. Listens on rtsp://localhost:8554 by default.
set -euo pipefail
cd "$(dirname "$0")/.."
exec ./tools/mediamtx/mediamtx ./tools/mediamtx/mediamtx.yml
