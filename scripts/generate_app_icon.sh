#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="${ROOT_DIR}/assets/app-icon/source"
GENERATED_DIR="${ROOT_DIR}/assets/app-icon/generated"
ICONSET_DIR="${GENERATED_DIR}/AppIcon.iconset"

DARK_ICON="${SOURCE_DIR}/robinne_icon_dark_1024.png"
LIGHT_ICON="${SOURCE_DIR}/robinne_icon_light_1024.png"
TRANSPARENT_ICON="${SOURCE_DIR}/robinne_icon_transparent_1024.png"

for file in "${DARK_ICON}" "${LIGHT_ICON}" "${TRANSPARENT_ICON}"; do
    if [[ ! -f "${file}" ]]; then
        echo "Missing icon source: ${file}" >&2
        exit 1
    fi
done

mkdir -p "${GENERATED_DIR}"
rm -rf "${ICONSET_DIR}"
mkdir -p "${ICONSET_DIR}"

CENTERED_TRANSPARENT_ICON="${GENERATED_DIR}/robinne_icon_transparent_centered_1024.png"
OFFICIAL_ICON_1024="${GENERATED_DIR}/AppIcon-1024.png"
LIGHT_PREVIEW_ICON_1024="${GENERATED_DIR}/AppIcon-light-1024.png"
ICNS_OUTPUT="${GENERATED_DIR}/AppIcon.icns"

swift - "${TRANSPARENT_ICON}" "${CENTERED_TRANSPARENT_ICON}" <<'SWIFT'
import AppKit

let inputPath = CommandLine.arguments[1]
let outputPath = CommandLine.arguments[2]
let alphaThreshold: CGFloat = 0.03
let targetOccupancy: CGFloat = 0.74
let opticalOffsetX: CGFloat = -44
let opticalOffsetY: CGFloat = -78

guard let image = NSImage(contentsOfFile: inputPath),
      let tiffData = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiffData) else {
    fputs("Failed to load transparent icon source\n", stderr)
    exit(1)
}

let width = bitmap.pixelsWide
let height = bitmap.pixelsHigh
var minX = width
var minY = height
var maxX = -1
var maxY = -1

for y in 0..<height {
    for x in 0..<width {
        guard let color = bitmap.colorAt(x: x, y: y) else { continue }
        if color.alphaComponent > alphaThreshold {
            minX = min(minX, x)
            minY = min(minY, y)
            maxX = max(maxX, x)
            maxY = max(maxY, y)
        }
    }
}

guard maxX >= 0 else {
    fputs("Transparent icon has no visible pixels\n", stderr)
    exit(1)
}

let contentWidth = maxX - minX + 1
let contentHeight = maxY - minY + 1
let canvasSize = CGSize(width: width, height: height)
let maxTargetSize = min(canvasSize.width, canvasSize.height) * targetOccupancy
let scale = min(maxTargetSize / CGFloat(contentWidth), maxTargetSize / CGFloat(contentHeight))
let destinationWidth = CGFloat(contentWidth) * scale
let destinationHeight = CGFloat(contentHeight) * scale
let destinationRect = CGRect(
    x: (canvasSize.width - destinationWidth) / 2.0 + opticalOffsetX,
    y: (canvasSize.height - destinationHeight) / 2.0 + opticalOffsetY,
    width: destinationWidth,
    height: destinationHeight
)
let sourceRect = CGRect(x: minX, y: minY, width: contentWidth, height: contentHeight)

let outputImage = NSImage(size: NSSize(width: canvasSize.width, height: canvasSize.height))
outputImage.lockFocus()
NSColor.clear.setFill()
NSBezierPath(rect: CGRect(origin: .zero, size: canvasSize)).fill()
image.draw(in: destinationRect, from: sourceRect, operation: .sourceOver, fraction: 1.0)
outputImage.unlockFocus()

guard let outputTiff = outputImage.tiffRepresentation,
      let outputBitmap = NSBitmapImageRep(data: outputTiff),
      let outputData = outputBitmap.representation(using: .png, properties: [:]) else {
    fputs("Failed to render centered transparent icon\n", stderr)
    exit(1)
}

do {
    try outputData.write(to: URL(fileURLWithPath: outputPath))
} catch {
    fputs("Failed to write centered transparent icon: \(error)\n", stderr)
    exit(1)
}
SWIFT

swift - "${CENTERED_TRANSPARENT_ICON}" "${OFFICIAL_ICON_1024}" "${LIGHT_PREVIEW_ICON_1024}" <<'SWIFT'
import AppKit

let centeredIconPath = CommandLine.arguments[1]
let officialOutputPath = CommandLine.arguments[2]
let darkPreviewOutputPath = CommandLine.arguments[3]

guard let centeredIcon = NSImage(contentsOfFile: centeredIconPath) else {
    fputs("Failed to load centered icon source\n", stderr)
    exit(1)
}

let canvasSize = CGSize(width: 1024, height: 1024)
let cardRect = CGRect(x: 64, y: 64, width: 896, height: 896)
let cornerRadius: CGFloat = 196

func imageData(from image: NSImage) -> Data? {
    guard let tiff = image.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff) else {
        return nil
    }
    return bitmap.representation(using: .png, properties: [:])
}

func drawIcon(outputPath: String, backgroundColors: [NSColor], highlightColor: NSColor) {
    let output = NSImage(size: NSSize(width: canvasSize.width, height: canvasSize.height))
    output.lockFocus()

    NSColor.clear.setFill()
    NSBezierPath(rect: CGRect(origin: .zero, size: canvasSize)).fill()

    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.18)
    shadow.shadowBlurRadius = 22
    shadow.shadowOffset = NSSize(width: 0, height: -10)
    shadow.set()

    let roundedPath = NSBezierPath(roundedRect: cardRect, xRadius: cornerRadius, yRadius: cornerRadius)
    roundedPath.addClip()

    let gradient = NSGradient(colors: backgroundColors)!
    gradient.draw(in: cardRect, angle: -55)

    let highlightRect = CGRect(x: cardRect.minX + 80, y: cardRect.midY, width: 560, height: 360)
    let highlightGradient = NSGradient(starting: highlightColor.withAlphaComponent(0.22), ending: .clear)!
    highlightGradient.draw(in: NSBezierPath(ovalIn: highlightRect), relativeCenterPosition: NSPoint(x: -0.4, y: 0.6))

    centeredIcon.draw(in: CGRect(origin: .zero, size: canvasSize), from: CGRect(origin: .zero, size: centeredIcon.size), operation: .sourceOver, fraction: 1.0)

    output.unlockFocus()

    guard let data = imageData(from: output) else {
        fputs("Failed to encode composed icon\n", stderr)
        exit(1)
    }

    do {
        try data.write(to: URL(fileURLWithPath: outputPath))
    } catch {
        fputs("Failed to write composed icon: \(error)\n", stderr)
        exit(1)
    }
}

drawIcon(
    outputPath: officialOutputPath,
    backgroundColors: [
        NSColor(calibratedRed: 0.99, green: 0.99, blue: 0.995, alpha: 1.0),
        NSColor(calibratedRed: 0.925, green: 0.93, blue: 0.945, alpha: 1.0)
    ],
    highlightColor: NSColor.white
)

drawIcon(
    outputPath: darkPreviewOutputPath,
    backgroundColors: [
        NSColor(calibratedRed: 0.26, green: 0.29, blue: 0.37, alpha: 1.0),
        NSColor(calibratedRed: 0.10, green: 0.12, blue: 0.18, alpha: 1.0)
    ],
    highlightColor: NSColor(calibratedRed: 0.78, green: 0.82, blue: 0.92, alpha: 1.0)
)
SWIFT

create_icon() {
    local size="$1"
    local output="$2"
    sips -s format png -z "${size}" "${size}" "${OFFICIAL_ICON_1024}" --out "${output}" >/dev/null
}

create_icon 16 "${ICONSET_DIR}/icon_16x16.png"
create_icon 32 "${ICONSET_DIR}/icon_16x16@2x.png"
create_icon 32 "${ICONSET_DIR}/icon_32x32.png"
create_icon 64 "${ICONSET_DIR}/icon_32x32@2x.png"
create_icon 128 "${ICONSET_DIR}/icon_128x128.png"
create_icon 256 "${ICONSET_DIR}/icon_128x128@2x.png"
create_icon 256 "${ICONSET_DIR}/icon_256x256.png"
create_icon 512 "${ICONSET_DIR}/icon_256x256@2x.png"
create_icon 512 "${ICONSET_DIR}/icon_512x512.png"
cp "${OFFICIAL_ICON_1024}" "${ICONSET_DIR}/icon_512x512@2x.png"

iconutil -c icns "${ICONSET_DIR}" -o "${ICNS_OUTPUT}"

echo "Generated icon assets:"
echo "  ${CENTERED_TRANSPARENT_ICON}"
echo "  ${OFFICIAL_ICON_1024}"
echo "  ${LIGHT_PREVIEW_ICON_1024}"
echo "  ${ICNS_OUTPUT}"
