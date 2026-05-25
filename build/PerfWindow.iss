; PerfWindow installer - Inno Setup script.
;
; Produces PerfWindow-Setup.exe: installs PerfWindow.exe + sensord.exe into
; Program Files, adds a Start Menu shortcut and an Add/Remove Programs entry,
; bundles the Microsoft Visual C++ runtime and the PawnIO kernel driver, and
; ships an uninstaller that removes every file and any legacy registry data
; PerfWindow left behind.
;
; PawnIO replaces WinRing0 (LibreHardwareMonitor 0.9.5+); it is a signed
; driver that Microsoft Defender does not flag, so no Defender exclusions are
; needed any longer. The installer still purges the legacy WinRing0 Defender
; entries written by PerfWindow 0.4.1 and earlier, so an upgrade leaves the
; machine clean.
;
; Build it with  build\build.ps1  (which compiles the app, then runs ISCC).

#define AppName "PerfWindow"
#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
#define AppPublisher "Luka Stojiljkovic"

[Setup]
AppId={{A7CF0EC7-99C8-4188-8BE8-7B3AD9878765}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\PerfWindow.exe
OutputDir=..\dist
OutputBaseFilename={#AppName}-Setup
SetupIconFile=..\dashboard\assets\PerfWindow.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
MinVersion=10.0
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "..\dashboard\target\release\PerfWindow.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dashboard\target\release\sensord.exe";    DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE";                                 DestDir: "{app}"; Flags: ignoreversion
Source: "vendor\vc_redist.x64.exe";                                     Flags: dontcopy
Source: "vendor\PawnIO_setup.exe";                                      Flags: dontcopy

[Icons]
Name: "{group}\{#AppName}";           Filename: "{app}\PerfWindow.exe"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}";     Filename: "{app}\PerfWindow.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\PerfWindow.exe"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; PerfWindow keeps config/runtime data outside {app}; remove all of it so the
; machine is left exactly as it was before installation. PawnIO is left alone
; on the assumption it may be shared with other LibreHardwareMonitor-based
; tools — the user can uninstall it from Add/Remove Programs.
Type: filesandordirs; Name: "{app}"
Type: filesandordirs; Name: "{userappdata}\{#AppName}"
Type: filesandordirs; Name: "{localappdata}\{#AppName}"

[Code]
const
  VC_REDIST_KEY = 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64';
  { Legacy from PerfWindow 0.4.1 and earlier: the Defender ThreatID for
    "VulnerableDriver:WinNT/Winring0", allowed at install time so the alert
    did not surface while LHM held WinRing0 open. LHM 0.9.5+ no longer uses
    WinRing0, so the allow rule is dead weight. We still remove it on every
    install and uninstall to leave Defender's allow-list clean for users
    upgrading from a build that registered it. }
  WIN_RING0_THREAT_ID = '2147937641';

{ Wrap a string in single quotes for embedding in a PowerShell command. }
function PsQuote(const S: String): String;
begin
  Result := Chr(39) + S + Chr(39);
end;

{ Run a PowerShell command hidden and wait for it. Best-effort. }
procedure RunPowerShell(const Command: String);
var
  ResultCode: Integer;
begin
  Exec('powershell.exe',
       '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -Command "' + Command + '"',
       '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

{ True iff a 14.x (Visual C++ 2015-2022) x64 runtime is registered on this
  machine. Reads the standard Microsoft-published location. ABI compatibility
  is preserved across the entire 14.x line, so any 14.x install is sufficient
  for binaries linked against vcruntime140.dll / msvcp140.dll. }
function IsVcRedistInstalled: Boolean;
var
  Installed, Major: Cardinal;
begin
  Result := RegQueryDWordValue(HKEY_LOCAL_MACHINE, VC_REDIST_KEY, 'Installed', Installed)
        and (Installed = 1)
        and RegQueryDWordValue(HKEY_LOCAL_MACHINE, VC_REDIST_KEY, 'Major', Major)
        and (Major >= 14);
end;

{ Drop the bundled vc_redist.x64.exe into the temp dir and return its full path. }
function ExtractVcRedist: String;
begin
  ExtractTemporaryFile('vc_redist.x64.exe');
  Result := ExpandConstant('{tmp}\vc_redist.x64.exe');
end;

{ Run vc_redist.x64.exe with Microsoft's silent-install switches and return
  its exit code. Returns -1 if the binary could not be launched at all. }
function InstallVcRedistSilent(const Path: String): Integer;
var
  ResultCode: Integer;
begin
  if Exec(Path, '/install /quiet /norestart', '',
          SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    Result := ResultCode
  else
    Result := -1;
end;

{ Tell the user the runtime install failed, optionally hand them the
  Microsoft download URL, and abort the installer. PerfWindow.exe will not
  launch without the runtime, so shipping it to disk now would just reproduce
  the "double-click does nothing" bug. }
procedure HandleVcRedistFailure(ExitCode: Integer);
var
  ResultCode: Integer;
  Response: Integer;
begin
  Response := MsgBox(
    'Visual C++ Runtime installation failed (code ' + IntToStr(ExitCode) + ').' + #13#10 +
    'PerfWindow cannot start without it.' + #13#10 + #13#10 +
    'Open the Microsoft download page in your browser?',
    mbError, MB_YESNO);
  if Response = IDYES then
    ShellExec('open',
              'https://aka.ms/vs/17/release/vc_redist.x64.exe',
              '', '', SW_SHOW, ewNoWait, ResultCode);
  Abort;
end;

{ End-to-end VC++ runtime sequence: skip if already installed; otherwise
  extract the bundled redistributable, surface a status message, run it
  silently, and route any unexpected exit code through HandleVcRedistFailure
  (which aborts the installer). Treats vcredist codes 0, 1638 (newer already
  installed) and 3010 (success, reboot pending) as success. }
procedure EnsureVcRedist;
var
  ExitCode: Integer;
  RedistPath: String;
begin
  if IsVcRedistInstalled then
    Exit;
  WizardForm.StatusLabel.Caption := 'Installing Visual C++ Runtime...';
  RedistPath := ExtractVcRedist;
  ExitCode := InstallVcRedistSilent(RedistPath);
  if not ((ExitCode = 0) or (ExitCode = 1638) or (ExitCode = 3010)) then
    HandleVcRedistFailure(ExitCode);
end;

{ Drop the bundled PawnIO setup binary into the temp dir and return its path. }
function ExtractPawnIo: String;
begin
  ExtractTemporaryFile('PawnIO_setup.exe');
  Result := ExpandConstant('{tmp}\PawnIO_setup.exe');
end;

{ Run PawnIO_setup.exe with namazso's silent-install switches (taken from the
  official winget manifest at microsoft/winget-pkgs:
  manifests/n/namazso/PawnIO/2.2.0). Returns its exit code, or -1 if the
  process could not be started. }
function InstallPawnIoSilent(const Path: String): Integer;
var
  ResultCode: Integer;
begin
  if Exec(Path, '-install -silent', '',
          SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    Result := ResultCode
  else
    Result := -1;
end;

{ Tell the user PawnIO installation failed but continue installing
  PerfWindow. Unlike the VC++ runtime, PerfWindow does start without PawnIO:
  the UI and sensord process come up and show every reading the OS can give
  without a kernel driver (load, memory, NVMe SMART, network throughput).
  Only CPU temperature, clock and power go missing — so we keep the install
  and tell the user how to recover. }
procedure HandlePawnIoFailure(ExitCode: Integer);
begin
  MsgBox(
    'PawnIO driver installation failed (code ' + IntToStr(ExitCode) + ').' + #13#10 +
    #13#10 +
    'PerfWindow will still install, but CPU temperature, clock and power' +
    ' readings will be unavailable until PawnIO is installed manually from' +
    #13#10 +
    'https://pawnio.eu',
    mbInformation, MB_OK);
end;

{ End-to-end PawnIO sequence: extract the bundled installer, surface a status
  message, run it silently, and route any unexpected exit code through
  HandlePawnIoFailure (a non-fatal warning). Treats codes 0 and 3010 (success
  with pending reboot) as success. PawnIO_setup.exe is upgrade-aware
  (UpgradeBehavior=uninstallPrevious in the winget manifest), so we don't
  bother detecting an existing install — re-running it on a current install
  is a no-op. }
procedure EnsurePawnIo;
var
  ExitCode: Integer;
  PawnIoPath: String;
begin
  WizardForm.StatusLabel.Caption := 'Installing PawnIO kernel driver...';
  PawnIoPath := ExtractPawnIo;
  ExitCode := InstallPawnIoSilent(PawnIoPath);
  if not ((ExitCode = 0) or (ExitCode = 3010)) then
    HandlePawnIoFailure(ExitCode);
end;

{ Strip the legacy WinRing0 Defender entries written by PerfWindow 0.4.1 and
  earlier. Safe to call on a clean machine (every cmdlet has -ErrorAction
  SilentlyContinue). A reconciliation pass also walks the Defender detection
  history and removes any other ID a definition update may have introduced
  for the same driver, mirroring the install-side reconciliation we used to
  do — so users upgrading from 0.4.1 finish with no PerfWindow-owned Defender
  state at all. }
procedure CleanupLegacyDefenderEntries;
begin
  RunPowerShell(
    'Remove-MpPreference -ExclusionPath ' + PsQuote(ExpandConstant('{app}')) + ' -ErrorAction SilentlyContinue;'
    + 'Remove-MpPreference -ExclusionPath ' + PsQuote(ExpandConstant('{localappdata}\{#AppName}')) + ' -ErrorAction SilentlyContinue;'
    + 'Remove-MpPreference -ExclusionProcess ' + PsQuote('sensord.exe') + ' -ErrorAction SilentlyContinue;'
    + 'Remove-MpPreference -ThreatIDDefaultAction_Ids ' + WIN_RING0_THREAT_ID + ' -ErrorAction SilentlyContinue;'
    + 'Get-MpThreatDetection -ErrorAction SilentlyContinue'
    + ' | Where-Object { $_.Resources -match ''winring'' }'
    + ' | ForEach-Object { try { Remove-MpPreference -ThreatIDDefaultAction_Ids $_.ThreatID -ErrorAction Stop } catch { } }');
end;

{ Stop and delete the legacy R0sensord WinRing0 driver service if it is still
  registered from a pre-0.4.2 install. PawnIO uses its own service name and
  manages its own lifecycle, so there is nothing to do for it here. }
procedure CleanupLegacyDriverService;
var
  ResultCode: Integer;
begin
  Exec('sc.exe', 'stop R0sensord',   '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('sc.exe', 'delete R0sensord', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then begin
    EnsureVcRedist;
    EnsurePawnIo;
  end;
  if CurStep = ssPostInstall then begin
    CleanupLegacyDefenderEntries;
    CleanupLegacyDriverService;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep <> usUninstall then
    Exit;

  CleanupLegacyDefenderEntries;
  CleanupLegacyDriverService;
end;
