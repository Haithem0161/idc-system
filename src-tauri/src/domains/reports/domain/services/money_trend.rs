//! Pure trend math. Computes a `TrendCell` from a `(current, prior)` pair.
//! Permille (parts-per-thousand) avoids floats over IPC.

use crate::domains::reports::domain::entities::TrendCell;

pub struct TrendInputs {
    pub current: i64,
    pub prior: i64,
}

pub fn trend_cell(inp: TrendInputs) -> TrendCell {
    TrendCell {
        current_iqd: inp.current,
        prior_iqd: inp.prior,
        delta_iqd: inp.current.saturating_sub(inp.prior),
        delta_permille: permille_change(inp.current, inp.prior),
    }
}

/// (current - prior) / prior expressed in permille. Prior=0 returns 0 (no
/// baseline to compare against; the UI can decide to render `—`).
pub fn permille_change(current: i64, prior: i64) -> i64 {
    if prior == 0 {
        return 0;
    }
    // Saturating arithmetic: even pathological year-end values stay i64.
    let delta = current.saturating_sub(prior);
    delta.saturating_mul(1000).checked_div(prior).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permille_handles_zero_prior() {
        assert_eq!(permille_change(100, 0), 0);
    }

    #[test]
    fn permille_basic_50pct() {
        // 150 vs 100 = +50.0% = 500 permille.
        assert_eq!(permille_change(150, 100), 500);
    }

    #[test]
    fn permille_negative() {
        assert_eq!(permille_change(80, 100), -200);
    }

    #[test]
    fn trend_cell_arithmetic() {
        let t = trend_cell(TrendInputs {
            current: 200,
            prior: 100,
        });
        assert_eq!(t.delta_iqd, 100);
        assert_eq!(t.delta_permille, 1000);
    }

    #[test]
    fn trend_cell_zero_prior_renders_zero_permille() {
        let t = trend_cell(TrendInputs {
            current: 500,
            prior: 0,
        });
        assert_eq!(t.delta_iqd, 500);
        assert_eq!(t.delta_permille, 0);
    }

    #[test]
    fn trend_cell_carries_current_and_prior_unchanged() {
        let t = trend_cell(TrendInputs {
            current: 123_456,
            prior: 100_000,
        });
        assert_eq!(t.current_iqd, 123_456);
        assert_eq!(t.prior_iqd, 100_000);
        assert_eq!(t.delta_iqd, 23_456);
        // 23456 * 1000 / 100000 = 234 permille.
        assert_eq!(t.delta_permille, 234);
    }

    #[test]
    fn permille_negative_below_minus_one_thousand_clamps_at_minus_thousand() {
        // current 0, prior 200 -> -200 * 1000 / 200 = -1000.
        assert_eq!(permille_change(0, 200), -1000);
    }

    #[test]
    fn permille_extreme_growth_handles_large_factors() {
        // 10x growth: (1_000_000 - 100_000) * 1000 / 100_000 = 9000.
        assert_eq!(permille_change(1_000_000, 100_000), 9_000);
    }

    /// Saturating arithmetic keeps pathological i64 inputs in-range.
    #[test]
    fn permille_saturates_on_i64_overflow_inputs() {
        // (i64::MAX - 1) * 1000 would overflow; saturating_mul keeps us
        // bounded.
        let v = permille_change(i64::MAX, 1);
        assert_eq!(v, i64::MAX);
    }

    #[test]
    fn permille_zero_current_zero_prior_is_zero() {
        assert_eq!(permille_change(0, 0), 0);
    }
}
