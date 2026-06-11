#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ ! -f .env ]]; then
  cp .env.example .env
  echo "Created .env from .env.example — set OPENAI_API_KEY before chatting."
fi

SKILLS_DIR="${HOME}/.maco/skills"
mkdir -p "$SKILLS_DIR"
echo "Skills directory: $SKILLS_DIR"

cargo run -p maco-server -- init

echo ""
echo "Init complete. Start server:"
echo "  cargo run -p maco-server -- --bind 127.0.0.1:8080"
echo ""
echo "Optional frontend (from frontend/):"
echo "  npm install && npm run dev"
