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

# Downloads the official Microsoft Visual C++ 2015-2022 Redistributable into
# build\vendor\ on demand. The file is bundled by PerfWindow.iss (dontcopy)
# and installed by the installer on machines that do not have a 14.x runtime.
# Re-runs are a no-op once the file is present.
function Ensure-VcRedist {
    $vendorDir = Join-Path $root 'build\vendor'
    $vendorBin = Join-Path $vendorDir 'vc_redist.x64.exe'
    if (Test-Path $vendorBin) { return $vendorBin }
    if (-not (Test-Path $vendorDir)) {
        New-Item -ItemType Directory -Path $vendorDir | Out-Null
    }
    Write-Host '== Downloading Visual C++ Redistributable =='
    $previousProgress = $ProgressPreference
    $ProgressPreference = 'SilentlyContinue'
    try {
        Invoke-WebRequest `
            -Uri 'https://aka.ms/vs/17/release/vc_redist.x64.exe' `
            -OutFile $vendorBin `
            -UseBasicParsing
    } catch {
        # Don't leave a truncated file behind — Test-Path would treat it as a
        # valid cache on the next run and Inno would bundle a broken binary.
        Remove-Item $vendorBin -ErrorAction SilentlyContinue
        throw
    } finally {
        $ProgressPreference = $previousProgress
    }
    return $vendorBin
}

# Downloads PawnIO_setup.exe (the kernel driver LibreHardwareMonitor 0.9.5+
# uses for MSR access — it replaced WinRing0) into build\vendor\ on demand.
# Pinned to an exact version with a SHA-256 check so the bundled artifact is
# reproducible and a tampered redirect cannot slip an unexpected driver into
# the installer. Re-runs are a no-op once a matching file is present.
function Ensure-PawnIo {
    $vendorDir = Join-Path $root 'build\vendor'
    $vendorBin = Join-Path $vendorDir 'PawnIO_setup.exe'
    $version = '2.2.0'
    $expectedSha256 = '1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032'
    $url = "https://github.com/namazso/PawnIO.Setup/releases/download/$version/PawnIO_setup.exe"

    if (Test-Path $vendorBin) {
        $existing = (Get-FileHash $vendorBin -Algorithm SHA256).Hash
        if ($existing -eq $expectedSha256) { return $vendorBin }
        # Cached file is wrong (older pinned version, partial download); fall
        # through and re-download.
        Remove-Item $vendorBin
    }
    if (-not (Test-Path $vendorDir)) {
        New-Item -ItemType Directory -Path $vendorDir | Out-Null
    }
    Write-Host "== Downloading PawnIO $version =="
    $previousProgress = $ProgressPreference
    $ProgressPreference = 'SilentlyContinue'
    try {
        Invoke-WebRequest -Uri $url -OutFile $vendorBin -UseBasicParsing
    } catch {
        Remove-Item $vendorBin -ErrorAction SilentlyContinue
        throw
    } finally {
        $ProgressPreference = $previousProgress
    }
    $actual = (Get-FileHash $vendorBin -Algorithm SHA256).Hash
    if ($actual -ne $expectedSha256) {
        Remove-Item $vendorBin -ErrorAction SilentlyContinue
        throw "PawnIO SHA-256 mismatch: expected $expectedSha256, got $actual"
    }
    return $vendorBin
}

$appVersion = Get-CargoVersion
Write-Host "== Publishing sensord (version $appVersion) =="
dotnet publish "$root\sensord\src" -c Release -r win-x64 --self-contained `
    -p:Version=$appVersion -p:AssemblyVersion=$appVersion -p:FileVersion=$appVersion
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
Ensure-VcRedist | Out-Null
Ensure-PawnIo | Out-Null
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
