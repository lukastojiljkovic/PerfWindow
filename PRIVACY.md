# PerfWindow Privacy Statement

Last updated: 1 October 2026

PerfWindow doesn't collect or send personal data to its author or anyone else. It has no accounts, telemetry,
analytics, crash reporting or ads. Sensor readings, hardware names and network details, such as the name of the
Wi-Fi network you're connected to, are shown in the window and never leave your PC.

## What stays on your PC

- **Settings** are stored in `%APPDATA%\PerfWindow\config.toml`.
- **The result of the last update check** is stored in `%APPDATA%\PerfWindow\update-cache.json`.
- **Crash details.** If PerfWindow crashes, it writes the error and a backtrace to `%APPDATA%\PerfWindow\panic.log`.
  The file can contain file paths that include your user name. It isn't sent anywhere.
- **Service events.** The `PerfWindowSensor` service writes start, stop and error messages to the Windows Application
  event log under the source `PerfWindowSensor`.
- **Theme.** PerfWindow reads the Windows light and dark mode setting from the registry, so it can follow the Windows
  theme if you choose to. It doesn't write to the registry.

Uninstalling PerfWindow removes the program, the service and `%APPDATA%\PerfWindow` for the account that runs the
uninstaller. Event log entries stay in the Windows event log until Windows clears them.

## Update checks

When PerfWindow starts, it checks whether a newer version is available, at most once every six hours. The check is a
single anonymous HTTPS request to GitHub's public API at `api.github.com`, identified only by the user agent
`PerfWindow/<version>`. It contains no identifiers, settings or sensor data. Like any web request, it reveals your IP
address to GitHub.

If you choose to install an update, PerfWindow downloads the installer from GitHub to your temporary folder.

You can turn off update checks in **Settings** > **Updates**. GitHub's handling of these requests is governed by the
GitHub General Privacy Statement at https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement.

## Other

- **Links** to GitHub and to component websites open in your web browser, where those sites' privacy statements apply.
- **Data protection law.** The author receives no data from PerfWindow, so there is no personal data for the author to
  access, correct or delete. For data GitHub processes during update checks, contact GitHub as described in its
  privacy statement.

## Contact

Open an issue at https://github.com/lukastojiljkovic/PerfWindow/issues.
