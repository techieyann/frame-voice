//! Poll fast during gestures, less often while waiting for the next gesture.
use std::time::Duration;

pub const BADGE_INTERVAL: Duration = Duration::from_millis(8);
pub fn input_interval(engaged: bool, available: bool) -> Duration {
    Duration::from_millis(if !available {
        100
    } else if engaged {
        4
    } else {
        16
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fast_gestures_and_quiet_idle() {
        assert_eq!(input_interval(true, true), Duration::from_millis(4));
        assert_eq!(input_interval(false, true), Duration::from_millis(16));
        assert_eq!(input_interval(true, false), Duration::from_millis(100));
    }
}
