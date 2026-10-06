//! Monotonic milliseconds, with a release baseline on every context acquisition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A tap was accepted: prearm the microphone so capture is warm if the hold follows.
    Arm,
    /// The second touch was accepted: begin visual feedback while capture warms.
    Hold,
    /// The hold lasted long enough: this is a real recording.
    Start,
    /// Released after recording began.
    Stop,
    /// The gesture was abandoned before recording began; cancel any prearmed capture.
    Abort,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Idle,
    Tap(u64),
    Ready(u64),
    Hold(u64),
    Recording,
}
pub struct Gesture {
    state: State,
    raw: bool,
    stable: bool,
    changed: u64,
    baseline: bool,
    pub tap_ms: u64,
    pub ready_ms: u64,
    pub arm_ms: u64,
}
impl Default for Gesture {
    fn default() -> Self {
        Self {
            state: State::Idle,
            raw: false,
            stable: false,
            changed: 0,
            baseline: true,
            tap_ms: 250,
            ready_ms: 600,
            arm_ms: 150,
        }
    }
}
impl Gesture {
    pub fn reset(&mut self) {
        self.state = State::Idle;
        self.baseline = true;
    }
    /// Pending edges and gesture deadlines require the fast polling cadence.
    pub fn engaged(&self) -> bool {
        self.raw || self.stable || !matches!(self.state, State::Idle)
    }
    pub fn update(&mut self, now: u64, raw: bool) -> Option<Event> {
        if self.baseline {
            self.raw = raw;
            self.stable = raw;
            self.changed = now;
            // A held cap on entering context must first be released.
            if !raw {
                self.baseline = false;
            }
            return None;
        }
        if raw != self.raw {
            self.raw = raw;
            self.changed = now;
        }
        if now.saturating_sub(self.changed) < 30 {
            return None;
        }
        let rising = raw && !self.stable;
        let falling = !raw && self.stable;
        self.stable = raw;
        if let State::Ready(t) = self.state {
            if now - t > self.ready_ms {
                self.state = State::Idle;
                return Some(Event::Abort);
            }
        }
        if rising {
            if matches!(self.state, State::Ready(_)) {
                self.state = State::Hold(now);
                return Some(Event::Hold);
            }
            self.state = State::Tap(now);
        } else if falling {
            let previous = self.state;
            self.state = match previous {
                State::Tap(t) if now - t <= self.tap_ms => State::Ready(now),
                _ => State::Idle,
            };
            match previous {
                State::Recording => return Some(Event::Stop),
                State::Hold(_) => return Some(Event::Abort),
                // Only a genuine (short) tap prearms; a long press falls through to Idle.
                State::Tap(t) if now - t <= self.tap_ms => return Some(Event::Arm),
                _ => {}
            }
        } else if raw {
            match self.state {
                State::Tap(t) if now - t > self.tap_ms => self.state = State::Idle,
                State::Hold(t) if now - t >= self.arm_ms => {
                    self.state = State::Recording;
                    return Some(Event::Start);
                }
                _ => {}
            }
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn edge(g: &mut Gesture, t: u64, value: bool) -> Option<Event> {
        g.update(t, value);
        g.update(t + 30, value)
    }
    #[test]
    fn tap_hold_release() {
        let mut g = Gesture::default();
        g.update(0, false);
        edge(&mut g, 100, true);
        edge(&mut g, 200, false);
        edge(&mut g, 300, true);
        assert_eq!(g.update(479, true), None);
        assert_eq!(g.update(480, true), Some(Event::Start));
        assert_eq!(edge(&mut g, 500, false), Some(Event::Stop));
    }
    #[test]
    fn plain_hold_and_stale_touch_do_nothing() {
        let mut g = Gesture::default();
        g.update(0, true);
        g.update(1000, true);
        edge(&mut g, 1100, false);
        edge(&mut g, 1200, true);
        assert_eq!(g.update(3000, true), None);
        edge(&mut g, 3100, false);
        edge(&mut g, 3200, true);
        assert_eq!(g.update(4000, true), None);
    }
    #[test]
    fn expired_tap_short_hold_and_glitch() {
        let mut g = Gesture::default();
        g.update(0, false);
        g.update(10, true);
        g.update(20, false);
        assert_eq!(g.update(1000, false), None);
        edge(&mut g, 1100, true);
        edge(&mut g, 1200, false);
        edge(&mut g, 1900, true);
        assert_eq!(g.update(2200, true), None);
        edge(&mut g, 2300, false);
        edge(&mut g, 2400, true);
        edge(&mut g, 2500, false);
        edge(&mut g, 2600, true);
        // A short re-press that never arms prearm capture, then aborts on release.
        assert_eq!(edge(&mut g, 2650, false), Some(Event::Abort));
        assert_eq!(g.update(3000, false), None);
    }
    #[test]
    fn tap_prearms_and_hold_commits() {
        let mut g = Gesture::default();
        g.update(0, false);
        assert_eq!(edge(&mut g, 100, true), None);
        // Accepting the tap prearms the microphone.
        assert_eq!(edge(&mut g, 200, false), Some(Event::Arm));
        // Pressing again while Ready starts the hold, but does not commit yet.
        assert_eq!(edge(&mut g, 300, true), Some(Event::Hold));
        assert_eq!(g.update(479, true), None);
        assert_eq!(g.update(480, true), Some(Event::Start));
        assert_eq!(edge(&mut g, 500, false), Some(Event::Stop));
    }
    #[test]
    fn abandoned_gesture_aborts_capture() {
        // Ready window expires without a hold.
        let mut g = Gesture::default();
        g.update(0, false);
        edge(&mut g, 100, true);
        assert_eq!(edge(&mut g, 200, false), Some(Event::Arm));
        assert_eq!(g.update(900, false), Some(Event::Abort));
        // Released from Hold before the arm threshold.
        let mut g = Gesture::default();
        g.update(0, false);
        edge(&mut g, 100, true);
        assert_eq!(edge(&mut g, 200, false), Some(Event::Arm));
        edge(&mut g, 300, true);
        assert_eq!(g.update(360, false), None);
        assert_eq!(g.update(400, false), Some(Event::Abort));
    }
}
