# Support

## Where to ask

| You want to | Go to |
| --- | --- |
| Report something that is broken | **Issues** — use the bug report form |
| Ask for a feature or a change | **Issues** — use the feature request form |
| Ask a question | **Issues** — open an issue |
| Report a security problem | **Not an issue.** See [SECURITY.md](SECURITY.md) |
| Read the licence or the terms | [LICENSE](LICENSE), [TERMS.md](TERMS.md) |
| Check what data leaves your device | [PRIVACY.md](PRIVACY.md) |

There is no Discussions tab. Issues and the issue forms are the only channel.

## Before opening an issue

- Say which version you are running. **Settings > Updates** shows the current
  version, and the footer shows it too.
- Say which Windows version and build you are on (`winver`).
- Say your CPU and GPU model, and whether you launched PerfWindow elevated.
- Say what you did, what you expected, and what happened instead.
- If PerfWindow wrote `%APPDATA%\PerfWindow\panic.log`, paste the newest entry.
  **Do not paste personal data**: no user names, no file paths you would not
  publish, no licence keys, and nothing from a database file. If a line looks
  personal, replace it with `[redacted]`.

## What is in scope

The dashboard, the sensor service (`sensord`) and the source in this
repository. Which sensors appear depends on the hardware, firmware and drivers,
so a reading your machine does not expose is a limit, not a bug; the README
describes what each card can show.

The only network request PerfWindow makes is the update check against GitHub's
public Releases API; there is no telemetry. See [PRIVACY.md](PRIVACY.md).

PawnIO and the Microsoft Visual C++ Redistributable are third-party components
the installer adds. Report a problem in one of those to its author.

## What is not in scope

- Support for a modified build, or for a build from a fork.
- Help with a sensor your hardware, firmware or driver does not expose.
- Response-time guarantees. This is a project maintained by one person, with no
  support contract and no paid tier.
