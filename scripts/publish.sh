#!/usr/bin/env bash
# Publishes an ffmpeg test pattern (with a clock overlay), encoded as H.264, to mediamtx in a loop.
# Stream URL: rtsp://localhost:8554/test
# -g 30            : one keyframe (IDR) per second, so a client that joins late gets a picture quickly
# -tune zerolatency: no B-frames, which keeps timestamp handling simple while learning
set -euo pipefail

exec ffmpeg -hide_banner -re \
  -f lavfi -i "testsrc2=size=640x480:rate=30" \
  -c:v libx264 -profile:v baseline -tune zerolatency -g 30 -pix_fmt yuv420p \
  -f rtsp rtsp://localhost:8554/test
