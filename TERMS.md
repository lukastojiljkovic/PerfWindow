# PerfWindow Terms of Use

Last updated: 1 October 2026

These terms apply to the PerfWindow application and installer published at
https://github.com/lukastojiljkovic/PerfWindow. PerfWindow's source code is licensed under the MIT License (LICENSE).
Nothing in these terms limits the rights the MIT License gives you for the source code.

By installing or using PerfWindow, you agree to these terms. If you don't agree, don't install or use it.

## 1. What PerfWindow does

PerfWindow shows readings from your PC's hardware sensors: load, temperatures, clocks, power, fan speeds, voltages,
storage health, battery and network throughput. It reads sensors and doesn't change clocks, voltages, fan speeds or
any other hardware setting.

To read sensors that need administrator access, the installer:

- registers the `PerfWindowSensor` Windows service, which runs as LocalSystem. It starts when you open PerfWindow,
  after you approve a Windows administrator prompt, and stops when you close it.
- installs the PawnIO kernel driver, which the service uses to read processor registers.
- installs the Microsoft Visual C++ Redistributable if your PC doesn't have it.

## 2. Your responsibility

- Only install PerfWindow on PCs you're allowed to administer. On a PC managed by an organization, follow its
  policies on installing drivers and services.
- Sensor readings come from the hardware and its firmware, and some are estimates. Don't rely on PerfWindow alone
  to protect hardware from overheating or to make safety decisions.
- You're responsible for complying with the laws and agreements that apply to you.

## 3. Limits of PerfWindow

- Which sensors PerfWindow can read depends on your hardware, firmware and drivers. Some readings may be missing or
  inaccurate.
- Without the PawnIO driver, processor temperature, clock and power readings aren't available.

## 4. Third-party software and services

- **PawnIO.** The PawnIO driver is third-party software by its author, installed with its own installer and licensed
  under its own terms (THIRD-PARTY-NOTICES.md). Other monitoring tools can use it too, so uninstalling PerfWindow asks
  before removing it.
- **GitHub.** Update checks and downloads use GitHub, which is governed by GitHub's terms.

## 5. No warranty

PerfWindow is provided free of charge, "as is" and "as available", without warranty of any kind, express or implied,
including the warranties of merchantability, fitness for a particular purpose and non-infringement. You bear the entire
risk of using PerfWindow.

## 6. Limitation of liability

To the fullest extent permitted by applicable law, the author and contributors aren't liable for any damages arising
from the use of, or inability to use, PerfWindow. This includes damage to hardware, loss of data, loss of profits,
business interruption and any direct, indirect, incidental, special or consequential damages, even if they were advised
of their possibility. Some jurisdictions don't allow certain liability to be excluded or limited, for example liability
for intent or gross negligence. Where that's the case, liability is limited to the extent the law allows.

## 7. Third-party components

PerfWindow includes third-party components that are licensed under their own terms, among them the Microsoft .NET
runtime, LibreHardwareMonitorLib and the PawnIO driver. Their license files are installed in the `licenses` folder
next to PerfWindow.exe and listed in THIRD-PARTY-NOTICES.md. Their licensors provide them "as is" and have no
liability to you in connection with PerfWindow.

PerfWindow isn't affiliated with or endorsed by Microsoft. Windows is a trademark of the Microsoft group of companies.

## 8. Privacy

PerfWindow doesn't collect or send personal data. Sensor readings never leave your PC. The update check is described
in PRIVACY.md.

## 9. Changes

These terms may change in future releases. The terms included with a release apply to that release.

## Contact

Open an issue at https://github.com/lukastojiljkovic/PerfWindow/issues.
