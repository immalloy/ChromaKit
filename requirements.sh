#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")"
python3 -m pip install --upgrade pip
python3 -m pip install -r requirements.txt
