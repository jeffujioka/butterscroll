//! Smooth scrolling for notched mouse wheels.
//!
//! Each notch adds `step_px × acceleration` pixels to the distance still to travel.
//! The content is pulled toward that target by a critically damped spring: it eases
//! in, reaches its top speed, then glides to a stop, and its speed never jumps, so
//! a new notch mid-glide blends in the way a trackpad flick does. The spring is
//! integrated in closed form, so the motion depends on elapsed time, never on how
//! evenly frames arrive. Sub-pixel remainders carry over so the total emitted
//! always equals the total requested.

use std::time::{Duration, Instant};

use crate::config::ScrollConfig;

/// Frame interval the macOS layer drives `frame` at.
pub const FRAME: Duration = Duration::from_micros(8_333);

/// `omega × duration` at which a unit step is 99% covered: `(1 + x)·e^-x = 0.01`.
const SETTLE_OMEGA_TAUS: f64 = 6.64;

/// Below this much distance left and this speed (px/s) the glide snaps to its end.
const SNAP_PX: f64 = 0.5;
const SNAP_SPEED: f64 = 30.0;

#[derive(Debug, Default)]
pub struct ScrollAnimator {
    /// Distance still to travel per axis.
    remaining: [f64; 2],
    /// Current speed per axis in px/s, signed like `remaining`.
    velocity: [f64; 2],
    carry: [f64; 2],
    last_frame: Option<Instant>,
    /// Axis, sign and time of the previous notch, for acceleration streaks.
    last_notch: Option<(usize, i8, Instant)>,
    streak: u32,
}

impl ScrollAnimator {
    pub fn is_active(&self) -> bool {
        self.remaining != [0.0, 0.0] || self.velocity != [0.0, 0.0] || self.carry.iter().any(|c| c.abs() >= 1.0)
    }

    /// Whether the vertical axis still has distance or speed left.
    pub fn vertical_active(&self) -> bool {
        self.remaining[1] != 0.0 || self.velocity[1] != 0.0 || self.carry[1].abs() >= 1.0
    }

    /// Queue wheel notches. `lines` is `[horizontal, vertical]`, positive = content moves
    /// right/down (CoreGraphics scroll convention).
    pub fn add(&mut self, lines: [i64; 2], now: Instant, cfg: &ScrollConfig) {
        let axis = if lines[1] != 0 { 1 } else { 0 };
        let amount = lines[axis];
        if amount == 0 {
            return;
        }
        let sign = amount.signum() as i8;

        let continues = match self.last_notch {
            Some((a, s, t)) => {
                a == axis && s == sign && now.duration_since(t).as_secs_f64() * 1000.0 <= cfg.accel_window_ms
            }
            None => false,
        };
        let same_direction = matches!(self.last_notch, Some((a, s, _)) if a == axis && s == sign);
        if !same_direction {
            // Reversing must feel instant: drop what was still queued and stop dead.
            self.remaining = [0.0, 0.0];
            self.velocity = [0.0, 0.0];
            self.carry = [0.0, 0.0];
        }
        self.streak = if continues { self.streak + 1 } else { 0 };
        self.last_notch = Some((axis, sign, now));

        let multiplier = if cfg.acceleration {
            (1.0 + cfg.accel_per_notch * self.streak as f64).min(cfg.accel_max)
        } else {
            1.0
        };
        if !self.is_active() {
            self.last_frame = None;
        }
        self.remaining[axis] += amount as f64 * cfg.step_px * multiplier;
    }

    // Indexing keeps the per-axis arrays visibly in lockstep.
    #[allow(clippy::needless_range_loop)]
    /// Advance the animation to `now`; returns whole pixels `[dx, dy]` to post, if any.
    pub fn frame(&mut self, now: Instant, cfg: &ScrollConfig) -> Option<[i32; 2]> {
        if !self.is_active() {
            return None;
        }
        let dt = match self.last_frame {
            Some(prev) => now.saturating_duration_since(prev).min(FRAME * 6),
            None => FRAME,
        };
        self.last_frame = Some(now);

        let omega = SETTLE_OMEGA_TAUS / (cfg.duration_ms / 1000.0);
        let t = dt.as_secs_f64();
        let decay = (-omega * t).exp();
        let mut out = [0i32; 2];
        for axis in 0..2 {
            let (r0, v0) = (self.remaining[axis], self.velocity[axis]);
            let mut r = (r0 + (v0 + omega * r0) * t) * decay;
            let mut v = (v0 - omega * (v0 + omega * r0) * t) * decay;
            if r.abs() < SNAP_PX && v.abs() < SNAP_SPEED {
                r = 0.0;
                v = 0.0;
            }
            self.remaining[axis] = r;
            self.velocity[axis] = v;
            self.carry[axis] += r0 - r;
            let whole = self.carry[axis].trunc();
            self.carry[axis] -= whole;
            out[axis] = whole as i32;
        }
        if self.remaining == [0.0, 0.0] {
            // Round the last sub-pixel so long scrolls don't drift.
            for axis in 0..2 {
                let rounded = self.carry[axis].round();
                out[axis] += rounded as i32;
                self.carry[axis] = 0.0;
            }
        }
        (out != [0, 0]).then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cfg() -> ScrollConfig {
        Config::default().scroll
    }

    /// Runs frames until idle; returns per-frame outputs and elapsed time.
    fn drain(anim: &mut ScrollAnimator, start: Instant, cfg: &ScrollConfig) -> (Vec<[i32; 2]>, Duration) {
        let mut t = start;
        let mut frames = Vec::new();
        while anim.is_active() {
            t += FRAME;
            if let Some(px) = anim.frame(t, cfg) {
                frames.push(px);
            }
            assert!(t - start < Duration::from_secs(5), "animation never settles");
        }
        (frames, t - start)
    }

    fn total(frames: &[[i32; 2]]) -> [i32; 2] {
        frames.iter().fold([0, 0], |a, f| [a[0] + f[0], a[1] + f[1]])
    }

    #[test]
    fn one_notch_scrolls_exactly_step_px_with_ease_out() {
        let cfg = cfg();
        let mut anim = ScrollAnimator::default();
        let t0 = Instant::now();
        anim.add([0, -1], t0, &cfg);
        let (frames, elapsed) = drain(&mut anim, t0, &cfg);
        assert_eq!(total(&frames), [0, -60]);
        assert!(frames.first().unwrap()[1].abs() >= frames.last().unwrap()[1].abs());
        assert!(elapsed <= Duration::from_millis(500), "took {elapsed:?}");
    }

    #[test]
    fn a_notch_eases_in_instead_of_starting_at_full_speed() {
        let cfg = cfg();
        let mut anim = ScrollAnimator::default();
        let t0 = Instant::now();
        anim.add([0, 1], t0, &cfg);
        let (frames, _) = drain(&mut anim, t0, &cfg);
        let speeds: Vec<i32> = frames.iter().map(|f| f[1]).collect();
        let peak = speeds.iter().copied().max().unwrap();
        assert!(speeds[0] * 3 < peak, "first frame {} vs peak {peak}", speeds[0]);
    }

    #[test]
    fn a_new_notch_mid_glide_does_not_jerk_the_speed() {
        let cfg = cfg();
        let t0 = Instant::now();
        let mut anim = ScrollAnimator::default();
        anim.add([0, 1], t0, &cfg);
        let mut t = t0;
        let mut last = 0;
        for _ in 0..12 {
            t += FRAME;
            last = anim.frame(t, &cfg).map_or(0, |f| f[1]);
        }
        anim.add([0, 1], t, &cfg);
        t += FRAME;
        let next = anim.frame(t, &cfg).map_or(0, |f| f[1]);
        assert!((next - last).abs() <= 2, "speed jumped from {last} to {next} px/frame");
    }

    #[test]
    fn fractional_speed_never_stalls_or_overshoots_the_total() {
        let mut cfg = cfg();
        cfg.step_px = 7.0;
        cfg.duration_ms = 800.0;
        let mut anim = ScrollAnimator::default();
        let t0 = Instant::now();
        anim.add([0, 1], t0, &cfg);
        let (frames, _) = drain(&mut anim, t0, &cfg);
        assert_eq!(total(&frames), [0, 7]);
        assert!(frames.iter().all(|f| f[1] >= 0), "never scrolls backwards: {frames:?}");
    }

    #[test]
    fn fast_notches_accelerate_slow_ones_do_not() {
        let cfg = cfg();
        let t0 = Instant::now();

        let mut slow = ScrollAnimator::default();
        for i in 0..5 {
            slow.add([0, 1], t0 + Duration::from_millis(200 * i), &cfg);
        }
        let mut fast = ScrollAnimator::default();
        for i in 0..5 {
            fast.add([0, 1], t0 + Duration::from_millis(30 * i), &cfg);
        }
        let slow_total = total(&drain(&mut slow, t0 + Duration::from_secs(1), &cfg).0)[1];
        let fast_total = total(&drain(&mut fast, t0 + Duration::from_secs(1), &cfg).0)[1];
        assert_eq!(slow_total, 300);
        // 60 × (1 + 1.3 + 1.6 + 1.9 + 2.2)
        assert_eq!(fast_total, 480);
    }

    #[test]
    fn acceleration_is_capped() {
        let cfg = cfg();
        let t0 = Instant::now();
        let mut anim = ScrollAnimator::default();
        for i in 0..40 {
            anim.add([0, 1], t0 + Duration::from_millis(10 * i), &cfg);
        }
        let px = total(&drain(&mut anim, t0 + Duration::from_secs(1), &cfg).0)[1];
        assert!(px <= 40 * 60 * 5, "{px}");
    }

    #[test]
    fn reversing_drops_queued_distance() {
        let cfg = cfg();
        let t0 = Instant::now();
        let mut anim = ScrollAnimator::default();
        anim.add([0, 3], t0, &cfg);
        anim.add([0, -1], t0 + Duration::from_millis(10), &cfg);
        let (frames, _) = drain(&mut anim, t0 + Duration::from_millis(10), &cfg);
        assert_eq!(total(&frames), [0, -60]);
        assert!(frames.iter().all(|f| f[1] <= 0));
    }

    #[test]
    fn horizontal_notches_scroll_horizontally() {
        let cfg = cfg();
        let t0 = Instant::now();
        let mut anim = ScrollAnimator::default();
        anim.add([2, 0], t0, &cfg);
        assert_eq!(total(&drain(&mut anim, t0, &cfg).0), [120, 0]);
    }
}
