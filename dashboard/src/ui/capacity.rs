//! Per-panel rendering budget computed from the card's allocated rectangle.
//!
//! `rows` is the maximum number of priority-ranked stat rows a panel should
//! emit; `columns` is whether to render them in a 2-column split or stacked
//! single-column. Each panel publishes a ranked candidate list and the
//! renderer selects as many top priorities as `rows` allows.
//!
//! The budget is derived from the card's *allocated* rectangle, both width and
//! height: as the zoom-to-fit layout shrinks a card the height budget drops
//! first, so a card sheds its lowest-priority rows before they can overflow
//! the frame.

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

    /// Decide the rendering budget from the card's allocated width *and*
    /// height, in points.
    ///
    /// Width sets whether the rows can split into two columns at all (below
    /// ~300 px a second column is not worth the lost label room). Height then
    /// sets how many rows fit below whatever fixed furniture the panel paints
    /// around them (title, donut, legend, sparkline). The thresholds are the
    /// exact content heights measured for the tallest panel at each tier, so a
    /// card handed a row height always gets a stat budget that fits inside it:
    /// the eleven-candidate GPU list needs 302 px, eight candidates 238 px,
    /// six 212 px, and below that the two-column split removes rows in pairs.
    ///
    /// `rows` is capped at 11, the longest public candidate list (GPU).
    pub fn from_card_size(width: f32, height: f32) -> Self {
        // A card spends ~118 px on padding plus its load donut before the stat
        // block starts, so two columns only pay off once the card is wide
        // enough to leave each column a label and a value.
        let columns = if width >= 300.0 { 2 } else { 1 };
        if columns == 1 {
            // Single-column stats stack, so a row costs a full 22 px and the
            // load donut (86 px) dominates past three rows. Three is the most
            // a narrow card can show without pushing its own height.
            return Capacity {
                rows: 3,
                columns: 1,
            };
        }
        let rows = if height >= 302.0 {
            11
        } else if height >= 238.0 {
            8
        } else if height >= 212.0 {
            6
        } else if height >= 140.0 {
            4
        } else {
            2
        };
        Capacity { rows, columns }
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

    #[test]
    fn width_and_height_together_set_the_budget() {
        assert_eq!(
            Capacity::from_card_size(320.0, 320.0),
            Capacity {
                rows: 11,
                columns: 2
            }
        );
    }

    #[test]
    fn height_tiers_drop_rows_in_pairs() {
        assert_eq!(Capacity::from_card_size(320.0, 302.0).rows, 11);
        assert_eq!(Capacity::from_card_size(320.0, 238.0).rows, 8);
        assert_eq!(Capacity::from_card_size(320.0, 212.0).rows, 6);
        assert_eq!(Capacity::from_card_size(320.0, 140.0).rows, 4);
        assert_eq!(Capacity::from_card_size(320.0, 139.0).rows, 2);
    }

    #[test]
    fn a_full_height_card_is_full_at_its_measured_height() {
        // 302 px is the measured content height of the eleven-candidate GPU
        // card, so it must still resolve to the full budget.
        assert_eq!(Capacity::from_card_size(320.0, 302.0).rows, 11);
    }

    #[test]
    fn a_narrow_card_collapses_to_one_column_whatever_its_height() {
        assert_eq!(
            Capacity::from_card_size(299.0, 400.0),
            Capacity {
                rows: 3,
                columns: 1
            }
        );
    }
}
