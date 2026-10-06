//! Developer-only smoke check: warm output, play three cues, then release it.
use frame_voice::{audio, config::Config, feedback};
use std::{thread, time::Duration};
fn main() -> anyhow::Result<()> {
    let c = Config::load()?;
    let lease = feedback::warm(&c);
    thread::sleep(Duration::from_millis(500));
    for hz in [880., 660., 320.] {
        audio::beep(&c, hz);
        thread::sleep(Duration::from_millis(300));
    }
    drop(lease);
    thread::sleep(Duration::from_millis(400));
    Ok(())
}
