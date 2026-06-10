//! Plain-language explanations for every metric the dashboard surfaces.
//!
//! Tooltips serve readers who do not have hardware-monitoring vocabulary in
//! their head — they should *teach*, not just label. The text deliberately
//! sticks to one or two sentences: what the metric measures, what a typical
//! value looks like, and what a high value implies.
//!
//! Add a new entry to [`DESCRIPTIONS`] whenever a new stat row, chip or bar
//! ships. Lookup is case-insensitive on the trimmed label, so the same key
//! covers `"TEMP"`, `"temp"` and `"Temp"`. Callers that paint custom widgets
//! (bars, donuts, chips) can fetch the text directly with [`describe`].

use egui::Response;

/// Return the plain-language description for a metric label, if one is
/// registered. Trimmed and upper-cased before lookup.
pub fn describe(label: &str) -> Option<&'static str> {
    let key = label.trim().to_ascii_uppercase();
    DESCRIPTIONS
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&key))
        .map(|(_, v)| *v)
}

/// Attach the registered tooltip (if any) to an egui [`Response`] in one
/// expression: `tip(stat_row(ui, …, "TEMP", …), "TEMP")`.
pub fn tip(response: Response, label: &str) -> Response {
    match describe(label) {
        Some(text) => response.on_hover_text(text),
        None => response,
    }
}

/// The canonical metric → description table. Keep entries short and concrete,
/// keys unique (lookup returns the first match, so a duplicate key shadows
/// every later entry) and every entry wired to a caller — dead text rots.
#[rustfmt::skip]
const DESCRIPTIONS: &[(&str, &str)] = &[
    // ---- CPU panel ----
    ("TEMP",     "Hottest temperature reported by the component, in °C. Sustained values above ~85 °C on a CPU or ~83 °C on a GPU usually trigger thermal throttling."),
    ("CLOCK",    "Highest current core frequency, in GHz. Modern CPUs and GPUs boost above their base clock when there is thermal and power headroom."),
    ("POWER",    "Component power draw, in watts. Combines every on-die power domain reported by the silicon (cores, cache, memory controller, platform)."),
    ("VCORE",    "CPU core voltage rail. Typical idle/load range is 0.8–1.4 V. Unstable voltage causes crashes and graphical artefacts."),
    ("TJMAX",    "Headroom to the CPU's thermal throttle point (TjMax), in °C. 0 = throttling right now. Smaller numbers mean the cooler is at its limit."),

    // ---- GPU panel ----
    ("HOTSPOT",  "GPU die hot spot — the warmest single point on the GPU silicon. Usually 10–15 °C above the averaged core temperature."),
    ("JUNCTION", "GDDR / VRAM memory die hot spot, in °C. The VRAM chips can exceed the GPU core in memory-heavy workloads (large textures, on-GPU inference)."),
    ("VRAM",     "GPU on-card video memory used out of the card's total. Split between dedicated and DXGI-shared system RAM where the driver reports it."),
    ("MEM USE",  "VRAM memory-controller load, 0–100 %. Independent of GPU core load: a saturated bus can throttle even an idle-looking GPU."),
    ("PCIE",     "GPU PCIe link throughput (receive + transmit), bytes per second. Sustained saturation can bottleneck game streaming or model loading."),
    ("V",        "GPU core voltage rail, in volts."),

    // ---- RAM panel ----
    ("USED",     "Memory currently allocated by running processes. Does not include OS file cache (see CACHED)."),
    ("FREE",     "Memory immediately available for new allocations. Does not count reclaimable cache pages — Windows can release the cache on demand."),
    ("CACHED",   "OS file-system cache. The kernel treats this as free memory and reclaims it transparently when applications need RAM."),
    ("DIMM",     "Hottest DIMM temperature on the board, in °C. Some platforms expose per-module thermal sensors; sustained values above ~85 °C usually indicate weak case airflow."),

    // ---- Battery panel (HEALTH) ----
    ("HEALTH",   "Remaining capacity relative to the design capacity, 0–100 % (100 = new). Computed from the embedded controller's full-charge and design figures."),

    // ---- Network panel ----
    ("DOWN",     "Inbound throughput on the active network adapter, bytes per second."),
    ("UP",       "Outbound throughput on the active network adapter, bytes per second."),
    ("LINK",     "Adapter's negotiated link speed (for example 1 Gbit/s or 2.4 Gbit/s). Empty when the OS does not report it."),
    ("WI-FI",    "Connected wireless network: SSID, signal quality and the negotiated PHY rate (the radio link's current top speed, not your internet speed)."),
    ("IFACE",    "Active network interface (Wi-Fi adapter name or Ethernet device). Selected as the interface currently passing traffic."),

    // ---- Battery panel ----
    ("STATE",     "Battery charge / discharge state. Charging while plugged in; Discharging on battery; AC when fully charged."),
    ("RATE",      "Instantaneous power flow. Positive while charging, negative while discharging. A heavier load makes the discharge rate more negative."),
    ("TIME",      "OS estimate of remaining battery life at the current discharge rate. Updates lazily; treat short-term changes as noisy."),
    ("REMAINING", "Current charge level, 0–100 %. Mirrors the donut on the left of the card."),

    // ---- Sensors panel ----
    ("BOARD",    "Motherboard temperature sensor, in °C. Tracks chipset / chassis warm-up; sustained values above 50 °C indicate poor case airflow."),
    ("VRM",      "Voltage Regulator Module temperature, in °C. Hot VRMs throttle CPU power delivery; values above 90 °C suggest a heatsink or airflow problem."),

    // ---- Footer ----
    // Keyed "UPTIME", not "UP": the network panel already owns "UP" and a
    // duplicate key would shadow this entry (lookup returns the first match).
    ("UPTIME",   "Time since the OS last booted. Resets after a full shutdown or restart, not after sleep."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_is_case_insensitive() {
        assert!(describe("TEMP").is_some());
        assert!(describe("temp").is_some());
        assert!(describe("Temp").is_some());
    }

    #[test]
    fn describe_trims_whitespace() {
        assert!(describe("  POWER  ").is_some());
    }

    #[test]
    fn describe_returns_none_for_unknown_keys() {
        assert!(describe("ZZZ-NONEXISTENT").is_none());
    }

    #[test]
    fn every_descriptions_entry_is_non_empty() {
        for (key, value) in DESCRIPTIONS {
            assert!(!key.is_empty(), "empty key in DESCRIPTIONS");
            assert!(!value.is_empty(), "empty value for key {key}");
        }
    }

    #[test]
    fn keys_are_unique_so_no_entry_is_shadowed() {
        let mut seen = std::collections::HashSet::new();
        for (key, _) in DESCRIPTIONS {
            assert!(
                seen.insert(key.to_ascii_uppercase()),
                "duplicate DESCRIPTIONS key: {key}"
            );
        }
    }

    #[test]
    fn uptime_and_network_up_are_distinct_entries() {
        let uptime = describe("UPTIME").expect("UPTIME entry exists");
        let up = describe("UP").expect("UP entry exists");
        assert!(uptime.contains("booted"));
        assert!(up.contains("Outbound"));
    }
}
