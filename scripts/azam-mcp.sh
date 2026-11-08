#!/usr/bin/env sh
# Starts azam's MCP server (stdio) for an AI agent such as Claude Code, building it first.
# Builds the azam app too, so the server's open_window tool can show the design live.
# Build output goes to stderr: stdout carries the MCP protocol only.
set -eu
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
if [ ! -d third_party/bui/.git ]; then
  sh scripts/setup-bui.sh >&2
fi
cargo build --release --quiet -p azam-app -p azam-mcp >&2
exec "$root/target/release/azam-mcp" "$@"
