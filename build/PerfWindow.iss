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
  { Continuation lines must NOT begin with '#' — Inno's preprocessor (ISPP)
    treats a leading '#' as a directive marker and aborts compilation with
    "Unknown preprocessor directive" on the otherwise valid Pascal literal
    `#13#10`. Keep every continuation line opening with the '+' operator. }
  MsgBox(
    'PawnIO driver installation failed (code ' + IntToStr(ExitCode) + ').'
    + #13#10 + #13#10
    + 'PerfWindow will still install, but CPU temperature, clock and power'
    + ' readings will be unavailable until PawnIO is installed manually from'
    + #13#10 + 'https://pawnio.eu',
    mbInformation, MB_OK);
end;

{ End-to-end PawnIO sequence: extract the bundled installer, run a
  best-effort silent uninstall of any prior install (cleans the registry
  entry and any half-uninstalled state), then run the silent install and
  route an unexpected install exit code through HandlePawnIoFailure
  (a non-fatal warning).

  Why the explicit uninstall step: PawnIO_setup.exe -install -silent does
  NOT upgrade in place. When a prior version (or even the same version) is
  registered in the Windows Uninstall registry — and especially when that
  prior install is half-uninstalled — the installer aborts with Windows
  error 183 (ERROR_ALREADY_EXISTS) and the driver service is never created.
  The official Microsoft winget manifest for this package documents an
  UpgradeBehavior of "uninstallPrevious", confirming that the supported
  upgrade path is uninstall-then-install, which is what we do here.

  Uninstall exit code is ignored on purpose: a fresh machine has nothing to
  uninstall (uninstall returns non-zero), and that is fine — the subsequent
  install handles the clean case correctly. Install exit code is the only
  one that gates HandlePawnIoFailure. Treats install codes 0 and 3010
  (success with pending reboot) as success. }
procedure EnsurePawnIo;
var
  ExitCode: Integer;
  IgnoredCode: Integer;
  PawnIoPath: String;
begin
  WizardForm.StatusLabel.Caption := 'Installing PawnIO kernel driver...';
  PawnIoPath := ExtractPawnIo;
  { Step 1: best-effort uninstall. Cleans any prior install (any version)
    plus any half-uninstalled state from a previously aborted install. }
  Exec(PawnIoPath, '-uninstall -silent', '',
       SW_HIDE, ewWaitUntilTerminated, IgnoredCode);
  { Step 2: install. Whatever was there is gone; this should start clean. }
  ExitCode := InstallPawnIoSilent(PawnIoPath);
  if not ((ExitCode = 0) or (ExitCode = 3010)) then
    HandlePawnIoFailure(ExitCode);
end;

{ Install the PerfWindow sensor service as a LocalSystem auto-start service.
  Idempotent: `sc create` returns error 1073 (ERROR_SERVICE_EXISTS) on a
  re-install; we treat that as success and let `sc start` continue.
  The service hosts sensord.exe --service from the install directory,
  talking to dashboards over the named pipe \\.\pipe\PerfWindowSensor. }
procedure InstallSensorService;
var
  Cmd: String;
  ResultCode: Integer;
begin
  WizardForm.StatusLabel.Caption := 'Installing PerfWindow sensor service...';
  Cmd := 'create PerfWindowSensor binPath= "\"' + ExpandConstant('{app}\sensord.exe') + '\" --service"'
       + ' start= auto'
       + ' DisplayName= "PerfWindow Sensor"'
       + ' obj= LocalSystem';
  Exec('sc.exe', Cmd, '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('sc.exe',
       'description PerfWindowSensor "Provides hardware sensor readings to PerfWindow."',
       '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('sc.exe', 'start PerfWindowSensor', '',
       SW_HIDE, ewWaitUntilTerminated, ResultCode);
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

{ Stop and delete the PerfWindowSensor service. Best-effort: a partially
  installed machine may not have the service at all, which is fine. Run
  before file deletion so the binary isn't locked. }
procedure UninstallSensorService;
var
  ResultCode: Integer;
begin
  Exec('sc.exe', 'stop PerfWindowSensor',   '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('sc.exe', 'delete PerfWindowSensor', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

{ Ask the user whether to remove the PawnIO driver too, and if Yes run
  PawnIO's own uninstaller via the QuietUninstallString registry value
  (falling back to UninstallString + /SILENT). }
procedure UninstallPawnIo;
var
  Cmd: String;
  ResultCode: Integer;
begin
  if MsgBox(
       'PerfWindow installed the PawnIO kernel driver to read hardware sensors.'
       + #13#10 + #13#10
       + 'Other monitoring tools (HWiNFO, LibreHardwareMonitor) may also use it.'
       + #13#10 + #13#10
       + 'Remove PawnIO driver?',
       mbConfirmation, MB_YESNO) <> IDYES then
    Exit;

  if RegQueryStringValue(HKEY_LOCAL_MACHINE,
       'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO_is1',
       'QuietUninstallString', Cmd) then
  begin
    { Use shell to parse the quoted path + args. }
    ShellExec('', Cmd, '', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    Exit;
  end;

  if RegQueryStringValue(HKEY_LOCAL_MACHINE,
       'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO_is1',
       'UninstallString', Cmd) then
  begin
    Cmd := Cmd + ' /SILENT';
    ShellExec('', Cmd, '', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    Exit;
  end;

  MsgBox(
    'PawnIO uninstaller not found in registry. If you want to remove it, use Add/Remove Programs.',
    mbInformation, MB_OK);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then begin
    EnsureVcRedist;
    EnsurePawnIo;
    InstallSensorService;
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

  UninstallSensorService;
  CleanupLegacyDefenderEntries;
  CleanupLegacyDriverService;
  UninstallPawnIo;
end;
