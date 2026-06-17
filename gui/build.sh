#!/bin/bash
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET="$PROJECT_DIR/target/debug"
APP_NAME="HagibisMapping"
APP_DISPLAY="Hagibis Mapping"
APP_VERSION="0.1.0"

echo "==> Building Rust..."
cd "$PROJECT_DIR"; cargo build

echo "==> Compiling Swift..."
HELPER_O=$(ls "$TARGET"/build/hagibis_hub_mapper-*/out/*nsevent_helper*.o 2>/dev/null | head -1)
clang -c "$SCRIPT_DIR/elevate.c" -o "$SCRIPT_DIR/elevate.o"
swiftc -o "$SCRIPT_DIR/$APP_NAME" "$SCRIPT_DIR/main.swift" "$SCRIPT_DIR/elevate.o" \
    "$TARGET/libhagibis_hub_mapper.a" "$HELPER_O" \
    -framework IOKit -framework CoreGraphics \
    -framework Foundation -framework AppKit -framework Security
rm -f "$SCRIPT_DIR/elevate.o"

echo "==> App icon + bundle..."
ICONSET="$SCRIPT_DIR/AppIcon.iconset"; mkdir -p "$ICONSET"
SRC="$SCRIPT_DIR/icon.png"
for sz in 16 32 64 128 256 512; do
    sips -z $sz $sz "$SRC" --out "$ICONSET/icon_${sz}x${sz}.png" >/dev/null 2>&1
done
iconutil -c icns "$ICONSET" -o "$SCRIPT_DIR/AppIcon.icns" 2>/dev/null; rm -rf "$ICONSET"

APP_BUNDLE="$SCRIPT_DIR/$APP_NAME.app"
rm -rf "$APP_BUNDLE"; mkdir -p "$APP_BUNDLE/Contents/MacOS" "$APP_BUNDLE/Contents/Resources"
cp "$SCRIPT_DIR/$APP_NAME" "$APP_BUNDLE/Contents/MacOS/$APP_NAME"
cp "$SCRIPT_DIR/AppIcon.icns" "$APP_BUNDLE/Contents/Resources/AppIcon.icns"
cp "$SCRIPT_DIR/hub.png" "$APP_BUNDLE/Contents/Resources/hub.png" 2>/dev/null || true
cp "$SCRIPT_DIR/Info.plist" "$APP_BUNDLE/Contents/Info.plist"

echo "==> Signing (hardened runtime + USB entitlement)..."
xattr -cr "$APP_BUNDLE" 2>/dev/null || true
# --options runtime is REQUIRED: without the hardened runtime the
# com.apple.security.device.usb entitlement is not honored in the AEWP/root
# context and IOHIDManagerOpen(seize) fails with kIOReturnExclusiveAccess.
# NOTE: codesign is intentionally NOT silenced/|| true — a signing failure here
# produces a bundle that cannot seize the hub, so it must abort the build.
codesign --force --deep --sign - --options runtime \
    --entitlements "$SCRIPT_DIR/entitlements.plist" "$APP_BUNDLE"

# Fail loudly if the hardened-runtime flag did not actually get set — this is the
# exact regression that silently breaks IOKit seize.
if ! codesign --display --verbose=4 "$APP_BUNDLE" 2>&1 | grep -q 'flags=.*runtime'; then
    echo "ERROR: bundle is not signed with hardened runtime — IOKit seize will fail." >&2
    exit 1
fi

# Must match CFBundleIdentifier in the Info.plist above.
BUNDLE_ID="com.gabrielebaldassarre.override-hub"

echo "==> Clearing quarantine + resetting TCC (ad-hoc signature changes each build)..."
# Local builds aren't quarantined, but this is harmless and covers moved/copied bundles.
xattr -dr com.apple.quarantine "$APP_BUNDLE" 2>/dev/null || true
# Each ad-hoc re-sign changes the code identity, so prior TCC grants go stale.
# Reset them so the first launch re-prompts cleanly.
tccutil reset ListenEvent   "$BUNDLE_ID" >/dev/null 2>&1 || true
tccutil reset Accessibility "$BUNDLE_ID" >/dev/null 2>&1 || true
tccutil reset All           "$BUNDLE_ID" >/dev/null 2>&1 || true

echo "==> Done"
echo "    bundle:  $APP_BUNDLE"
echo "    launch:  open -a \"$APP_BUNDLE\"   (or double-click in Finder)"
echo "             grant permissions on first run; it elevates via AEWP (admin password)."

echo "==> Packaging DMG..."
DMG_NAME="${APP_NAME}-${APP_VERSION}"
DMG_FILE="$SCRIPT_DIR/${DMG_NAME}.dmg"
DMG_STAGING="$SCRIPT_DIR/dmg-staging"
rm -rf "$DMG_STAGING" "$DMG_FILE"
mkdir -p "$DMG_STAGING"
cp -R "$APP_BUNDLE" "$DMG_STAGING/"
# Standard macOS DMG layout: the app bundle + a symlink to /Applications
# (the user drags the app onto the Applications folder to install).
ln -s /Applications "$DMG_STAGING/Applications"
hdiutil create -volname "$APP_DISPLAY" \
    -srcfolder "$DMG_STAGING" \
    -ov -format UDZO \
    "$DMG_FILE" >/dev/null
rm -rf "$DMG_STAGING"
echo "    dmg:     $DMG_FILE"
echo ""
echo "    Distribution:  $DMG_FILE"
