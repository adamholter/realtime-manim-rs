#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
port=${1:-8917}

"$repo_root/scripts/build-web.sh"
cd "$repo_root"
PORT="$port" exec /usr/local/bin/node server/server.mjs
