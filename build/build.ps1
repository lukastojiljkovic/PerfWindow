# Publishes sensord, builds the dashboard, and compiles the Windows installer.
#
# Output:
#   dashboard\target\release\PerfWindow.exe   - the dashboard
#   dashboard\target\release\sensord.exe      - the sensor backend, plus its
#                                               self-contained .NET runtime
#                                               files as siblings
#   dist\PerfWindow-Setup.exe                 - the installer
#
# Run it from a developer command prompt, or otherwise ensure the MSVC
# environment is on PATH (the Rust build needs the linker and resource
# compiler). Inno Setup must be installed; if it is not:
#   winget install -e --id JRSoftware.InnoSetup

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

# Third-party binaries bundled into the installer, pinned to exact versions
# with SHA-256 checks so the build is reproducible and a tampered redirect
# cannot slip an unexpected binary into the installer.
#
# Microsoft Visual C++ 2015-2022 Redistributable (x64) 14.44.35211.0 — the
# fixed-version Microsoft URL (the aka.ms permalink floats to whatever is
# newest, which would silently change the shipped bytes).
$VcRedistUrl    = 'https://download.visualstudio.microsoft.com/download/pr/73aabf2e-9532-4f68-99f7-3247081a619c/CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B/VC_redist.x64.exe'
$VcRedistSha256 = 'CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B'
# PawnIO 2.2.0 — the kernel driver LibreHardwareMonitor 0.9.5+ uses for MSR
# access (it replaced WinRing0).
$PawnIoUrl      = 'https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe'
$PawnIoSha256   = '1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032'

# Parses the `version` field of [package] in dashboard/Cargo.toml. Errors if
# the file is missing, the [package] section is missing, or the field cannot
# be parsed — the build must not produce a version-less artifact.
function Get-CargoVersion {
    $manifest = Join-Path $root 'dashboard\Cargo.toml'
    if (-not (Test-Path $manifest)) {
        throw "Cargo.toml not found at $manifest"
    }
    $inPackage = $false
    foreach ($line in Get-Content -LiteralPath $manifest) {
        $trimmed = $line.Trim()
        if ($trimmed -match '^\[(.+)\]$') {
            $inPackage = ($matches[1] -eq 'package')
            continue
        }
        if ($inPackage -and $trimmed -match '^\s*version\s*=\s*"([^"]+)"') {
            return $matches[1]
        }
    }
    throw "Could not find 'version' under [package] in $manifest"
}

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

# Returns the vendored copy of a bundled binary, downloading into
# build\vendor\ on demand and verifying the pinned SHA-256 both for the
# cached file and after a fresh download. A cached file with the wrong hash
# (older pin, partial download) is re-fetched. The vendored files are bundled
# by PerfWindow.iss (dontcopy) and run by the installer.
function Ensure-VendorFile {
    param(
        [Parameter(Mandatory)] [string]$FileName,
        [Parameter(Mandatory)] [string]$Url,
        [Parameter(Mandatory)] [string]$ExpectedSha256
    )
    $vendorDir = Join-Path $root 'build\vendor'
    $vendorBin = Join-Path $vendorDir $FileName

    if (Test-Path $vendorBin) {
        $existing = (Get-FileHash $vendorBin -Algorithm SHA256).Hash
        if ($existing -eq $ExpectedSha256) { return $vendorBin }
        Remove-Item $vendorBin
    }
    if (-not (Test-Path $vendorDir)) {
        New-Item -ItemType Directory -Path $vendorDir | Out-Null
    }
    Write-Host "== Downloading $FileName =="
    $previousProgress = $ProgressPreference
    $ProgressPreference = 'SilentlyContinue'
    try {
        Invoke-WebRequest -Uri $Url -OutFile $vendorBin -UseBasicParsing
    } catch {
        # Don't leave a truncated file behind — it would be hash-checked and
        # re-downloaded next run, but a stray partial file invites confusion.
        Remove-Item $vendorBin -ErrorAction SilentlyContinue
        throw
    } finally {
        $ProgressPreference = $previousProgress
    }
    $actual = (Get-FileHash $vendorBin -Algorithm SHA256).Hash
    if ($actual -ne $ExpectedSha256) {
        Remove-Item $vendorBin -ErrorAction SilentlyContinue
        throw "$FileName SHA-256 mismatch: expected $ExpectedSha256, got $actual"
    }
    return $vendorBin
}

$appVersion = Get-CargoVersion
Write-Host "== Publishing sensord (version $appVersion) =="
# The installer bundles the publish directory wholesale, so clear stale
# output first — a leftover file from a previous publish (different
# dependency set or publish mode) must not ship.
$publishDir = "$root\sensord\src\bin\Release\net8.0-windows\win-x64\publish"
if (Test-Path $publishDir) { Remove-Item -Recurse -Force $publishDir }
dotnet publish "$root\sensord\src" -c Release -r win-x64 --self-contained `
    -p:Version=$appVersion -p:AssemblyVersion=$appVersion -p:FileVersion=$appVersion
if ($LASTEXITCODE -ne 0) { throw 'sensord publish failed' }

Write-Host '== Building dashboard =='
cargo build --release --manifest-path "$root\dashboard\Cargo.toml"
if ($LASTEXITCODE -ne 0) { throw 'dashboard build failed' }

# build.rs places the sensord folder publish next to PerfWindow.exe; the
# installer packages the pair, so the key files must exist before compiling
# it (sensord.dll is the managed payload the sensord.exe apphost loads).
$releaseDir = "$root\dashboard\target\release"
$perfExe    = Join-Path $releaseDir 'PerfWindow.exe'
$sensordExe = Join-Path $releaseDir 'sensord.exe'
$sensordDll = Join-Path $releaseDir 'sensord.dll'
foreach ($f in @($perfExe, $sensordExe, $sensordDll)) {
    if (-not (Test-Path $f)) { throw "expected build output missing: $f" }
}

Write-Host '== Building installer =='
Ensure-VendorFile 'vc_redist.x64.exe' $VcRedistUrl $VcRedistSha256 | Out-Null
Ensure-VendorFile 'PawnIO_setup.exe' $PawnIoUrl $PawnIoSha256 | Out-Null
$iscc = Find-Iscc
if (-not $iscc) {
    throw 'Inno Setup compiler (ISCC.exe) not found. Install it with: winget install -e --id JRSoftware.InnoSetup'
}
& $iscc /Q "/DAppVersion=$appVersion" "$root\build\PerfWindow.iss"
if ($LASTEXITCODE -ne 0) { throw 'installer compilation failed' }

$setup = "$root\dist\PerfWindow-Setup.exe"
Write-Host '== Done =='
Get-Item $perfExe, $sensordExe, $setup |
    Select-Object Name, @{ N = 'MB'; E = { [math]::Round($_.Length / 1MB, 2) } }
