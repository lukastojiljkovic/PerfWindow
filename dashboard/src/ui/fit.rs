//! Zoom-to-fit geometry for the adaptive card grid.
//!
//! Everything here is pure arithmetic on zoom-1.0 point sizes, so it is
//! unit-tested without an egui context. The renderer measures the window in
//! *physical* pixels and divides by the OS scale factor to get these numbers,
//! which keeps the decision invariant under the zoom it applies (a zoom that
//! shrank its own input would oscillate).
//!
//! The layout is a fixed set of cards. Each card publishes a ladder of content
//! heights, fullest first — 287-point row-1 cards with an eleven-candidate GPU
//! need ~302 points, a pinned sparkline needs ~212, and so on. For each
//! candidate column count the grid's natural footprint is derived from the
//! fullest rung; the column count whose footprint best fills the available
//! area wins, and a zoom factor scales the whole UI (chrome included) so the
//! layout fills the window. When even the smallest zoom cannot fit the
//! footprint, [`shed_for`] picks the least destructive rung that does.

/// Comfortable per-card column width, in points. The 4-column layout at the
/// design size (1180x600) resolves to this width, so it is the natural unit
/// the grid is measured in.
pub const NATURAL_CARD_WIDTH: f32 = 287.0;
/// Gap between cards in the grid, in points.
pub const GRID_GAP: f32 = 10.0;
/// Padding around the grid body, in points.
pub const GRID_PADDING: f32 = 13.0;
/// Smallest zoom the UI is allowed to apply.
pub const MIN_ZOOM: f32 = 0.75;
/// Largest zoom the UI is allowed to apply.
pub const MAX_ZOOM: f32 = 2.5;
/// A recomputed zoom closer than this (relatively) to the current one is left
/// alone, so the renderer never nudges the scale factor frame by frame.
pub const ZOOM_EPSILON: f32 = 0.01;
/// Candidate column counts, most columns first so ties resolve to more.
pub const COLUMN_CANDIDATES: [usize; 3] = [4, 3, 2];

/// One rung of a card's content ladder: the outer height the card needs when
/// its stat block is capped at `rows` entries, and the value the card's
/// [`crate::ui::capacity::Capacity`] must report at that height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardTier {
    pub height: f32,
    pub rows: usize,
}

/// One card's contribution to the grid geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct CardMetrics {
    /// Base column span, before the elastic storage card expands.
    pub span: usize,
    /// The storage card: it absorbs whatever columns its row leaves free.
    pub elastic: bool,
    /// Network / Battery / Sensors reserve a column for themselves, which is
    /// what the elastic card's span is computed against.
    pub reserves: bool,
    /// Content heights, fullest first. The last rung is the card's floor: it
    /// is never given less than this.
    pub tiers: Vec<CardTier>,
}

impl CardMetrics {
    /// Height at ladder rung `tier`, clamped to the card's own last rung.
    fn height_at(&self, tier: usize) -> f32 {
        self.tiers[tier.min(self.tiers.len() - 1)].height
    }
}

/// The chosen layout for one frame: how many columns, the zoom to apply, and
/// the natural footprint at that zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub cols: usize,
    pub zoom: f32,
    /// Natural grid width at zoom 1.0.
    pub width: f32,
    /// Natural grid height at zoom 1.0.
    pub height: f32,
    /// Height the grid may occupy once `zoom` is applied.
    pub target_height: f32,
}

/// Rung of the global content-shedding ladder. `Shed(0)` is full content;
/// each step drops the next-lowest-priority rows from every card that still
/// has something to drop, until only every card's floor rung remains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Shed(pub usize);

/// Identity of a fit input: the physical window size in pixels, the OS scale
/// factor, the height the chrome occupies (in zoom-1.0 points) and a signature
/// of the card set. The renderer caches its plan under this key, so the
/// planner runs only when one of those changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FitKey {
    physical_w: u32,
    physical_h: u32,
    native_milli: u32,
    chrome_milli: u32,
    cards: u64,
}

impl FitKey {
    /// Build a key from the window's physical pixel size, its OS scale factor
    /// (with the chrome height in zoom-1.0 points) and a caller-supplied
    /// signature of the card set. The values are rounded so sub-pixel noise
    /// cannot spuriously invalidate a plan.
    pub fn new(physical_w: f32, physical_h: f32, native: f32, chrome: f32, cards: u64) -> Self {
        Self {
            physical_w: physical_w.max(0.0).round() as u32,
            physical_h: physical_h.max(0.0).round() as u32,
            native_milli: (native.max(0.0) * 1000.0).round() as u32,
            chrome_milli: (chrome.max(0.0) * 1000.0).round() as u32,
            cards,
        }
    }
}

/// Natural (zoom-1.0) width of a `cols`-column grid, including padding.
pub fn natural_width(cols: usize) -> f32 {
    let cols = cols.max(1) as f32;
    cols * NATURAL_CARD_WIDTH + (cols - 1.0) * GRID_GAP + 2.0 * GRID_PADDING
}

/// Resolve each card's column span for a `cols`-wide grid, mirroring the
/// renderer: the elastic storage card fills the columns the reserved row-2
/// cards (network, battery, sensors) leave free.
pub fn effective_spans(cards: &[CardMetrics], cols: usize) -> Vec<usize> {
    let cols = cols.max(1);
    let mut spans: Vec<usize> = cards.iter().map(|c| c.span.max(1).min(cols)).collect();
    let reserved = cards.iter().filter(|c| c.reserves).count();
    for i in 0..cards.len() {
        if !cards[i].elastic {
            continue;
        }
        if reserved > 0 {
            spans[i] = cols.saturating_sub(reserved).max(2).min(cols);
        } else {
            let following: usize = spans[i + 1..].iter().sum();
            if following == 0 {
                spans[i] = cols;
            }
        }
    }
    spans
}

/// Pack `cards` into rows of at most `cols` columns, greedily, mirroring the
/// renderer's placement. Each row is the list of card indices it holds.
fn pack_rows(cards: &[CardMetrics], cols: usize) -> Vec<Vec<usize>> {
    let cols = cols.max(1);
    let spans = effective_spans(cards, cols);
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut idx = 0;
    while idx < cards.len() {
        let mut row = Vec::new();
        let mut used = 0usize;
        while idx < cards.len() {
            let span = spans[idx].max(1);
            if used + span > cols {
                break;
            }
            used += span;
            row.push(idx);
            idx += 1;
        }
        // A card wider than the whole grid still consumes one row, so the
        // loop can never wedge.
        if row.is_empty() {
            row.push(idx);
            idx += 1;
        }
        rows.push(row);
    }
    rows
}

/// Height each row is given at ladder rung `shed`, before any stretching to
/// fill the window. The renderer reuses this so its rows and [`grid_height`]
/// agree exactly.
pub fn row_heights(cards: &[CardMetrics], cols: usize, shed: Shed) -> Vec<f32> {
    pack_rows(cards, cols)
        .iter()
        .map(|row| {
            row.iter()
                .map(|&i| cards[i].height_at(shed.0))
                .fold(0.0_f32, f32::max)
        })
        .collect()
}

/// Natural (zoom-1.0) height of a `cols`-column grid at `shed`, including the
/// inter-row gaps and the grid padding.
pub fn grid_height(cards: &[CardMetrics], cols: usize, shed: Shed) -> f32 {
    let rows = row_heights(cards, cols, shed);
    if rows.is_empty() {
        return 2.0 * GRID_PADDING;
    }
    rows.iter().sum::<f32>() + GRID_GAP * (rows.len() as f32 - 1.0) + 2.0 * GRID_PADDING
}

/// The deepest rung any card can reach.
fn deepest_shed(cards: &[CardMetrics]) -> usize {
    cards
        .iter()
        .map(|c| c.tiers.len().saturating_sub(1))
        .max()
        .unwrap_or(0)
}

/// Pick the least destructive shed rung whose grid height fits `target`
/// (zoom-1.0 points). Returns the deepest rung when even that overflows — the
/// renderer never scrolls, so an impossibly small window simply shows the
/// sparsest possible grid.
pub fn shed_for(cards: &[CardMetrics], cols: usize, target: f32) -> Shed {
    let last = deepest_shed(cards);
    let mut chosen = Shed(last);
    for tier in 0..=last {
        if grid_height(cards, cols, Shed(tier)) <= target {
            chosen = Shed(tier);
            break;
        }
    }
    chosen
}

/// Choose the column count and zoom that best fill `avail` (zoom-1.0 points),
/// then report the height the grid may occupy at that zoom.
///
/// Candidates are 4 / 3 / 2 columns; a single column joins them only when two
/// columns cannot even reach the minimum zoom without overflowing the width.
pub fn plan(cards: &[CardMetrics], avail_w: f32, avail_h: f32) -> Plan {
    let avail_w = avail_w.max(1.0);
    let avail_h = avail_h.max(1.0);

    let mut candidates: Vec<usize> = COLUMN_CANDIDATES.to_vec();
    if avail_w / natural_width(2) < MIN_ZOOM {
        candidates.push(1);
    }

    let mut best: Option<(usize, f32, f32, f32)> = None;
    for &cols in &candidates {
        let width = natural_width(cols);
        let height = grid_height(cards, cols, Shed(0)).max(1.0);
        let fit = (avail_w / width).min(avail_h / height);
        // Strictly greater keeps the earlier (wider) candidate on ties.
        if best.is_none_or(|(_, best_fit, _, _)| fit > best_fit) {
            best = Some((cols, fit, width, height));
        }
    }
    let (cols, fit, width, height) = best.expect("candidate list is never empty");
    let zoom = fit.clamp(MIN_ZOOM, MAX_ZOOM);
    Plan {
        cols,
        zoom,
        width,
        height,
        target_height: avail_h / zoom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seven single-column row-1 cards plus a double-width storage card — the
    /// shape the real grid packs at every size. Heights mirror the measured
    /// ladders: row-1 cards give up sparkline room, storage gives up disks.
    fn sample_cards() -> Vec<CardMetrics> {
        let row1 = CardMetrics {
            span: 1,
            elastic: false,
            reserves: true,
            tiers: vec![
                CardTier {
                    height: 302.0,
                    rows: 11,
                },
                CardTier {
                    height: 238.0,
                    rows: 8,
                },
                CardTier {
                    height: 212.0,
                    rows: 6,
                },
            ],
        };
        let mut cards = vec![row1; 7];
        cards.push(CardMetrics {
            span: 2,
            elastic: true,
            reserves: false,
            tiers: vec![
                CardTier {
                    height: 256.0,
                    rows: 4,
                },
                CardTier {
                    height: 210.0,
                    rows: 3,
                },
                CardTier {
                    height: 164.0,
                    rows: 2,
                },
                CardTier {
                    height: 118.0,
                    rows: 1,
                },
            ],
        });
        cards
    }

    #[test]
    fn wide_window_prefers_four_columns() {
        let plan = plan(&sample_cards(), 3000.0, 700.0);
        assert_eq!(plan.cols, 4);
    }

    #[test]
    fn square_window_prefers_three_columns() {
        // Four columns are wider than the square, so three fit it better.
        let plan = plan(&sample_cards(), 1000.0, 900.0);
        assert_eq!(plan.cols, 3);
    }

    #[test]
    fn tall_window_prefers_two_columns() {
        let plan = plan(&sample_cards(), 800.0, 1600.0);
        assert_eq!(plan.cols, 2);
    }

    #[test]
    fn zoom_is_clamped_to_its_bounds() {
        // A huge window would want a zoom far above the cap.
        let big = plan(&sample_cards(), 20_000.0, 12_000.0);
        assert_eq!(big.zoom, MAX_ZOOM);
        // A cramped one would want a zoom below the floor.
        let small = plan(&sample_cards(), 720.0, 420.0);
        assert_eq!(small.zoom, MIN_ZOOM);
    }

    #[test]
    fn zoom_never_leaves_the_band() {
        for w in [200.0, 720.0, 1180.0, 1920.0, 3840.0] {
            for h in [300.0, 500.0, 1080.0, 2160.0] {
                let plan = plan(&sample_cards(), w, h);
                assert!(
                    (MIN_ZOOM..=MAX_ZOOM).contains(&plan.zoom),
                    "zoom {} out of band at {w}x{h}",
                    plan.zoom
                );
            }
        }
    }

    #[test]
    fn ties_resolve_to_more_columns() {
        let cards = sample_cards();
        let w4 = natural_width(4);
        let h4 = grid_height(&cards, 4, Shed(0));
        assert_eq!(plan(&cards, w4, h4).cols, 4);
    }

    #[test]
    fn shedding_keeps_full_content_when_it_fits() {
        let cards = sample_cards();
        let natural = grid_height(&cards, 4, Shed(0));
        assert_eq!(shed_for(&cards, 4, natural), Shed(0));
        assert_eq!(shed_for(&cards, 4, natural + 100.0), Shed(0));
    }

    #[test]
    fn shedding_walks_down_the_ladder_one_rung_at_a_time() {
        let cards = sample_cards();
        let mut previous = f32::INFINITY;
        for tier in 0..=deepest_shed(&cards) {
            let height = grid_height(&cards, 4, Shed(tier));
            assert!(
                height <= previous,
                "rung {tier} is not shorter than rung {}",
                tier - 1
            );
            previous = height;
            // A target one point above this rung still needs this rung (unless
            // the rung above is the same height, which the ladder allows at
            // saturation).
            if tier > 0 {
                let above = grid_height(&cards, 4, Shed(tier - 1));
                if above > height {
                    assert_eq!(shed_for(&cards, 4, above - 0.5), Shed(tier));
                }
            }
        }
    }

    #[test]
    fn shedding_pins_the_sparkline_before_dropping_more_rows() {
        let cards = sample_cards();
        let natural = grid_height(&cards, 4, Shed(0));
        let pinned = grid_height(&cards, 4, Shed(1));
        assert!(pinned < natural);
        assert_eq!(shed_for(&cards, 4, natural - 1.0), Shed(1));
        assert_eq!(shed_for(&cards, 4, pinned), Shed(1));
    }

    #[test]
    fn shedding_saturates_at_the_deepest_rung() {
        let cards = sample_cards();
        let floor = grid_height(&cards, 4, Shed(deepest_shed(&cards)));
        assert_eq!(
            shed_for(&cards, 4, floor - 1000.0),
            Shed(deepest_shed(&cards))
        );
    }
}
