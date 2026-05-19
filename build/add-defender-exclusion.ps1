# PerfWindow - Windows Defender exclusion setup.
#
# PerfWindow reads CPU temperature / clock / power through the
# LibreHardwareMonitor library, which loads the WinRing0 kernel driver.
# WinRing0 is on Microsoft's vulnerable-driver list, so Windows Defender
# quarantines it on sight - which breaks those CPU readings and flags the app.
#
# This script adds the Windows Defender exclusions PerfWindow needs. It shows a
# disclaimer and asks for confirmation first, and self-elevates (the Defender
# cmdlets require administrator). Run it once.
#
# The shipping installer will perform the equivalent step for its install
# directory. To undo:  Remove-MpPreference -ExclusionPath '<folder>'

$ErrorActionPreference = 'Stop'

# --- self-elevate: Add-MpPreference requires administrator -------------------
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"")
    exit
}

Add-Type -AssemblyName System.Windows.Forms

$repo = Split-Path $PSScriptRoot -Parent
$runtime = Join-Path $env:LOCALAPPDATA 'PerfWindow'

# --- disclaimer + consent ----------------------------------------------------
$disclaimer = @"
PerfWindow needs Windows Defender exclusions.

PerfWindow reads CPU temperature, clock and power through the
LibreHardwareMonitor library, which loads the 'WinRing0' kernel driver.
WinRing0 is on Microsoft's vulnerable-driver list, so Windows Defender
quarantines it - which stops those CPU readings and flags the app.

This will tell Windows Defender to skip:

  - folder:   $repo
  - folder:   $runtime
  - process:  sensord.exe

WHAT THIS MEANS: Windows Defender will no longer scan those folders or
the sensord process. Keep only PerfWindow's own files there. You can undo
it at any time with Remove-MpPreference.

Add these Windows Defender exclusions now?
"@

$answer = [System.Windows.Forms.MessageBox]::Show(
    $disclaimer, 'PerfWindow Setup',
    [System.Windows.Forms.MessageBoxButtons]::YesNo,
    [System.Windows.Forms.MessageBoxIcon]::Warning)

if ($answer -ne [System.Windows.Forms.DialogResult]::Yes) {
    exit
}

# --- apply -------------------------------------------------------------------
if (-not (Test-Path $runtime)) {
    New-Item -ItemType Directory -Path $runtime -Force | Out-Null
}
Add-MpPreference -ExclusionPath $repo
Add-MpPreference -ExclusionPath $runtime
Add-MpPreference -ExclusionProcess 'sensord.exe'

[System.Windows.Forms.MessageBox]::Show(
    "Done. Windows Defender now excludes:`n`n  $repo`n  $runtime`n  sensord.exe`n`nYou can now build and run PerfWindow.",
    'PerfWindow Setup',
    [System.Windows.Forms.MessageBoxButtons]::OK,
    [System.Windows.Forms.MessageBoxIcon]::Information) | Out-Null
