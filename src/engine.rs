//! Platform-independent event logic: input events in, a verdict on the original
//! event plus synthetic effects out. The macOS layer only translates.

use std::time::Instant;

use crate::config::{Action, ButtonBinding, Config, DragMode};
use crate::keys::{Shortcut, FLAG_SHIFT};
use crate::scroll::ScrollAnimator;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    /// `button` is the CoreGraphics number (2 = middle).
    ButtonDown { button: u32 },
    ButtonUp { button: u32 },
    Dragged { button: u32, dx: f64, dy: f64 },
    /// `lines` = `[horizontal, vertical]` line deltas of a wheel event.
    Wheel { lines: [i64; 2], continuous: bool, flags: u64 },
}

/// What happens to the original event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Swallow,
    /// Pass with all scroll deltas negated.
    InvertScroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollPhase {
    /// Plain continuous scroll, outside any gesture (smoothed horizontal wheel).
    None,
    Began,
    Changed,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    Key(Shortcut),
    /// Re-post a click of this CoreGraphics button at the cursor.
    Click { button: u32 },
    /// Pixel scroll; positive = content moves right/down.
    Scroll { px: [i32; 2], phase: ScrollPhase, flags: u64 },
    /// Pin the cursor in place (and stop it moving) while dragging.
    FreezeCursor(bool),
}

#[derive(Debug, PartialEq)]
pub struct Outcome {
    pub verdict: Verdict,
    pub effects: Vec<Effect>,
}

impl Outcome {
    fn pass() -> Self {
        Outcome { verdict: Verdict::Pass, effects: Vec::new() }
    }
    fn swallow(effects: Vec<Effect>) -> Self {
        Outcome { verdict: Verdict::Swallow, effects }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PressState {
    /// Not moved past the click slop yet.
    Pending,
    /// Moving, no gesture fired yet / drag-scrolling.
    Dragging,
    /// Gesture fired; ignore the rest of this drag.
    Fired,
}

#[derive(Debug)]
struct Press {
    button: u32,
    moved: [f64; 2],
    state: PressState,
}

pub struct Engine {
    cfg: Config,
    press: Option<Press>,
    anim: ScrollAnimator,
    /// Modifier flags of the latest wheel event, reused for animated frames.
    wheel_flags: u64,
    /// A vertical wheel glide is open as a trackpad gesture; apps are waiting for its end.
    gesture_open: bool,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Engine { cfg, press: None, anim: ScrollAnimator::default(), wheel_flags: 0, gesture_open: false }
    }

    pub fn is_animating(&self) -> bool {
        self.anim.is_active() || self.gesture_open
    }

    pub fn handle(&mut self, input: Input, now: Instant) -> Outcome {
        match input {
            Input::ButtonDown { button } => self.button_down(button),
            Input::Dragged { button, dx, dy } => self.dragged(button, dx, dy),
            Input::ButtonUp { button } => self.button_up(button),
            Input::Wheel { lines, continuous, flags } => self.wheel(lines, continuous, flags, now),
        }
    }

    /// Next animation frame of smooth scrolling. A vertical glide is posted as one trackpad
    /// gesture (began, changed, ended) so apps treat it like two fingers; a horizontal one
    /// stays unphased, because a phased horizontal scroll can trigger swipe navigation.
    pub fn frame(&mut self, now: Instant) -> Vec<Effect> {
        let px = self.anim.frame(now, &self.cfg.scroll).unwrap_or([0, 0]);
        let flags = self.wheel_flags;
        let mut effects = Vec::new();
        if px != [0, 0] {
            let phase = if self.gesture_open {
                ScrollPhase::Changed
            } else if px[1] != 0 {
                self.gesture_open = true;
                ScrollPhase::Began
            } else {
                ScrollPhase::None
            };
            effects.push(Effect::Scroll { px, phase, flags });
        }
        if self.gesture_open && !self.anim.vertical_active() {
            effects.extend(self.end_gesture());
        }
        effects
    }

    /// Stops any glide and returns the effect that ends an open gesture, so that a config
    /// reload or a pause never leaves an app waiting for the end of a scroll.
    pub fn stop_scrolling(&mut self) -> Vec<Effect> {
        self.anim = ScrollAnimator::default();
        self.end_gesture().into_iter().collect()
    }

    fn end_gesture(&mut self) -> Option<Effect> {
        std::mem::take(&mut self.gesture_open)
            .then_some(Effect::Scroll { px: [0, 0], phase: ScrollPhase::Ended, flags: self.wheel_flags })
    }

    fn binding(&self, button: u32) -> Option<&ButtonBinding> {
        self.cfg.buttons.get(&button)
    }

    fn button_down(&mut self, button: u32) -> Outcome {
        if self.press.is_some() || self.binding(button).is_none() {
            return Outcome::pass();
        }
        self.press = Some(Press { button, moved: [0.0, 0.0], state: PressState::Pending });
        Outcome::swallow(Vec::new())
    }

    fn dragged(&mut self, button: u32, dx: f64, dy: f64) -> Outcome {
        let Some(press) = self.press.as_mut().filter(|p| p.button == button) else {
            return Outcome::pass();
        };
        let binding = self.cfg.buttons.get(&button).expect("press only exists for bound buttons");
        let drag = &self.cfg.drag;
        press.moved[0] += dx;
        press.moved[1] += dy;
        let mut effects = Vec::new();

        if press.state == PressState::Pending
            && !matches!(binding.drag, DragMode::None)
            && press.moved[0].hypot(press.moved[1]) > drag.click_slop_px
        {
            press.state = PressState::Dragging;
            effects.push(Effect::FreezeCursor(true));
            if binding.drag == DragMode::Scroll {
                effects.push(Effect::Scroll { px: [0, 0], phase: ScrollPhase::Began, flags: 0 });
            }
        }

        if press.state == PressState::Dragging {
            match &binding.drag {
                DragMode::Scroll => {
                    let sign = if drag.scroll_invert { -1.0 } else { 1.0 };
                    let px = [
                        (dx * drag.scroll_speed * sign).round() as i32,
                        (dy * drag.scroll_speed * sign).round() as i32,
                    ];
                    if px != [0, 0] {
                        effects.push(Effect::Scroll { px, phase: ScrollPhase::Changed, flags: 0 });
                    }
                }
                DragMode::Gestures(g) => {
                    let [x, y] = press.moved;
                    let t = drag.gesture_threshold_px;
                    let action = if x.abs() >= y.abs() && x.abs() >= t {
                        Some(if x > 0.0 { &g.right } else { &g.left })
                    } else if y.abs() > x.abs() && y.abs() >= t {
                        // Screen y grows downwards.
                        Some(if y > 0.0 { &g.down } else { &g.up })
                    } else {
                        None
                    };
                    if let Some(action) = action {
                        press.state = PressState::Fired;
                        effects.extend(action_effects(action, button));
                    }
                }
                DragMode::None => unreachable!("None never enters Dragging"),
            }
        }
        Outcome::swallow(effects)
    }

    fn button_up(&mut self, button: u32) -> Outcome {
        let Some(press) = self.press.take_if(|p| p.button == button) else {
            return Outcome::pass();
        };
        let binding = self.cfg.buttons.get(&button).expect("press only exists for bound buttons");
        let effects = match press.state {
            PressState::Pending => action_effects(&binding.click, button),
            PressState::Dragging | PressState::Fired => {
                let mut fx = Vec::new();
                if binding.drag == DragMode::Scroll {
                    fx.push(Effect::Scroll { px: [0, 0], phase: ScrollPhase::Ended, flags: 0 });
                }
                fx.push(Effect::FreezeCursor(false));
                fx
            }
        };
        Outcome::swallow(effects)
    }

    fn wheel(&mut self, mut lines: [i64; 2], continuous: bool, flags: u64, now: Instant) -> Outcome {
        let s = &self.cfg.scroll;
        // Trackpads and Magic Mouse already scroll smoothly; leave them alone.
        if continuous {
            return Outcome::pass();
        }
        if !s.smooth {
            return Outcome {
                verdict: if s.invert { Verdict::InvertScroll } else { Verdict::Pass },
                effects: Vec::new(),
            };
        }
        if s.invert {
            lines = [-lines[0], -lines[1]];
        }
        // Shift+wheel scrolls horizontally on notched wheels. AppKit does this for
        // line events only, so convert here and drop shift from the pixel events.
        let mut out_flags = flags;
        if flags & FLAG_SHIFT != 0 && lines[0] == 0 {
            lines = [lines[1], 0];
            out_flags &= !FLAG_SHIFT;
        }
        if lines == [0, 0] {
            return Outcome::pass();
        }
        self.wheel_flags = out_flags;
        self.anim.add(lines, now, s);
        Outcome::swallow(Vec::new())
    }
}

fn action_effects(action: &Action, button: u32) -> Vec<Effect> {
    match action {
        Action::None => Vec::new(),
        Action::Passthrough => vec![Effect::Click { button }],
        Action::Shortcut(s) => vec![Effect::Key(*s)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::FLAG_COMMAND;
    use crate::scroll::FRAME;

    const MIDDLE: u32 = 2;
    const BACK: u32 = 3;

    fn engine() -> Engine {
        Engine::new(Config::default())
    }

    fn run(e: &mut Engine, inputs: &[Input]) -> Vec<Outcome> {
        let now = Instant::now();
        inputs.iter().map(|i| e.handle(*i, now)).collect()
    }

    fn effects(outcomes: &[Outcome]) -> Vec<Effect> {
        outcomes.iter().flat_map(|o| o.effects.clone()).collect()
    }

    fn key(name: &str) -> Effect {
        Effect::Key(Shortcut::parse(name).unwrap())
    }

    #[test]
    fn unbound_buttons_pass_through_untouched() {
        let mut e = engine();
        let out = run(
            &mut e,
            &[
                Input::ButtonDown { button: 7 },
                Input::Dragged { button: 7, dx: 100.0, dy: 0.0 },
                Input::ButtonUp { button: 7 },
            ],
        );
        assert!(out.iter().all(|o| *o == Outcome::pass()));
    }

    #[test]
    fn click_fires_click_action_on_release() {
        let mut e = engine();
        let out = run(&mut e, &[Input::ButtonDown { button: BACK }, Input::ButtonUp { button: BACK }]);
        assert!(out.iter().all(|o| o.verdict == Verdict::Swallow));
        assert_eq!(effects(&out), vec![key("cmd+[")]);
    }

    #[test]
    fn jitter_below_slop_is_still_a_click() {
        let mut e = engine();
        let out = run(
            &mut e,
            &[
                Input::ButtonDown { button: BACK },
                Input::Dragged { button: BACK, dx: 2.0, dy: -2.0 },
                Input::ButtonUp { button: BACK },
            ],
        );
        assert_eq!(effects(&out), vec![key("cmd+[")]);
    }

    #[test]
    fn gesture_fires_once_by_dominant_axis_and_skips_click() {
        let mut e = engine();
        let mut inputs = vec![Input::ButtonDown { button: BACK }];
        inputs.extend((0..10).map(|_| Input::Dragged { button: BACK, dx: 3.0, dy: -10.0 }));
        inputs.push(Input::ButtonUp { button: BACK });
        let out = run(&mut e, &inputs);
        assert_eq!(
            effects(&out),
            vec![Effect::FreezeCursor(true), key("ctrl+up"), Effect::FreezeCursor(false)]
        );
    }

    #[test]
    fn short_drag_without_gesture_does_nothing() {
        let mut e = engine();
        let out = run(
            &mut e,
            &[
                Input::ButtonDown { button: BACK },
                Input::Dragged { button: BACK, dx: -20.0, dy: 0.0 },
                Input::ButtonUp { button: BACK },
            ],
        );
        assert_eq!(effects(&out), vec![Effect::FreezeCursor(true), Effect::FreezeCursor(false)]);
    }

    #[test]
    fn drag_scroll_emits_phased_pixels_following_the_hand() {
        let mut e = engine();
        let out = run(
            &mut e,
            &[
                Input::ButtonDown { button: MIDDLE },
                Input::Dragged { button: MIDDLE, dx: 0.0, dy: 10.0 },
                Input::Dragged { button: MIDDLE, dx: -3.0, dy: 0.0 },
                Input::ButtonUp { button: MIDDLE },
            ],
        );
        assert_eq!(
            effects(&out),
            vec![
                Effect::FreezeCursor(true),
                Effect::Scroll { px: [0, 0], phase: ScrollPhase::Began, flags: 0 },
                Effect::Scroll { px: [0, 10], phase: ScrollPhase::Changed, flags: 0 },
                Effect::Scroll { px: [-3, 0], phase: ScrollPhase::Changed, flags: 0 },
                Effect::Scroll { px: [0, 0], phase: ScrollPhase::Ended, flags: 0 },
                Effect::FreezeCursor(false),
            ]
        );
    }

    #[test]
    fn middle_click_without_drag_is_reposted() {
        let mut e = engine();
        let out = run(&mut e, &[Input::ButtonDown { button: MIDDLE }, Input::ButtonUp { button: MIDDLE }]);
        assert_eq!(effects(&out), vec![Effect::Click { button: MIDDLE }]);
    }

    #[test]
    fn second_button_during_press_passes_through() {
        let mut e = engine();
        let out = run(
            &mut e,
            &[
                Input::ButtonDown { button: BACK },
                Input::ButtonDown { button: MIDDLE },
                Input::ButtonUp { button: MIDDLE },
                Input::ButtonUp { button: BACK },
            ],
        );
        assert_eq!(out[1], Outcome::pass());
        assert_eq!(out[2], Outcome::pass());
        assert_eq!(out[3].effects, vec![key("cmd+[")]);
    }

    fn wheel(lines: [i64; 2], flags: u64) -> Input {
        Input::Wheel { lines, continuous: false, flags }
    }

    fn animate(e: &mut Engine, start: Instant) -> Vec<Effect> {
        let mut t = start;
        let mut out = Vec::new();
        while e.is_animating() {
            t += FRAME;
            out.extend(e.frame(t));
        }
        out
    }

    fn total_px(fx: &[Effect]) -> [i32; 2] {
        fx.iter().fold([0, 0], |a, f| match f {
            Effect::Scroll { px, .. } => [a[0] + px[0], a[1] + px[1]],
            _ => a,
        })
    }

    #[test]
    fn trackpad_scroll_is_never_touched() {
        let mut cfg = Config::default();
        cfg.scroll.invert = true;
        let mut e = Engine::new(cfg);
        let out = e.handle(Input::Wheel { lines: [0, 3], continuous: true, flags: 0 }, Instant::now());
        assert_eq!(out, Outcome::pass());
    }

    #[test]
    fn smooth_wheel_is_swallowed_and_animated_with_modifiers_kept() {
        let mut e = engine();
        let t = Instant::now();
        assert_eq!(e.handle(wheel([0, -1], FLAG_COMMAND), t).verdict, Verdict::Swallow);
        let fx = animate(&mut e, t);
        assert!(fx.len() > 5, "should take several frames");
        assert_eq!(total_px(&fx), [0, -60]);
        assert!(fx.iter().all(|f| matches!(f, Effect::Scroll { flags: FLAG_COMMAND, .. })));
    }

    fn phases(fx: &[Effect]) -> Vec<ScrollPhase> {
        fx.iter()
            .filter_map(|f| match f {
                Effect::Scroll { phase, .. } => Some(*phase),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn vertical_wheel_glide_is_one_trackpad_gesture() {
        let mut e = engine();
        let t = Instant::now();
        e.handle(wheel([0, -1], 0), t);
        let phases = phases(&animate(&mut e, t));
        assert_eq!(phases.first(), Some(&ScrollPhase::Began));
        assert_eq!(phases.last(), Some(&ScrollPhase::Ended));
        assert!(phases[1..phases.len() - 1].iter().all(|p| *p == ScrollPhase::Changed));
        assert_eq!(phases.iter().filter(|p| **p == ScrollPhase::Began).count(), 1);
    }

    #[test]
    fn the_gesture_stays_open_while_notches_keep_coming() {
        let mut e = engine();
        let t = Instant::now();
        e.handle(wheel([0, -1], 0), t);
        let mut fx = Vec::new();
        for i in 1..=10 {
            fx.extend(e.frame(t + FRAME * i));
        }
        e.handle(wheel([0, -1], 0), t + FRAME * 10);
        fx.extend(animate(&mut e, t + FRAME * 10));
        let phases = phases(&fx);
        assert_eq!(phases.iter().filter(|p| **p == ScrollPhase::Began).count(), 1);
        assert_eq!(phases.iter().filter(|p| **p == ScrollPhase::Ended).count(), 1);
    }

    #[test]
    fn stopping_mid_glide_ends_the_open_gesture() {
        let mut e = engine();
        let t = Instant::now();
        e.handle(wheel([0, -1], 0), t);
        let mut fx = Vec::new();
        for i in 1..=10 {
            fx.extend(e.frame(t + FRAME * i));
        }
        fx.extend(e.stop_scrolling());
        assert_eq!(phases(&fx).last(), Some(&ScrollPhase::Ended));
        assert!(!e.is_animating());
        assert!(e.stop_scrolling().is_empty(), "the gesture ends only once");
    }

    #[test]
    fn horizontal_wheel_glide_stays_unphased() {
        let mut e = engine();
        let t = Instant::now();
        e.handle(wheel([0, -1], FLAG_SHIFT), t);
        assert!(phases(&animate(&mut e, t)).iter().all(|p| *p == ScrollPhase::None));
    }

    #[test]
    fn invert_flips_smooth_and_plain_scroll() {
        let mut cfg = Config::default();
        cfg.scroll.invert = true;
        let mut e = Engine::new(cfg.clone());
        let t = Instant::now();
        e.handle(wheel([0, 1], 0), t);
        assert_eq!(total_px(&animate(&mut e, t)), [0, -60]);

        cfg.scroll.smooth = false;
        let mut e = Engine::new(cfg);
        assert_eq!(e.handle(wheel([0, 1], 0), t).verdict, Verdict::InvertScroll);
    }

    #[test]
    fn plain_scroll_passes_when_smoothing_and_invert_are_off() {
        let mut cfg = Config::default();
        cfg.scroll.smooth = false;
        let mut e = Engine::new(cfg);
        assert_eq!(e.handle(wheel([0, 1], 0), Instant::now()), Outcome::pass());
    }

    #[test]
    fn shift_wheel_becomes_horizontal_without_shift() {
        let mut e = engine();
        let t = Instant::now();
        e.handle(wheel([0, -1], FLAG_SHIFT | FLAG_COMMAND), t);
        let fx = animate(&mut e, t);
        assert_eq!(total_px(&fx), [-60, 0]);
        assert!(fx.iter().all(|f| matches!(f, Effect::Scroll { flags: FLAG_COMMAND, .. })));
    }
}
