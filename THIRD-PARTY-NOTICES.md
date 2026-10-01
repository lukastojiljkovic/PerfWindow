# Third-Party Notices

PerfWindow's source code is licensed under the MIT License. The installer also includes the components below, which
are licensed under their own terms. Their full license and notice files are installed in the `licenses` folder next to
PerfWindow.exe.

## Sensor service (sensord.exe)

| Component | License |
| --- | --- |
| [.NET runtime](https://github.com/dotnet/runtime) and the Microsoft.Extensions and System libraries | MIT License, © .NET Foundation and Contributors, plus the third-party notices of the .NET runtime |
| [LibreHardwareMonitorLib](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) 0.9.6 | Mozilla Public License 2.0 |
| [BlackSharp.Core](https://github.com/Blacktempel/BlackSharp) 1.0.7, [DiskInfoToolkit](https://github.com/Blacktempel/DiskInfoToolkit) 1.1.2, [RAMSPDToolkit-NDD](https://github.com/Blacktempel/RAMSPDToolkit) 1.4.2 | Mozilla Public License 2.0, © Florian K. |
| [HidSharp](https://software.seekye.com/hidsharp) 2.6.4 | Apache License 2.0, © James F. Bellinger |
| [Mono.Posix.NETStandard](https://github.com/mono/mono) 1.0.0 | MIT License, © Microsoft Corporation |

PerfWindow uses the components licensed under the Mozilla Public License 2.0 unmodified. Their source code is
available from the linked repositories, at the versions listed.

## Dashboard (PerfWindow.exe)

| Component | License |
| --- | --- |
| Rust crates, among them [egui and eframe](https://github.com/emilk/egui), [serde](https://github.com/serde-rs/serde), [ureq](https://github.com/algesten/ureq) and [rustls](https://github.com/rustls/rustls) | Mostly MIT or Apache License 2.0. Every crate, its license and copyright notice are listed in `licenses\rust-crates.txt` |
| [IBM Plex Mono](https://github.com/IBM/plex), [Chakra Petch](https://github.com/m4rc1e/Chakra-Petch), [Space Mono](https://github.com/googlefonts/spacemono) | SIL Open Font License 1.1 |
| egui's default fonts: Ubuntu Light, Hack, Noto Emoji, emoji-icon-font | Ubuntu Font Licence 1.0, SIL Open Font License 1.1 and MIT, listed in `licenses\rust-crates.txt` |

## Installed by the installer

| Component | License |
| --- | --- |
| [PawnIO](https://pawnio.eu) 2.2.0 kernel driver, © namazso | Driver: GNU General Public License 2.0. Modules: GNU Lesser General Public License 2.1 |
| Microsoft Visual C++ 2015-2022 Redistributable (x64) | Microsoft Software License Terms, included in the redistributable |
| [Inno Setup](https://jrsoftware.org/isinfo.php) (installer and uninstaller) | [Inno Setup License](https://jrsoftware.org/files/is/license.txt), © Jordan Russell and Martijn Laan |

PerfWindow bundles the official, unmodified PawnIO installer and runs it as a separate program. PawnIO's source code is
available at https://github.com/namazso/PawnIO (driver) and https://github.com/namazso/PawnIO.Modules (modules),
tagged `2.2.0`.

Windows is a trademark of the Microsoft group of companies. PerfWindow isn't affiliated with or endorsed by Microsoft.
