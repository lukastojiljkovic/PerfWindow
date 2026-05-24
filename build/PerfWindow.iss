; PerfWindow installer - Inno Setup script.
;
; Produces PerfWindow-Setup.exe: installs PerfWindow.exe + sensord.exe into
; Program Files, adds a Start Menu shortcut and an Add/Remove Programs entry,
; optionally registers the Windows Defender exclusion the WinRing0 sensor
; driver needs, and ships an uninstaller that removes every file, the driver
; service and the Defender exclusions.
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

[Icons]
Name: "{group}\{#AppName}";           Filename: "{app}\PerfWindow.exe"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}";     Filename: "{app}\PerfWindow.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\PerfWindow.exe"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; sensord.sys (the WinRing0 driver) is written into {app} at runtime, and
; PerfWindow keeps config/runtime data outside {app}; remove all of it so the
; machine is left exactly as it was before installation.
Type: filesandordirs; Name: "{app}"
Type: filesandordirs; Name: "{userappdata}\{#AppName}"
Type: filesandordirs; Name: "{localappdata}\{#AppName}"

[Code]
const
  VC_REDIST_KEY = 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64';
  { Microsoft Defender threat ID for "VulnerableDriver:WinNT/Winring0".
    Defender's vulnerable-driver detection fires inside the kernel scanner
    once LibreHardwareMonitor loads the driver service; file-path exclusions
    cannot suppress it because the detection runs against the in-kernel
    image, not the .sys on disk. Registering this ID as Allow is the only
    setting that stops the recurring alert. }
  WIN_RING0_THREAT_ID = '2147937641';

var
  DefenderPage: TInputOptionWizardPage;

{ Wrap a string in single quotes for embedding in a PowerShell command. }
function PsQuote(const S: String): String;
begin
  Result := Chr(39) + S + Chr(39);
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

{ End-to-end prereq sequence: skip if already installed; otherwise extract
  the bundled redistributable, surface a status message, run it silently,
  and route any unexpected exit code through HandleVcRedistFailure (which
  aborts the installer). Treats vcredist codes 0, 1638 (newer already
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

{ Run a PowerShell command hidden and wait for it. Best-effort. }
procedure RunPowerShell(const Command: String);
var
  ResultCode: Integer;
begin
  Exec('powershell.exe',
       '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -Command "' + Command + '"',
       '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure InitializeWizard;
begin
  DefenderPage := CreateInputOptionPage(wpSelectTasks,
    'Windows Defender',
    'Allow PerfWindow to read your CPU sensors.',
    'PerfWindow reads CPU temperature, clock and power through a kernel driver'
    + ' (WinRing0) that is on Microsoft''s vulnerable-driver list. Without'
    + ' configuration, Windows Defender flags the loaded driver with a'
    + ' recurring "VulnerableDriver:WinNT/Winring0" alert. The option below'
    + ' adds Defender exclusions for PerfWindow''s install folder, its per-user'
    + ' data folder and the sensord.exe process, and registers an Allow rule'
    + ' for the WinRing0 threat signature so the alert does not surface'
    + ' again. The rest of your system stays protected, and the uninstaller'
    + ' reverses every change. You may leave it unchecked, but CPU'
    + ' temperature, clock and power will then show as unavailable and the'
    + ' Defender alert will keep returning.',
    False, False);
  DefenderPage.Add('Configure Windows Defender for PerfWindow (recommended)');
  DefenderPage.Values[0] := True;
end;

{ Tell Windows Defender to skip PerfWindow's folders and the sensord process,
  and to allow the specific WinRing0 threat signature. The reconciliation pass
  scans Defender's detection history for any other ID a definition update may
  have introduced for the same driver and allows those too, so a future
  Microsoft update that renumbers the signature self-heals on the next install. }
procedure AddDefenderExclusions;
begin
  RunPowerShell(
    'Add-MpPreference -ExclusionPath ' + PsQuote(ExpandConstant('{app}')) + ';'
    + 'Add-MpPreference -ExclusionPath ' + PsQuote(ExpandConstant('{localappdata}\{#AppName}')) + ';'
    + 'Add-MpPreference -ExclusionProcess ' + PsQuote('sensord.exe') + ';'
    + 'Add-MpPreference -ThreatIDDefaultAction_Ids ' + WIN_RING0_THREAT_ID + ' -ThreatIDDefaultAction_Actions Allow;'
    + 'Get-MpThreatDetection -ErrorAction SilentlyContinue'
    + ' | Where-Object { $_.Resources -match ''winring'' }'
    + ' | ForEach-Object { Add-MpPreference -ThreatIDDefaultAction_Ids $_.ThreatID -ThreatIDDefaultAction_Actions Allow }');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    EnsureVcRedist;
  if (CurStep = ssPostInstall) and DefenderPage.Values[0] then
    AddDefenderExclusions;
end;

{ On uninstall: drop the Defender exclusions and any leftover driver service. }
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ResultCode: Integer;
begin
  if CurUninstallStep <> usUninstall then
    Exit;

  RunPowerShell(
    'Remove-MpPreference -ExclusionPath ' + PsQuote(ExpandConstant('{app}')) + ';'
    + 'Remove-MpPreference -ExclusionPath ' + PsQuote(ExpandConstant('{localappdata}\{#AppName}')) + ';'
    + 'Remove-MpPreference -ExclusionProcess ' + PsQuote('sensord.exe') + ';'
    + 'Remove-MpPreference -ThreatIDDefaultAction_Ids ' + WIN_RING0_THREAT_ID + ';'
    + 'Get-MpThreatDetection -ErrorAction SilentlyContinue'
    + ' | Where-Object { $_.Resources -match ''winring'' }'
    + ' | ForEach-Object { try { Remove-MpPreference -ThreatIDDefaultAction_Ids $_.ThreatID -ErrorAction Stop } catch { } }');

  { LibreHardwareMonitor removes this service itself on a clean exit; delete it
    here too in case PerfWindow was killed before it could. }
  Exec('sc.exe', 'stop R0sensord',   '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('sc.exe', 'delete R0sensord', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;
