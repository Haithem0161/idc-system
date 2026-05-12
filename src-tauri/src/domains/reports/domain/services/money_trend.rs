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
}
