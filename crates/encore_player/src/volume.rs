pub(crate) const MAX_BOOST_PERCENT: i64 = 200;

pub(crate) fn perceptual_to_mpv(percent: i64) -> f64 {
    let percent = percent.clamp(0, MAX_BOOST_PERCENT);
    match percent {
        0 => 0.0,
        1..=100 => 100.0 * 10f64.powf(-(1.0 - percent as f64 / 100.0).powf(1.5)),
        _ => 100.0 * (percent as f64 / 100.0).powf(1.0 / 3.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_below_are_a_hard_mute() {
        assert_eq!(perceptual_to_mpv(0), 0.0);
        assert_eq!(perceptual_to_mpv(-5), 0.0);
    }

    #[test]
    fn full_volume_is_mpvs_unity_gain() {
        assert_eq!(perceptual_to_mpv(100), 100.0);
    }

    #[test]
    fn a_percent_above_100_boosts_past_unity_gain() {
        assert!(perceptual_to_mpv(150) > perceptual_to_mpv(100));
    }

    #[test]
    fn two_hundred_percent_is_double_the_amplitude_of_one_hundred() {
        let unity = perceptual_to_mpv(100);
        let boosted = perceptual_to_mpv(200);
        let unity_gain = (unity / 100.0).powf(3.0);
        let boosted_gain = (boosted / 100.0).powf(3.0);
        assert!((boosted_gain / unity_gain - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_percent_past_the_boost_ceiling_is_clamped_to_it() {
        assert_eq!(perceptual_to_mpv(250), perceptual_to_mpv(MAX_BOOST_PERCENT));
        assert_eq!(perceptual_to_mpv(1_000), perceptual_to_mpv(MAX_BOOST_PERCENT));
    }

    #[test]
    fn volume_increases_monotonically_across_the_whole_range() {
        let steps: Vec<f64> = (0..=MAX_BOOST_PERCENT)
            .step_by(10)
            .map(perceptual_to_mpv)
            .collect();
        assert!(steps.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
