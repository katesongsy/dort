//! UI liveness animation. Service outcomes stay untouched.
use super::glyphs;
use std::time::Duration;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Frame {
    #[default]
    Hidden,
    Static,
    Animated(usize),
}
#[derive(Default)]
pub struct Indicator {
    started: Option<Duration>,
    frame: Frame,
}
impl Indicator {
    pub fn reset(&mut self) {
        self.started = None;
        self.frame = Frame::Hidden;
    }
    pub fn update(&mut self, now: Duration, visible: bool, reduced: bool, ascii: bool) -> bool {
        let old = self.frame;
        if !visible {
            self.reset();
        } else {
            let start = *self.started.get_or_insert(now);
            let elapsed = now.saturating_sub(start);
            self.frame = if reduced {
                Frame::Static
            } else if elapsed < Duration::from_millis(150) {
                Frame::Hidden
            } else {
                let cadence = if ascii { 125 } else { 100 };
                let count = if ascii {
                    glyphs::ASCII_SPINNER.len()
                } else {
                    glyphs::SPINNER.len()
                };
                Frame::Animated(((elapsed.as_millis() - 150) / cadence % count as u128) as usize)
            };
        }
        old != self.frame
    }
    pub fn symbol(&self, ascii: bool) -> &'static str {
        match self.frame {
            Frame::Hidden => " ",
            Frame::Static => {
                if ascii {
                    "."
                } else {
                    glyphs::PENDING
                }
            }
            Frame::Animated(index) => {
                if ascii {
                    glyphs::ASCII_SPINNER[index % 4]
                } else {
                    glyphs::SPINNER[index % 10]
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn liveness_is_delayed_bounded_and_stops_when_hidden() {
        let mut indicator = Indicator::default();
        let ms = Duration::from_millis;
        assert!(!indicator.update(ms(0), true, false, false));
        assert!(!indicator.update(ms(149), true, false, false));
        assert!(indicator.update(ms(150), true, false, false));
        assert_eq!(indicator.symbol(false), glyphs::SPINNER[0]);
        assert!(!indicator.update(ms(249), true, false, false));
        assert!(indicator.update(ms(250), true, false, false));
        assert_eq!(indicator.symbol(false), glyphs::SPINNER[1]);
        assert!(indicator.update(ms(300), false, false, false));
        assert!(!indicator.update(ms(9999), false, false, false));
        assert_eq!(indicator.symbol(false), " ");
        assert!(!indicator.update(ms(10000), true, false, false));
    }
    #[test]
    fn reduced_motion_and_ascii_are_independent_of_color() {
        let mut indicator = Indicator::default();
        assert!(indicator.update(Duration::ZERO, true, true, true));
        assert_eq!(indicator.symbol(true), ".");
        assert!(!indicator.update(Duration::from_secs(30), true, true, true));
        indicator.reset();
        indicator.update(Duration::ZERO, true, false, true);
        indicator.update(Duration::from_millis(275), true, false, true);
        assert_eq!(indicator.symbol(true), "/");
    }
}
