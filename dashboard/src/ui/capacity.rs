//! Per-panel rendering budget computed from the card's allocated rectangle.
//!
//! `rows` is the maximum number of priority-ranked stat rows a panel should
//! emit; `columns` is whether to render them in a 2-column split or stacked
//! single-column. Each panel publishes a ranked candidate list and the
//! renderer selects as many top priorities as `rows` allows.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacity {
    pub rows: usize,
    pub columns: usize,
}

impl Capacity {
    /// Decide the rendering budget from the card's allocated width.
    ///
    /// Thresholds are tuned so that:
    /// - the default 1180x600 viewport (4 cols x ~287 px) stays Full,
    /// - 720x500 (2 cols x ~340 px) stays Full,
    /// - shrinking 820x500 to 3 cols x ~263 px keeps the priority-3 rows,
    /// - very narrow forced widths collapse to a single column with the
    ///   top 3 priorities only.
    ///
    /// `rows: 11` covers every current panel's full candidate list (GPU has
    /// eleven as of v0.10.0: TEMP, VRAM, POWER, CLOCK, MEM USE, HOTSPOT,
    /// JUNCTION, PCIE, V, MEM CLK, VIDEO). CPU/RAM/Battery/Network publish
    /// 4–5 each, so the higher cap is a no-op for them; the Sensors panel
    /// applies its own lower cap to fit its fixed card height.
    pub fn from_card_width(width: f32) -> Self {
        if width >= 260.0 {
            Capacity {
                rows: 11,
                columns: 2,
            }
        } else if width >= 180.0 {
            Capacity {
                rows: 4,
                columns: 2,
            }
        } else {
            Capacity {
                rows: 3,
                columns: 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_or_above_260_is_full_two_cols() {
        assert_eq!(
            Capacity::from_card_width(260.0),
            Capacity {
                rows: 11,
                columns: 2
            }
        );
        assert_eq!(
            Capacity::from_card_width(287.0),
            Capacity {
                rows: 11,
                columns: 2
            }
        );
        assert_eq!(
            Capacity::from_card_width(800.0),
            Capacity {
                rows: 11,
                columns: 2
            }
        );
    }

    #[test]
    fn between_180_and_260_is_compact_two_cols() {
        assert_eq!(
            Capacity::from_card_width(180.0),
            Capacity {
                rows: 4,
                columns: 2
            }
        );
        assert_eq!(
            Capacity::from_card_width(220.0),
            Capacity {
                rows: 4,
                columns: 2
            }
        );
        assert_eq!(
            Capacity::from_card_width(259.999),
            Capacity {
                rows: 4,
                columns: 2
            }
        );
    }

    #[test]
    fn below_180_is_tiny_one_col() {
        assert_eq!(
            Capacity::from_card_width(0.0),
            Capacity {
                rows: 3,
                columns: 1
            }
        );
        assert_eq!(
            Capacity::from_card_width(179.999),
            Capacity {
                rows: 3,
                columns: 1
            }
        );
    }
}
