# Publishes sensord, then builds the dashboard, producing PerfWindow.exe.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

Write-Host '== Publishing sensord =='
dotnet publish "$root\sensord\src" -c Release -r win-x64 --self-contained
if ($LASTEXITCODE -ne 0) { throw 'sensord publish failed' }

Write-Host '== Building dashboard =='
cargo build --release --manifest-path "$root\dashboard\Cargo.toml"
if ($LASTEXITCODE -ne 0) { throw 'dashboard build failed' }

$exe = "$root\dashboard\target\release\PerfWindow.exe"
Write-Host "== Done: $exe =="
Get-Item $exe | Select-Object Name, @{N='MB';E={[math]::Round($_.Length/1MB,1)}}
