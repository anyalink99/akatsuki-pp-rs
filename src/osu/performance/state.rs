use super::super::score_state::OsuScoreOrigin;

/// Score state variant that omits combo and is only used for hitresult
/// reconstruction during accuracy target matching.
pub(super) struct NoComboState {
    pub(super) n300: u32,
    pub(super) n100: u32,
    pub(super) n50: u32,
    pub(super) misses: u32,
    pub(super) large_tick_hits: u32,
    pub(super) slider_end_hits: u32,
}

impl NoComboState {
    pub(super) fn accuracy(&self, origin: OsuScoreOrigin) -> f64 {
        let mut numerator = 300 * self.n300 + 100 * self.n100 + 50 * self.n50;
        let mut denominator = 300 * (self.n300 + self.n100 + self.n50 + self.misses);

        match origin {
            OsuScoreOrigin::Stable => {}
            OsuScoreOrigin::WithSliderAcc {
                max_large_ticks,
                max_slider_ends,
            } => {
                let slider_end_hits = self.slider_end_hits.min(max_slider_ends);
                let large_tick_hits = self.large_tick_hits.min(max_large_ticks);

                numerator += 150 * slider_end_hits + 30 * large_tick_hits;
                denominator += 150 * max_slider_ends + 30 * max_large_ticks;
            }
            OsuScoreOrigin::WithoutSliderAcc {
                max_large_ticks,
                max_slider_ends,
            } => {
                let large_tick_hits = self.large_tick_hits.min(max_large_ticks);
                let slider_end_hits = self.slider_end_hits.min(max_slider_ends);

                numerator += 30 * large_tick_hits + 10 * slider_end_hits;
                denominator += 30 * max_large_ticks + 10 * max_slider_ends;
            }
        }

        if denominator == 0 {
            0.0
        } else {
            f64::from(numerator) / f64::from(denominator)
        }
    }
}
