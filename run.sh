#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")"
PYTHONPATH="$PWD/src${PYTHONPATH:+:$PYTHONPATH}" python3 -m chromakit
