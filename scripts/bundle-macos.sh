#!/usr/bin/env sh
# Builds target/release/azam.app, a macOS application bundle around the release binary.
set -eu
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
cargo build --release -p azam-app
app="$root/target/release/azam.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/assets"
cp target/release/azam "$app/Contents/MacOS/azam"
cp -R assets/makehuman "$app/Contents/Resources/assets/" 2>/dev/null || true
version="$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"azam-app","version":"\([^"]*\)".*/\1/p')"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>azam</string>
  <key>CFBundleDisplayName</key><string>azam</string>
  <key>CFBundleIdentifier</key><string>dev.azam.app</string>
  <key>CFBundleExecutable</key><string>azam</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version:-0.1.0}</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
</dict></plist>
PLIST
echo "built $app"
