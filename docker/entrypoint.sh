#!/bin/sh
set -eu

MACO_USER=maco
MACO_HOME=/home/maco
DATA_ROOT="${MACO_HOME}/.maco"

# Named volume 首次挂载时属主常为 root，需修正后再降权运行
mkdir -p "${DATA_ROOT}"
chown -R "${MACO_USER}:${MACO_USER}" "${DATA_ROOT}"

if [ ! -f "${DATA_ROOT}/data/maco.db" ]; then
  echo "maco: first run — initializing database..."
  gosu "${MACO_USER}" maco-server init
fi

exec gosu "${MACO_USER}" maco-server --bind "0.0.0.0:8080"
