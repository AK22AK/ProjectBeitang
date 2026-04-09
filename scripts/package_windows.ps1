param(
    [string]$Version,
    [string]$OutputDir = "dist",
    [string]$BinaryPath = "target/release/robinne.exe"
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot

if (-not $Version) {
    $cargoToml = Join-Path $RepoRoot "Cargo.toml"
    $versionLine = Select-String -Path $cargoToml -Pattern '^version = "(.+)"$' | Select-Object -First 1
    if (-not $versionLine) {
        throw "Failed to resolve version from Cargo.toml"
    }

    $Version = $versionLine.Matches[0].Groups[1].Value
}

$ResolvedOutputDir = if ([System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir
} else {
    Join-Path $RepoRoot $OutputDir
}

$ResolvedBinaryPath = if ([System.IO.Path]::IsPathRooted($BinaryPath)) {
    $BinaryPath
} else {
    Join-Path $RepoRoot $BinaryPath
}

if (-not (Test-Path $ResolvedBinaryPath)) {
    throw "Missing release binary: $ResolvedBinaryPath"
}

New-Item -ItemType Directory -Force -Path $ResolvedOutputDir | Out-Null

$PackageRoot = Join-Path $ResolvedOutputDir "Robinne-windows"
$ZipPath = Join-Path $ResolvedOutputDir "Robinne-v$Version-windows.zip"
$ReadmePath = Join-Path $PackageRoot "README-release.txt"

if (Test-Path $PackageRoot) {
    Remove-Item -Recurse -Force $PackageRoot
}

if (Test-Path $ZipPath) {
    Remove-Item -Force $ZipPath
}

New-Item -ItemType Directory -Force -Path $PackageRoot | Out-Null
Copy-Item -Path $ResolvedBinaryPath -Destination (Join-Path $PackageRoot "robinne.exe")

@"
Robinne v$Version

This archive contains the unsigned Windows release build for internal testing.

Files:
- robinne.exe

Notes:
- Windows may show a SmartScreen warning because this build is unsigned.
- Extract the zip before launching the app.
"@ | Set-Content -Path $ReadmePath -Encoding ASCII

Compress-Archive -Path (Join-Path $PackageRoot "*") -DestinationPath $ZipPath

Write-Host "Windows release archive created at $ZipPath"
