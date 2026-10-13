#!/usr/bin/env bash
# Builds target/Grip.app (menu bar only, ad-hoc signed).
# Usage: scripts/bundle.sh [bin-name]   (default: grip; "probe" bundles the gamepad probe)
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${1:-grip}"
if [[ "$BIN" == "probe" ]]; then
  cargo build --release --example probe
  SRC="target/release/examples/probe"; NAME="GripProbe"; ID="dev.conrado.grip.probe"
else
  cargo build --release
  SRC="target/release/$BIN"; NAME="Grip"; ID="dev.conrado.grip"
fi

APP="target/$NAME.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$SRC" "$APP/Contents/MacOS/$NAME"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>GRIP.</string>
  <key>CFBundleIdentifier</key><string>$ID</string>
  <key>CFBundleExecutable</key><string>$NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP"
echo "Built $APP"
