use crate::{
    model::mods::GameMods,
    osu::{
        attributes::{OsuDifficultyAttributes, OsuPerformanceAttributes},
        difficulty::skills::{flashlight::Flashlight, strain::OsuStrainSkill},
        score_state::OsuScoreState,
    },
    util::float_ext::FloatExt,
};

use super::PERFORMANCE_BASE_MULTIPLIER;

pub(super) struct OsuPerformanceInner<'mods> {
    pub(super) attrs: OsuDifficultyAttributes,
    pub(super) mods: &'mods GameMods,
    pub(super) acc: f64,
    pub(super) state: OsuScoreState,
    pub(super) effective_miss_count: f64,
    pub(super) using_classic_slider_acc: bool,
}

impl OsuPerformanceInner<'_> {
    pub(super) fn calculate(mut self) -> OsuPerformanceAttributes {
        let total_hits = self.state.total_hits();

        if total_hits == 0 {
            return OsuPerformanceAttributes {
                difficulty: self.attrs,
                ..Default::default()
            };
        }

        let total_hits = f64::from(total_hits);

        let mut multiplier = PERFORMANCE_BASE_MULTIPLIER;
        // * Global compensation buff so recent AR changes are net-positive overall.
        multiplier *= 1.07;

        if self.mods.nf() {
            multiplier *= (1.0 - 0.02 * self.effective_miss_count).max(0.9);
        }

        if self.mods.so() && total_hits > 0.0 {
            multiplier *= 1.0 - (f64::from(self.attrs.n_spinners) / total_hits).powf(0.85);
        }

        if self.mods.rx() {
            // * https://www.desmos.com/calculator/bc9eybdthb
            // * we use OD13.3 as maximum since it's the value at which great hitwidow becomes 0
            // * this is well beyond currently maximum achievable OD which is 12.17 (DTx2 + DA with OD11)
            let (n100_mult, n50_mult) = if self.attrs.od > 0.0 {
                (
                    (1.0 - (self.attrs.od / 13.33).powf(1.8)).max(0.0),
                    (1.0 - (self.attrs.od / 13.33).powf(5.0)).max(0.0),
                )
            } else {
                (1.0, 1.0)
            };

            // * As we're adding Oks and Mehs to an approximated number of combo breaks the result can be
            // * higher than total hits in specific scenarios (which breaks some calculations) so we need to clamp it.
            self.effective_miss_count = (self.effective_miss_count
                + f64::from(self.state.n100) * n100_mult
                + f64::from(self.state.n50) * n50_mult)
                .min(total_hits);
        }

        let aim_value = self.compute_aim_value();
        let speed_value = self.compute_speed_value();
        let acc_value = self.compute_accuracy_value();
        let flashlight_value = self.compute_flashlight_value();

        let pp = (aim_value.powf(1.1)
            + speed_value.powf(1.1)
            + acc_value.powf(1.1)
            + flashlight_value.powf(1.1))
        .powf(1.0 / 1.1)
            * multiplier;

        OsuPerformanceAttributes {
            difficulty: self.attrs,
            pp_acc: acc_value,
            pp_aim: aim_value,
            pp_flashlight: flashlight_value,
            pp_speed: speed_value,
            pp,
            effective_miss_count: self.effective_miss_count,
        }
    }

    fn compute_aim_value(&self) -> f64 {
        if self.mods.ap() {
            return 0.0;
        }

        let mut aim_value = OsuStrainSkill::difficulty_to_performance(self.attrs.aim);

        let total_hits = self.total_hits();

        let len_bonus = 0.95
            + 0.4 * (total_hits / 2000.0).min(1.0)
            + f64::from(u8::from(total_hits > 2000.0)) * (total_hits / 2000.0).log10() * 0.5;

        aim_value *= len_bonus;

        if self.effective_miss_count > 0.0 {
            aim_value *= Self::calculate_miss_penalty(
                self.effective_miss_count,
                self.attrs.aim_difficult_strain_count,
            );
        }

        let ar_bonus = if self.mods.rx() {
            0.0
        } else if self.attrs.ar > 10.33 {
            0.0
        } else if self.attrs.ar > 10.2 {
            0.03 / 0.13 * (10.33 - self.attrs.ar)
        } else if self.attrs.ar > 10.0 {
            0.03 + 0.35 * (10.2 - self.attrs.ar)
        } else if self.attrs.ar >= 9.0 {
            0.1 + 0.1 * (10.0 - self.attrs.ar)
        } else if self.attrs.ar < 8.0 {
            0.05 * (8.0 - self.attrs.ar)
        } else {
            0.0
        };

        aim_value *= 1.0 + ar_bonus * len_bonus;

        if self.mods.bl() {
            aim_value *= 1.3
                + (total_hits
                    * (0.0016 / (1.0 + 2.0 * self.effective_miss_count))
                    * self.acc.powf(16.0))
                    * (1.0 - 0.003 * self.attrs.hp * self.attrs.hp);
        } else if self.mods.hd() || self.mods.tc() {
            // * We want to give more reward for lower AR when it comes to aim and HD. This nerfs high AR and buffs lower AR.
            // * AR above 10.33 should not get any AR bonus.
            let ar_bonus = if self.attrs.ar > 10.33 {
                1.0
            } else {
                1.0 + 0.04 * (12.0 - self.attrs.ar)
            };

            aim_value *= ar_bonus;
        }

        // * We assume 15% of sliders in a map are difficult since there's no way to tell from the performance calculator.
        let estimate_diff_sliders = f64::from(self.attrs.n_sliders) * 0.15;

        if self.attrs.n_sliders > 0 {
            let estimate_improperly_followed_difficult_sliders = if self.using_classic_slider_acc {
                // * When the score is considered classic (regardless if it was made on old client or not) we consider all missing combo to be dropped difficult sliders
                let maximum_possible_droppled_sliders = total_imperfect_hits(&self.state);

                maximum_possible_droppled_sliders
                    .min(f64::from(self.attrs.max_combo - self.state.max_combo))
                    .clamp(0.0, estimate_diff_sliders)
            } else {
                // * We add tick misses here since they too mean that the player didn't follow the slider properly
                // * We however aren't adding misses here because missing slider heads has a harsh penalty by itself and doesn't mean that the rest of the slider wasn't followed properly
                (f64::from(
                    n_slider_ends_dropped(&self.attrs, &self.state)
                        + n_large_tick_miss(&self.attrs, &self.state),
                ))
                .min(estimate_diff_sliders)
            };

            let slider_nerf_factor = (1.0 - self.attrs.slider_factor)
                * (1.0 - estimate_improperly_followed_difficult_sliders / estimate_diff_sliders)
                    .powf(3.0)
                + self.attrs.slider_factor;
            aim_value *= slider_nerf_factor;
        }

        aim_value *= self.acc;
        // * It is important to consider accuracy difficulty when scaling with accuracy.
        aim_value *= 0.98 + self.attrs.od.powf(2.0) / 2500.0;

        aim_value
    }

    fn compute_speed_value(&self) -> f64 {
        if self.mods.rx() {
            return 0.0;
        }

        let mut speed_value = OsuStrainSkill::difficulty_to_performance(self.attrs.speed);

        let total_hits = self.total_hits();

        let len_bonus = 0.95
            + 0.4 * (total_hits / 2000.0).min(1.0)
            + f64::from(u8::from(total_hits > 2000.0)) * (total_hits / 2000.0).log10() * 0.5;

        speed_value *= len_bonus;

        if self.effective_miss_count > 0.0 {
            speed_value *= Self::calculate_miss_penalty(
                self.effective_miss_count,
                self.attrs.speed_difficult_strain_count,
            );
        }

        let ar_bonus = if self.mods.ap() {
            0.0
        } else if self.attrs.ar > 10.33 {
            0.0
        } else if self.attrs.ar > 10.2 {
            0.03 / 0.13 * (10.33 - self.attrs.ar)
        } else if self.attrs.ar > 10.0 {
            0.03 + 0.35 * (10.2 - self.attrs.ar)
        } else if self.attrs.ar >= 9.0 {
            0.1 + 0.1 * (10.0 - self.attrs.ar)
        } else {
            0.0
        };

        speed_value *= 1.0 + ar_bonus * len_bonus;

        if self.mods.bl() {
            // * Increasing the speed value by object count for Blinds isn't
            // * ideal, so the minimum buff is given.
            speed_value *= 1.12;
        } else if self.mods.hd() || self.mods.tc() {
            // * We want to give more reward for lower AR when it comes to aim and HD.
            // * This nerfs high AR and buffs lower AR.
            // * AR above 10.33 should not get any AR bonus.
            let ar_bonus = if self.attrs.ar > 10.33 {
                1.0
            } else {
                1.0 + 0.04 * (12.0 - self.attrs.ar)
            };

            speed_value *= ar_bonus;
        }

        // * Calculate accuracy assuming the worst case scenario
        let relevant_total_diff = total_hits - self.attrs.speed_note_count;
        let relevant_n300 = (f64::from(self.state.n300) - relevant_total_diff).max(0.0);
        let relevant_n100 = (f64::from(self.state.n100)
            - (relevant_total_diff - f64::from(self.state.n300)).max(0.0))
        .max(0.0);
        let relevant_n50 = (f64::from(self.state.n50)
            - (relevant_total_diff - f64::from(self.state.n300 + self.state.n100)).max(0.0))
        .max(0.0);

        let relevant_acc = if self.attrs.speed_note_count.eq(0.0) {
            0.0
        } else {
            (relevant_n300 * 6.0 + relevant_n100 * 2.0 + relevant_n50)
                / (self.attrs.speed_note_count * 6.0)
        };

        // * Scale the speed value with accuracy and OD.
        speed_value *= (0.95 + self.attrs.od * self.attrs.od / 750.0)
            * ((self.acc + relevant_acc) / 2.0).powf((14.5 - self.attrs.od) / 2.0);

        // * Scale the speed value with # of 50s to punish doubletapping.
        speed_value *= 0.99_f64.powf(
            f64::from(u8::from(f64::from(self.state.n50) >= total_hits / 500.0))
                * (f64::from(self.state.n50) - total_hits / 500.0),
        );

        speed_value
    }

    fn compute_accuracy_value(&self) -> f64 {
        if self.mods.rx() {
            return 0.0;
        }

        // * This percentage only considers HitCircles of any value - in this part
        // * of the calculation we focus on hitting the timing hit window.
        let mut amount_hit_objects_with_acc = self.attrs.n_circles;

        if !self.using_classic_slider_acc {
            amount_hit_objects_with_acc += self.attrs.n_sliders;
        }

        let mut better_acc_percentage = if amount_hit_objects_with_acc > 0 {
            f64::from(
                (self.state.n300 as i32
                    - (self.state.total_hits() as i32 - amount_hit_objects_with_acc as i32))
                    * 6
                    + self.state.n100 as i32 * 2
                    + self.state.n50 as i32,
            ) / f64::from(amount_hit_objects_with_acc * 6)
        } else {
            0.0
        };

        // * It is possible to reach a negative accuracy with this formula. Cap it at zero - zero points.
        if better_acc_percentage < 0.0 {
            better_acc_percentage = 0.0;
        }

        // * Lots of arbitrary values from testing.
        // * Considering to use derivation from perfect accuracy in a probabilistic manner - assume normal distribution.
        let mut acc_value =
            1.52163_f64.powf(self.attrs.od) * better_acc_percentage.powf(24.0) * 2.83;

        // * Bonus for many hitcircles - it's harder to keep good accuracy up for longer.
        acc_value *= (f64::from(amount_hit_objects_with_acc) / 1000.0)
            .powf(0.3)
            .min(1.15);

        // * Increasing the accuracy value by object count for Blinds isn't
        // * ideal, so the minimum buff is given.
        if self.mods.bl() {
            acc_value *= 1.14;
        } else if self.mods.hd() || self.mods.tc() {
            acc_value *= 1.08;
        }

        if self.mods.fl() {
            acc_value *= 1.02;
        }

        acc_value
    }

    fn compute_flashlight_value(&self) -> f64 {
        if !self.mods.fl() {
            return 0.0;
        }

        let mut flashlight_value = Flashlight::difficulty_to_performance(self.attrs.flashlight);

        let total_hits = self.total_hits();

        // * Penalize misses by assessing # of misses relative to the total # of objects. Default a 3% reduction for any # of misses.
        if self.effective_miss_count > 0.0 {
            flashlight_value *= 0.97
                * (1.0 - (self.effective_miss_count / total_hits).powf(0.775))
                    .powf(self.effective_miss_count.powf(0.875));
        }

        flashlight_value *= self.get_combo_scaling_factor();

        // * Account for shorter maps having a higher ratio of 0 combo/100 combo flashlight radius.
        flashlight_value *= 0.7
            + 0.1 * (total_hits / 200.0).min(1.0)
            + f64::from(u8::from(total_hits > 200.0))
                * 0.2
                * ((total_hits - 200.0) / 200.0).min(1.0);

        // * Scale the flashlight value with accuracy _slightly_.
        flashlight_value *= 0.5 + self.acc / 2.0;
        // * It is important to also consider accuracy difficulty when doing that.
        flashlight_value *= 0.98 + self.attrs.od.powf(2.0) / 2500.0;

        flashlight_value
    }

    // * Miss penalty assumes that a player will miss on the hardest parts of a map,
    // * so we use the amount of relatively difficult sections to adjust miss penalty
    // * to make it more punishing on maps with lower amount of hard sections.
    fn calculate_miss_penalty(miss_count: f64, diff_strain_count: f64) -> f64 {
        0.96 / ((miss_count / (4.0 * diff_strain_count.ln().powf(0.94))) + 1.0)
    }

    fn get_combo_scaling_factor(&self) -> f64 {
        if self.attrs.max_combo == 0 {
            1.0
        } else {
            (f64::from(self.state.max_combo).powf(0.8) / f64::from(self.attrs.max_combo).powf(0.8))
                .min(1.0)
        }
    }

    const fn total_hits(&self) -> f64 {
        self.state.total_hits() as f64
    }
}

pub(super) fn total_imperfect_hits(state: &OsuScoreState) -> f64 {
    f64::from(state.n100 + state.n50 + state.misses)
}

pub(super) const fn n_slider_ends_dropped(
    attrs: &OsuDifficultyAttributes,
    state: &OsuScoreState,
) -> u32 {
    attrs.n_sliders - state.slider_end_hits
}

pub(super) const fn n_large_tick_miss(
    attrs: &OsuDifficultyAttributes,
    state: &OsuScoreState,
) -> u32 {
    attrs.n_large_ticks - state.large_tick_hits
}
