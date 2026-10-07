# Security Policy

## Supported versions

Security fixes go into the latest release. The current release line is 0.11.x
(`dashboard/Cargo.toml`). Update to it before reporting.

## Reporting a vulnerability

Please don't open a public issue. Report it privately through
[GitHub's private vulnerability reporting](https://github.com/lukastojiljkovic/PerfWindow/security/advisories/new),
with the steps to reproduce it, the PerfWindow and Windows versions, the
hardware involved, and the impact you expect.

You'll get a reply in the advisory. Once a fix is released, the advisory is
published with credit to you, unless you prefer otherwise.

## What's in scope

PerfWindow runs a dashboard as the current user and a sensor service with
administrator rights. These parts matter most:

- **The sensor service.** `PerfWindowSensor` runs `sensord.exe --service` as
  `LocalSystem` and serves the named pipe `\\.\pipe\PerfWindowSensor`. Anything
  that lets a non-admin process make the service read, write or execute
  something it shouldn't is a vulnerability.
- **The updater.** The check reads GitHub's Releases API and the download
  follows only `https` URLs on `github.com` under this repository's release
  download path. The installer is hashed while it streams, checked again right
  before launch, and refused if the `.sha256` asset is missing, malformed or
  mismatched. Anything that makes the updater download or start a different
  file is a vulnerability.
- **The kernel driver.** The installer adds the third-party PawnIO driver,
  which the service uses to read processor registers. Anything that lets
  PerfWindow install, load or use a driver other than the pinned, checksummed
  PawnIO build is a vulnerability.
- **The installer.** It requests administrator rights, registers the service
  and chain-installs the bundled components. The installer isn't code-signed
  yet, so its integrity rests on the SHA-256 published with each release.

Out of scope:

- Problems in PawnIO, LibreHardwareMonitor, the .NET runtime, the Microsoft
  Visual C++ Redistributable, or Windows itself. Report those to their authors.
- Readings that are missing or wrong because of the hardware, firmware or
  drivers, as described in the README.
- Anything that requires an attacker who can already run code as the same user
  or as an administrator on the machine.
