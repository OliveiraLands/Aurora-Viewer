<#
.SYNOPSIS
    Builds "Aurora Viewer.app" from the built viewer: the executable in
    Contents/MacOS, the files of package.ps1 (licences, notices, install
    metadata) with the icon and the translated texts of Info.plist in
    Contents/Resources. The macOS job of the release runs it (PowerShell 7);
    signing and notarization come after, in the workflow.
.EXAMPLE
    ./scripts/package-macos.ps1 -Exe target/release/aurora-viewer -Out "Aurora Viewer.app"
#>
param(
    [Parameter(Mandatory)][string]$Exe,
    [Parameter(Mandatory)][string]$Out,
    # repository whose files go in the bundle (default: this script's)
    [string]$Source = (Split-Path $PSScriptRoot -Parent)
)
$ErrorActionPreference = 'Stop'

if (Test-Path -LiteralPath $Out) { Remove-Item -LiteralPath $Out -Recurse -Force }
$contents = Join-Path $Out 'Contents'
$macos = Join-Path $contents 'MacOS'
$resources = Join-Path $contents 'Resources'
New-Item -ItemType Directory -Force -Path $macos, $resources | Out-Null

# the files every package carries, then the executable where macOS runs it
# (code only in Contents/MacOS: the signature seals the rest as resources)
& (Join-Path $PSScriptRoot 'package.ps1') -Exe $Exe -Out $resources -Source $Source
Move-Item -LiteralPath (Join-Path $resources (Split-Path $Exe -Leaf)) -Destination (Join-Path $macos 'aurora-viewer')

# Optional local branding overrides (brand.local.json or AURORA_BRAND_CONFIG).
$brandPath = if ($env:AURORA_BRAND_CONFIG) { $env:AURORA_BRAND_CONFIG } else { Join-Path $Source 'brand.local.json' }
$brand = $null
if (Test-Path -LiteralPath $brandPath) {
    $brand = Get-Content -LiteralPath $brandPath -Raw | ConvertFrom-Json
}
$appName = if ($brand -and $brand.app_name) { [string]$brand.app_name } else { 'Aurora Viewer' }
$bundleId = if ($brand -and $brand.macos_bundle_id) { [string]$brand.macos_bundle_id } else { 'org.aurora-viewer.AuroraViewer' }

# MAJOR.MINOR.PATCH of the build, the only form CFBundleShortVersionString
# takes (a beta's pre-release part and the commits since the tag are left out)
$version = (Get-Content -LiteralPath (Join-Path $resources 'aurora-install.json') -Raw | ConvertFrom-Json).version
if ($version -notmatch '^(\d+\.\d+\.\d+)') { throw "Invalid package version: $version" }
$plist = (Get-Content -LiteralPath (Join-Path $Source 'packaging/macos/Info.plist') -Raw).Replace('@VERSION@', $Matches[1])
if ($appName -ne 'Aurora Viewer') {
    $plist = $plist.Replace('<string>Aurora Viewer</string>', "<string>$appName</string>")
    $plist = $plist.Replace('Aurora Viewer uses the microphone', "$appName uses the microphone")
}
if ($bundleId -ne 'org.aurora-viewer.AuroraViewer') {
    $plist = $plist.Replace('<string>org.aurora-viewer.AuroraViewer</string>', "<string>$bundleId</string>")
}
Set-Content -LiteralPath (Join-Path $contents 'Info.plist') -Value $plist -Encoding utf8NoBOM -NoNewline
Set-Content -LiteralPath (Join-Path $contents 'PkgInfo') -Value 'APPL????' -Encoding ascii -NoNewline

$icnsSource = Join-Path $Source 'assets/branding/aurora.icns'
if ($brand -and $brand.icons -and $brand.icons.macos_icns) {
    $candidate = if ([IO.Path]::IsPathRooted($brand.icons.macos_icns)) { $brand.icons.macos_icns } else { Join-Path $Source $brand.icons.macos_icns }
    if (Test-Path -LiteralPath $candidate) { $icnsSource = $candidate }
}
Copy-Item -LiteralPath $icnsSource -Destination (Join-Path $resources 'Aurora.icns')
Get-ChildItem -LiteralPath (Join-Path $Source 'packaging/macos') -Directory -Filter '*.lproj' | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $resources -Recurse
}
"${Out}: $appName $version"
