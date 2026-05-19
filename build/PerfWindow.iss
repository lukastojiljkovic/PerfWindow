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
#define AppVersion "0.1.0"
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
var
  DefenderPage: TInputOptionWizardPage;

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

procedure InitializeWizard;
begin
  DefenderPage := CreateInputOptionPage(wpSelectTasks,
    'Windows Defender',
    'Allow PerfWindow to read your CPU sensors.',
    'PerfWindow reads CPU temperature, clock and power through a kernel driver'
    + ' (WinRing0) that is on Microsoft''s vulnerable-driver list, so Windows'
    + ' Defender quarantines it on sight and those readings stop. The option'
    + ' below tells Defender to skip only PerfWindow''s install folder, its'
    + ' per-user data folder and the sensord.exe process; the rest of your'
    + ' system stays protected, and the uninstaller removes the exclusions'
    + ' again. You may leave it unchecked, but CPU temperature, clock and'
    + ' power will then show as unavailable.',
    False, False);
  DefenderPage.Add('Add the Windows Defender exclusions PerfWindow needs (recommended)');
  DefenderPage.Values[0] := True;
end;

{ Tell Windows Defender to skip PerfWindow's folders and the sensord process. }
procedure AddDefenderExclusions;
begin
  RunPowerShell(
    'Add-MpPreference -ExclusionPath ' + PsQuote(ExpandConstant('{app}')) + ';'
    + 'Add-MpPreference -ExclusionPath ' + PsQuote(ExpandConstant('{localappdata}\{#AppName}')) + ';'
    + 'Add-MpPreference -ExclusionProcess ' + PsQuote('sensord.exe'));
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
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
    + 'Remove-MpPreference -ExclusionProcess ' + PsQuote('sensord.exe'));

  { LibreHardwareMonitor removes this service itself on a clean exit; delete it
    here too in case PerfWindow was killed before it could. }
  Exec('sc.exe', 'stop R0sensord',   '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('sc.exe', 'delete R0sensord', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;
