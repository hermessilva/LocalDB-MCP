<#
.SYNOPSIS
  Prepara uma release local: bump de versão, build release, empacota .mcpb
  pra smoke test manual antes de tag/push.

.DESCRIPTION
  Não mexe em server.json — o hash SHA-256 publicado no registry é
  calculado pelo build do CI (.github/workflows/release.yml) a partir do
  artefato que ele mesmo gera, pra não depender de build reprodutível
  byte-a-byte entre máquina local e runner.

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

Write-Host "Preparando release v$Version..." -ForegroundColor Cyan

# 1. Versão no Cargo.toml
$cargoToml = Get-Content Cargo.toml -Raw
$cargoToml = $cargoToml -replace '(?m)^version = ".*"', "version = `"$Version`""
Set-Content Cargo.toml -Value $cargoToml -NoNewline

# 2. Versão no manifest.json do mcpb
$manifestPath = "mcpb/manifest.json"
$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
$manifest.version = $Version
($manifest | ConvertTo-Json -Depth 10) | Set-Content $manifestPath

# 3. Build release (regenera Cargo.lock com a nova versão)
Write-Host "cargo build --release..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build falhou" }

cargo test
if ($LASTEXITCODE -ne 0) { throw "cargo test falhou" }

# 4. Empacota .mcpb (só pra smoke test manual — não é o artefato oficial)
$stagingDir = "target/mcpb-staging"
if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir }
New-Item -ItemType Directory -Path "$stagingDir/server" -Force | Out-Null
Copy-Item $manifestPath "$stagingDir/manifest.json"
Copy-Item "target/release/mssql-localdb-mcp.exe" "$stagingDir/server/mssql-localdb-mcp.exe"

$bundleName = "mssql-localdb-mcp-$Version-win64.mcpb"
$bundlePath = "target/$bundleName"
if (Test-Path $bundlePath) { Remove-Item -Force $bundlePath }
Compress-Archive -Path "$stagingDir/*" -DestinationPath $bundlePath

Write-Host ""
Write-Host "Bundle local (smoke test): $bundlePath" -ForegroundColor Green
Write-Host ""
Write-Host "Revise o diff, depois:" -ForegroundColor Yellow
Write-Host "  git add Cargo.toml Cargo.lock mcpb/manifest.json"
Write-Host "  git commit -m `"chore: release v$Version`""
Write-Host "  git tag v$Version"
Write-Host "  git push origin master --tags"
Write-Host ""
Write-Host "O push da tag dispara .github/workflows/release.yml: build limpo no" -ForegroundColor Yellow
Write-Host "runner, .mcpb oficial anexado ao GitHub Release, e publish no MCP" -ForegroundColor Yellow
Write-Host "Registry via mcp-publisher (GitHub OIDC, sem secret manual)." -ForegroundColor Yellow
