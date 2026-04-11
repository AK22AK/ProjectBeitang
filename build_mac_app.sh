#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_NAME="Robinne"
BUNDLE_ID="com.ak22ak.robinne"
ICON_SCRIPT="${ROOT_DIR}/scripts/generate_app_icon.sh"
ICON_FILE="${ROOT_DIR}/assets/app-icon/generated/AppIcon.icns"
TARGET_DIR="${ROOT_DIR}/target/release"
OUTPUT_DIR="${ROOT_DIR}/dist"
SKIP_BUILD=0
VERSION=""

usage() {
    cat <<'EOF'
Usage: ./build_mac_app.sh [--version <version>] [--output-dir <dir>] [--skip-build]

Build a release .app bundle and package both .zip and .dmg artifacts.
EOF
}

resolve_version() {
    sed -nE 's/^version = "(.*)"/\1/p' "${ROOT_DIR}/Cargo.toml" | head -n 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            VERSION="${2:-}"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="${2:-}"
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ -z "${VERSION}" ]]; then
    VERSION="$(resolve_version)"
fi

if [[ -z "${VERSION}" ]]; then
    echo "Failed to resolve version from Cargo.toml" >&2
    exit 1
fi

mkdir -p "${OUTPUT_DIR}"
OUTPUT_DIR="$(cd "${OUTPUT_DIR}" && pwd)"

APP_BUNDLE_PATH="${OUTPUT_DIR}/${APP_NAME}.app"
APP_CONTENTS_PATH="${APP_BUNDLE_PATH}/Contents"
ZIP_PATH="${OUTPUT_DIR}/${APP_NAME}-v${VERSION}-macos.zip"
DMG_PATH="${OUTPUT_DIR}/${APP_NAME}-v${VERSION}-macos.dmg"
DMG_STAGING_DIR="$(mktemp -d "${OUTPUT_DIR}/.${APP_NAME}.dmg.XXXXXX")"

cleanup() {
    rm -rf "${DMG_STAGING_DIR}"
}

trap cleanup EXIT

if [[ "${SKIP_BUILD}" -eq 0 ]]; then
    echo "Building release binary..."
    cargo build --release
else
    echo "Skipping cargo build --release"
fi

if [[ ! -f "${TARGET_DIR}/robinne" ]]; then
    echo "Missing release binary: ${TARGET_DIR}/robinne" >&2
    exit 1
fi

rm -rf "${APP_BUNDLE_PATH}" "${ZIP_PATH}" "${DMG_PATH}"

echo "Creating app bundle structure in ${APP_BUNDLE_PATH}..."
mkdir -p "${APP_CONTENTS_PATH}/MacOS"
mkdir -p "${APP_CONTENTS_PATH}/Resources"

if [[ -x "${ICON_SCRIPT}" ]]; then
    echo "Generating app icon assets..."
    "${ICON_SCRIPT}"
fi

echo "Copying binary..."
cp "${TARGET_DIR}/robinne" "${APP_CONTENTS_PATH}/MacOS/${APP_NAME}"
chmod +x "${APP_CONTENTS_PATH}/MacOS/${APP_NAME}"

if [[ -f "${ICON_FILE}" ]]; then
    echo "Copying app icon..."
    cp "${ICON_FILE}" "${APP_CONTENTS_PATH}/Resources/AppIcon.icns"
fi

echo "Creating Info.plist..."
cat > "${APP_CONTENTS_PATH}/Info.plist" <<EOF
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
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

echo "Creating release zip..."
ditto -c -k --sequesterRsrc --keepParent "${APP_BUNDLE_PATH}" "${ZIP_PATH}"

echo "Preparing DMG staging directory..."
cp -R "${APP_BUNDLE_PATH}" "${DMG_STAGING_DIR}/${APP_NAME}.app"
ln -s /Applications "${DMG_STAGING_DIR}/Applications"

echo "Creating release dmg..."
hdiutil create \
    -volname "${APP_NAME}" \
    -srcfolder "${DMG_STAGING_DIR}" \
    -ov \
    -format UDZO \
    "${DMG_PATH}" >/dev/null

echo "App bundle created at ${APP_BUNDLE_PATH}"
echo "Release archive created at ${ZIP_PATH}"
echo "Release installer created at ${DMG_PATH}"
