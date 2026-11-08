#!/usr/bin/env sh
# Starts azam's headless MCP server (stdio), building it first. The azam window serves the same
# tools over HTTP itself (http://127.0.0.1:7465/mcp); this is for when no window is open.
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
