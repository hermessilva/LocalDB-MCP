<#
.SYNOPSIS
  Prepares a local release: version bump, release build, packages .mcpb
  for a manual smoke test before tag/push.

.DESCRIPTION
  Doesn't touch server.json — the SHA-256 hash published to the registry
  is computed by the CI build (.github/workflows/release.yml) from the
  artifact it generates itself, so it doesn't depend on a byte-for-byte
  reproducible build between the local machine and the runner.

.EXAMPLE
  ./scripts/prepare-release.ps1 -Version 0.1.0
#>
param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "Preparing release v$Version..." -ForegroundColor Cyan

# 1. Version in Cargo.toml
$cargoToml = Get-Content Cargo.toml -Raw
$cargoToml = $cargoToml -replace '(?m)^version = ".*"', "version = `"$Version`""
Set-Content Cargo.toml -Value $cargoToml -NoNewline

# 2. Version in the mcpb manifest.json
$manifestPath = "mcpb/manifest.json"
$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
$manifest.version = $Version
($manifest | ConvertTo-Json -Depth 10) | Set-Content $manifestPath

# 3. Release build (regenerates Cargo.lock with the new version)
Write-Host "cargo build --release..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

cargo test
if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }

# 4. Package .mcpb (manual smoke test only — not the official artifact)
$stagingDir = "target/mcpb-staging"
if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir }
New-Item -ItemType Directory -Path "$stagingDir/server" -Force | Out-Null
Copy-Item $manifestPath "$stagingDir/manifest.json"
Copy-Item "target/release/mssql-localdb-mcp.exe" "$stagingDir/server/mssql-localdb-mcp.exe"

$bundleName = "mssql-localdb-mcp-$Version-win64.mcpb"
$bundlePath = "target/$bundleName"
if (Test-Path $bundlePath) { Remove-Item -Force $bundlePath }

# Compress-Archive writes a literal `\` path separator into zip entry
# names on Windows, which violates the ZIP spec (requires `/`) and
# breaks extraction on any non-Windows reader or JS lib (Claude Desktop
# is Electron). Build the zip by hand via System.IO.Compression to
# guarantee `/` in entries.
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$stagingFull = (Resolve-Path $stagingDir).Path
$zip = [System.IO.Compression.ZipFile]::Open($bundlePath, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    Get-ChildItem -Path $stagingDir -Recurse -File | ForEach-Object {
        $relativePath = $_.FullName.Substring($stagingFull.Length + 1) -replace '\\', '/'
        [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
            $zip, $_.FullName, $relativePath, [System.IO.Compression.CompressionLevel]::Optimal
        ) | Out-Null
    }
} finally {
    $zip.Dispose()
}

Write-Host ""
Write-Host "Local bundle (smoke test): $bundlePath" -ForegroundColor Green
Write-Host ""
Write-Host "Review the diff, then:" -ForegroundColor Yellow
Write-Host "  git add Cargo.toml Cargo.lock mcpb/manifest.json"
Write-Host "  git commit -m `"chore: release v$Version`""
Write-Host "  git tag v$Version"
Write-Host "  git push origin master --tags"
Write-Host ""
Write-Host "Pushing the tag triggers .github/workflows/release.yml: clean build on" -ForegroundColor Yellow
Write-Host "the runner, official .mcpb attached to the GitHub Release, and publish to" -ForegroundColor Yellow
Write-Host "the MCP Registry via mcp-publisher (GitHub OIDC, no manual secret)." -ForegroundColor Yellow
