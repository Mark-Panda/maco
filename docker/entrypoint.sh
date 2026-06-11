#!/bin/sh
set -eu

if [ ! -f "${HOME}/.maco/data/maco.db" ]; then
  echo "maco: first run — initializing database..."
  maco-server init
fi

exec maco-server --bind "0.0.0.0:8080"
