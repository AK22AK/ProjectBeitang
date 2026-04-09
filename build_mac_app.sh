#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_NAME="Beitang"
BUNDLE_ID="com.jiangzhengjie.beitang"
TARGET_DIR="${ROOT_DIR}/target/debug"
APP_DIR="${ROOT_DIR}/${APP_NAME}.app/Contents"
ICON_SCRIPT="${ROOT_DIR}/scripts/generate_app_icon.sh"
ICON_FILE="${ROOT_DIR}/assets/app-icon/generated/AppIcon.icns"

echo "Building debug binary..."
cargo build

echo "Creating app bundle structure..."
mkdir -p "${APP_DIR}/MacOS"
mkdir -p "${APP_DIR}/Resources"

if [[ -x "${ICON_SCRIPT}" ]]; then
    echo "Generating app icon assets..."
    "${ICON_SCRIPT}"
fi

echo "Copying binary..."
cp "${TARGET_DIR}/beitang" "${APP_DIR}/MacOS/${APP_NAME}"

if [[ -f "${ICON_FILE}" ]]; then
    echo "Copying app icon..."
    cp "${ICON_FILE}" "${APP_DIR}/Resources/AppIcon.icns"
fi

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
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
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
