#!/bin/bash
set -e

APP_NAME="Beitang"
BUNDLE_ID="com.jiangzhengjie.beitang"
TARGET_DIR="target/debug"
APP_DIR="${APP_NAME}.app/Contents"

echo "Building debug binary..."
cargo build

echo "Creating app bundle structure..."
mkdir -p "${APP_DIR}/MacOS"
mkdir -p "${APP_DIR}/Resources"

echo "Copying binary..."
cp "${TARGET_DIR}/beitang" "${APP_DIR}/MacOS/${APP_NAME}"

echo "Creating Info.plist..."
cat > "${APP_DIR}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

echo "App bundle created at ${APP_NAME}.app"
echo "You can move this to your /Applications folder or run it directly via 'open ${APP_NAME}.app'"
