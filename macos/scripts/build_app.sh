#!/usr/bin/env bash
set -euo pipefail

# GDOU Net Login macOS (Apple Silicon arm64, macOS 15+) Build & Package Script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$ROOT_DIR/build"
APP_NAME="GDOU-net-login"
APP_BUNDLE="$BUILD_DIR/$APP_NAME.app"
RELEASE_ASSETS_DIR="$ROOT_DIR/../release-assets"

SIGN_IDENTITY="${1:-"-"}" # Default to Ad-hoc self-signing if not provided

echo "==> Building $APP_NAME for macOS 15+ (Apple Silicon arm64)..."
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"
mkdir -p "$RELEASE_ASSETS_DIR"

# 1. Compile Swift sources with size optimizations targeting Apple Silicon on
#    macOS 15+.  SwiftUI/AppKit remain shared system frameworks; these flags
#    mainly remove unused application code and keep the app's own footprint
#    small without sacrificing the native UI.
swiftc -parse-as-library \
    -Osize \
    -whole-module-optimization \
    -Xlinker -dead_strip \
    -target arm64-apple-macos15.0 \
    "$ROOT_DIR"/Sources/*.swift \
    "$ROOT_DIR"/Sources/Models/*.swift \
    "$ROOT_DIR"/Sources/Services/*.swift \
    "$ROOT_DIR"/Sources/Views/*.swift \
    -o "$BUILD_DIR/$APP_NAME"

# Remove local symbols from the release executable.  Keep Swift reflection
# metadata required by Codable/SwiftUI, while dropping debugger-only symbols.
strip -x "$BUILD_DIR/$APP_NAME"

# 2. Assemble .app bundle structure
echo "==> Creating macOS Application Bundle..."
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Resources"

cp "$BUILD_DIR/$APP_NAME" "$APP_BUNDLE/Contents/MacOS/$APP_NAME"
cp "$ROOT_DIR/Resources/Info.plist" "$APP_BUNDLE/Contents/Info.plist"
if [ -f "$ROOT_DIR/Resources/AppIcon.icns" ]; then
    cp "$ROOT_DIR/Resources/AppIcon.icns" "$APP_BUNDLE/Contents/Resources/AppIcon.icns"
fi
if [ -f "$ROOT_DIR/Resources/icon.png" ]; then
    cp "$ROOT_DIR/Resources/icon.png" "$APP_BUNDLE/Contents/Resources/icon.png"
fi

# 3. Code Signing
echo "==> Signing app bundle with identity: '$SIGN_IDENTITY'..."
codesign --force --deep --sign "$SIGN_IDENTITY" "$APP_BUNDLE"

# 4. Packaging DMG
DMG_STAGE="$BUILD_DIR/dmg-stage"
rm -rf "$DMG_STAGE"
mkdir -p "$DMG_STAGE"
cp -R "$APP_BUNDLE" "$DMG_STAGE/"
ln -s /Applications "$DMG_STAGE/Applications"

DMG_PATH="$RELEASE_ASSETS_DIR/gdou-net-login-macos-arm64.dmg"
echo "==> Packaging DMG: $DMG_PATH..."
rm -f "$DMG_PATH"
hdiutil create -volname "GDOU Net Login" -srcfolder "$DMG_STAGE" -ov -format UDZO "$DMG_PATH"
rm -rf "$DMG_STAGE"

echo "==> Build and packaging complete!"
echo "    App: $APP_BUNDLE"
echo "    DMG: $DMG_PATH"
