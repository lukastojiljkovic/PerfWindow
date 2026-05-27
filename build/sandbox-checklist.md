# Windows Sandbox manual verification — PerfWindow installer

Run after every release-candidate installer build. The sandbox cannot exercise
PawnIO (the driver fails to install in the sandboxed kernel) or true hardware
sensors, but it validates the install / launch / close / re-install loop on a
state that mirrors a clean user machine.

## Prerequisites

- Windows 10/11 Pro or Enterprise with the Windows Sandbox feature enabled.
- A built `PerfWindow-Setup.exe` in the project's `dist/` folder (the location
  mapped by `sandbox.wsb`). If `dist/` does not yet exist, run
  `build/build.ps1` first.

## Run

1. Double-click `build/sandbox.wsb`. The sandbox boots within ~30 s and opens
   the mapped `dist/` folder in Explorer.
2. Work through the checklist below from inside the sandbox. Record outcomes
   against each item.

## Checklist

- [ ] Run `PerfWindow-Setup.exe`. Note any prompts, errors, install time.
- [ ] PawnIO install warning expected (sandboxed kernel rejects the driver).
      Acknowledge and let the installer continue.
- [ ] At Finish, leave **Launch PerfWindow** checked and click Finish.
- [ ] **UAC prompt** appears. Click Yes.
- [ ] **Loading screen** is visible with a "Starting sensor service…" phase
      message.
- [ ] Dashboard renders within 5 s of UAC accept.
- [ ] Close the dashboard via the title-bar X.
- [ ] Within 10 s, `Get-Process sensord -ErrorAction SilentlyContinue` in a
      sandbox PowerShell window returns no rows.
- [ ] Launch PerfWindow from the Start menu.
- [ ] UAC again, loading screen again, dashboard renders again. Repeat the
      open/close cycle five times.
- [ ] On one launch, click No on the UAC. Verify the loading screen morphs
      to an error card reading "Service start was cancelled." with
      **Retry** and **Exit** buttons.
- [ ] Click Retry. UAC appears again. Click Yes. Dashboard loads.
- [ ] Run `PerfWindow-Setup.exe` again (upgrade-in-place test). The
      "Close PerfWindow" prompt should auto-resolve thanks to
      `ForceCloseApplications`. Installer reaches Finish.
- [ ] Uninstall via Settings → Apps. Confirm:
      - `C:\Program Files\PerfWindow` is gone.
      - `sc.exe query PerfWindowSensor` reports "service does not exist".
      - The PawnIO removal prompt appears; answering Yes runs PawnIO's
        own uninstaller.

The outcome is recorded in the pull-request description as a bulleted result
list.
