# Publishes sensord, builds the dashboard, and compiles the Windows installer.
#
# Output:
#   dashboard\target\release\PerfWindow.exe   - the dashboard
#   dashboard\target\release\sensord.exe      - the sensor backend (sibling)
#   dist\PerfWindow-Setup.exe                 - the installer
#
# Run it from a developer command prompt, or otherwise ensure the MSVC
# environment is on PATH (the Rust build needs the linker and resource
# compiler). Inno Setup must be installed; if it is not:
#   winget install -e --id JRSoftware.InnoSetup

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

# Locate the Inno Setup compiler: the common install folders first, then the
# install location recorded in the Inno Setup uninstall registry key.
function Find-Iscc {
    $candidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    )
    foreach ($c in $candidates) {
        if ($c -and (Test-Path $c)) { return $c }
    }
    $uninstallRoots = @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall'
    )
    foreach ($regRoot in $uninstallRoots) {
        foreach ($key in (Get-ChildItem $regRoot -ErrorAction SilentlyContinue)) {
            $props = Get-ItemProperty $key.PSPath -ErrorAction SilentlyContinue
            if ($props.DisplayName -match 'Inno Setup' -and $props.InstallLocation) {
                $iscc = Join-Path $props.InstallLocation 'ISCC.exe'
                if (Test-Path $iscc) { return $iscc }
            }
        }
    }
    return $null
}

Write-Host '== Publishing sensord =='
dotnet publish "$root\sensord\src" -c Release -r win-x64 --self-contained
if ($LASTEXITCODE -ne 0) { throw 'sensord publish failed' }

Write-Host '== Building dashboard =='
cargo build --release --manifest-path "$root\dashboard\Cargo.toml"
if ($LASTEXITCODE -ne 0) { throw 'dashboard build failed' }

# build.rs places sensord.exe next to PerfWindow.exe; the installer packages
# both, so both must be present before compiling it.
$releaseDir = "$root\dashboard\target\release"
$perfExe    = Join-Path $releaseDir 'PerfWindow.exe'
$sensordExe = Join-Path $releaseDir 'sensord.exe'
foreach ($f in @($perfExe, $sensordExe)) {
    if (-not (Test-Path $f)) { throw "expected build output missing: $f" }
}

Write-Host '== Building installer =='
$iscc = Find-Iscc
if (-not $iscc) {
    throw 'Inno Setup compiler (ISCC.exe) not found. Install it with: winget install -e --id JRSoftware.InnoSetup'
}
& $iscc /Q "$root\build\PerfWindow.iss"
if ($LASTEXITCODE -ne 0) { throw 'installer compilation failed' }

$setup = "$root\dist\PerfWindow-Setup.exe"
Write-Host '== Done =='
Get-Item $perfExe, $sensordExe, $setup |
    Select-Object Name, @{ N = 'MB'; E = { [math]::Round($_.Length / 1MB, 2) } }
