#!/bin/bash
# Builds dist/Sandbox.app (+ Sandbox-macOS.zip) from a release binary.
# Usage (from Sandbox/): tools/macos/make_app.sh <path to sandbox binary> <build number>
# Runs on macOS (sips, iconutil, codesign, ditto). CI: .github/workflows/sandbox.yml
set -euo pipefail
BIN="$1"
BUILD="${2:-0}"
APP=dist/Sandbox.app
rm -rf dist && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/sandbox"
# Shipped files (read-only; your edits go to ~/Library/Application Support/Sandbox).
cp -R assets "$APP/Contents/Resources/assets"
rm -rf "$APP/Contents/Resources/assets/models"/*.bbmodel

# Icon: one pixel-art PNG → all sizes (nearest neighbour keeps the pixels).
ICONSET=dist/Sandbox.iconset
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s tools/macos/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) tools/macos/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Sandbox.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Sandbox</string>
  <key>CFBundleDisplayName</key><string>Sandbox</string>
  <key>CFBundleIdentifier</key><string>io.github.tvoitapo4ek.sandbox</string>
  <key>CFBundleExecutable</key><string>sandbox</string>
  <key>CFBundleIconFile</key><string>Sandbox</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.${BUILD}</string>
  <key>CFBundleVersion</key><string>${BUILD}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.simulation-games</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc signature (required to run on Apple Silicon; not notarised).
codesign --force --deep --sign - "$APP"
ditto -c -k --keepParent "$APP" dist/Sandbox-macOS.zip
echo "built $APP (build $BUILD)"
