#!/bin/bash
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUST_DIR="$SCRIPT_DIR/../override-hub-rs"
TARGET="$RUST_DIR/target/debug"
APP_NAME="OverrideHub"

echo "==> Building..."
cd "$RUST_DIR"; cargo build

echo "==> Compiling..."
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
cat > "$APP_BUNDLE/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>OverrideHub</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundleIdentifier</key><string>com.gabrielebaldassarre.override-hub</string>
<key>CFBundleName</key><string>Override Hub</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>LSUIElement</key><true/>
</dict></plist>
EOF

xattr -cr "$APP_BUNDLE" 2>/dev/null || true
# --options runtime is REQUIRED: without the hardened runtime the
# com.apple.security.device.usb entitlement is not honored in the AEWP/root
# context and IOHIDManagerOpen(seize) fails with kIOReturnExclusiveAccess.
codesign --force --deep --sign - --options runtime --entitlements "$SCRIPT_DIR/OverrideHub.entitlements" "$APP_BUNDLE" 2>/dev/null || true

echo "==> Done"
echo "    binary:  $SCRIPT_DIR/$APP_NAME  (run: sudo $SCRIPT_DIR/$APP_NAME)"
echo "    bundle:  $APP_BUNDLE"
