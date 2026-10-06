use crate::config::Config;
use anyhow::{bail, Context, Result};
use std::{
    io::{Read, Write},
    os::fd::AsRawFd,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub const RATE: u32 = 16000;
pub const CHUNK: usize = 800; // 50 ms
/// Cue length and peak. Long enough to read as a deliberate tone rather than a
/// clipped 60 ms blip; the 5 ms fades keep the edges click-free.
const TONE_SAMPLES: usize = 2880; // 180 ms
const TONE_FADE: f32 = 80.;
const TONE_PEAK: f32 = 9000.;
#[derive(Clone, Default)]
pub struct Control {
    pub stop: Arc<AtomicBool>,
    pub cancel: Arc<AtomicBool>,
    /// Set once the capture backend has delivered its first PCM, i.e. the mic is live.
    pub live: Arc<AtomicBool>,
    /// Set once the captured audio is confirmed to contain speech, so the stop
    /// tone and the processing badge are emitted together.
    pub voiced: Arc<AtomicBool>,
}
impl Control {
    pub fn canceled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}
pub use crate::process::Process;
pub fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.;
    }
    (samples.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
}
pub const CHUNK_MS: u64 = 50;
fn speech_levels(samples: &[i16]) -> (f64, f64) {
    if samples.is_empty() {
        return (0., 0.);
    }
    let n = samples.len() as f64;
    let mean = samples.iter().map(|&v| v as f64).sum::<f64>() / n;
    let energy = samples
        .iter()
        .map(|&v| (v as f64 - mean).powi(2))
        .sum::<f64>();
    if energy <= f64::EPSILON {
        return (0., 0.);
    }
    let level = (energy / n).sqrt();
    // Feedback leaking from the speakers is tonal, not speech. Goertzel
    // measures concentration at our three cue frequencies without an FFT.
    for hz in [880., 660., 320.] {
        let coefficient = 2. * (2. * std::f64::consts::PI * hz / RATE as f64).cos();
        let (mut previous, mut before) = (0., 0.);
        for &sample in samples {
            let next = sample as f64 - mean + coefficient * previous - before;
            before = previous;
            previous = next;
        }
        let power = previous * previous + before * before - coefficient * previous * before;
        if 2. * power / (n * energy) >= 0.65 {
            return (level, 0.);
        }
    }
    (level, level)
}
/// Per-chunk energy evidence after ambient-floor and cue-tone rejection.
pub(crate) fn speech_candidates(samples: &[i16], threshold: f64) -> Vec<bool> {
    let (mut quiet, chunks): (Vec<f64>, Vec<f64>) =
        samples.chunks(CHUNK).map(speech_levels).unzip();
    if chunks.is_empty() {
        return Vec::new();
    }
    // Include noise above the absolute threshold when estimating the ambient floor.
    let index = (quiet.len() / 10).min(quiet.len() - 1);
    let (_, floor, _) = quiet.select_nth_unstable_by(index, f64::total_cmp);
    let gate = threshold.max(*floor + (threshold * 0.5).max(*floor * 0.5));
    chunks.into_iter().map(|level| level >= gate).collect()
}
/// Decide whether a recording contains speech worth uploading, and return the
/// trimmed span. Two cheap guards keep silent/noisy releases off the network:
/// an adaptive ambient floor (the quietest ~10% of 50 ms chunks) and a minimum
/// contiguous-speech duration (`VOICE_MIN_SPEECH_MS`).
pub fn trim(samples: &[i16], threshold: f64, min_speech_ms: u64) -> Option<&[i16]> {
    let chunks = speech_candidates(samples, threshold);
    let min_chunks = min_speech_ms.div_ceil(CHUNK_MS).max(2) as usize;
    // Require min_chunks contiguous chunks above the gate; reject clicks/breaths.
    let mut run = 0usize;
    let mut voiced = false;
    for &candidate in &chunks {
        run = if candidate { run + 1 } else { 0 };
        if run >= min_chunks {
            voiced = true;
            break;
        }
    }
    if !voiced {
        return None;
    }
    let first = chunks
        .iter()
        .position(|&candidate| candidate)?
        .saturating_sub(6)
        * CHUNK;
    let last = ((chunks.iter().rposition(|&candidate| candidate)? + 7) * CHUNK).min(samples.len());
    Some(&samples[first..last])
}
pub fn wav(samples: &[i16]) -> Vec<u8> {
    let bytes = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + bytes as usize);
    out.extend(b"RIFF");
    out.extend((36 + bytes).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(RATE.to_le_bytes());
    out.extend((RATE * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(bytes.to_le_bytes());
    for v in samples {
        out.extend(v.to_le_bytes());
    }
    out
}
pub fn record(c: &Config, control: &Control) -> Result<Vec<i16>> {
    if control.canceled() || control.stop.load(Ordering::SeqCst) {
        return Ok(Vec::new());
    }
    let mut child = Process(
        Command::new(c.get("FFMPEG_BIN", "ffmpeg"))
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "pulse",
                "-fragment_size",
                c.get("FFMPEG_FRAGMENT_SIZE", "64"),
                "-i",
                "default",
                "-ar",
                "16000",
                "-ac",
                "1",
                "-f",
                "s16le",
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("start ffmpeg capture")?,
    );
    let mut stdout = child.0.stdout.take().context("capture stdout missing")?;
    let start = Instant::now();
    let mut last_audio = Instant::now();
    let mut bytes = Vec::with_capacity(RATE as usize * 2);
    let max_duration = Duration::from_secs_f64(c.max_seconds);
    let max_bytes = (c.max_seconds * RATE as f64 * 2.) as usize;
    let mut buf = [0u8; CHUNK * 2];
    while start.elapsed() < max_duration
        && !control.stop.load(Ordering::SeqCst)
        && !control.canceled()
    {
        let mut fd = libc::pollfd {
            fd: stdout.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: fd points at one valid pollfd; stdout lives across the call.
        let n = unsafe { libc::poll(&mut fd, 1, 50) };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e.into());
        }
        if n > 0 {
            let n = stdout.read(&mut buf)?;
            if n == 0 {
                bail!("microphone capture ended unexpectedly");
            }
            if bytes.is_empty() {
                // SteamOS can reset ADC gains when audio streams change. Repair
                // them after this stream is open, before emitting the live cue.
                restore_mic_gain(c, control)?;
                // Signal the daemon that capture is live; it plays the start tone
                // only once a committed recording can actually hear the user.
                control.live.store(true, Ordering::SeqCst);
            }
            bytes.extend_from_slice(&buf[..n]);
            last_audio = Instant::now();
            if bytes.len() >= max_bytes {
                break;
            }
        }
        if last_audio.elapsed() > Duration::from_secs(3) {
            bail!("microphone produced no audio for 3 seconds");
        }
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect())
}
fn restore_mic_gain(c: &Config, control: &Control) -> Result<()> {
    let helper = c.home.join(".local/share/frame-voice/mic-gain.sh");
    if !helper.is_file() {
        return Ok(());
    }
    let mut child = Process(
        Command::new(helper)
            .arg("--once")
            .env("MIC_GAIN", c.get("MIC_GAIN", "120"))
            .env("MIC_CARD", c.get("MIC_CARD", "0"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("start microphone gain setup")?,
    );
    let start = Instant::now();
    loop {
        if control.canceled() || control.stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        if let Some(status) = child.0.try_wait()? {
            if !status.success() {
                bail!("microphone gain setup failed");
            }
            return Ok(());
        }
        if start.elapsed() >= Duration::from_secs(2) {
            bail!("microphone gain setup timed out");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
pub fn beep(c: &Config, hz: f32) {
    crate::feedback::beep(c, hz);
}
pub(crate) fn legacy_beep(c: &Config, hz: f32) {
    if !c.beep {
        return;
    }
    let c = c.clone();
    std::thread::spawn(move || {
        let samples: Vec<_> = (0..TONE_SAMPLES)
            .map(|i| {
                let envelope = (i as f32 / TONE_FADE)
                    .min(1.)
                    .min((TONE_SAMPLES - i) as f32 / TONE_FADE);
                ((i as f32 * hz * std::f32::consts::TAU / RATE as f32).sin() * TONE_PEAK * envelope)
                    as i16
            })
            .collect();
        let data: Vec<u8> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
        // pw-play starts in single-digit milliseconds; paplay takes ~350 ms.
        // pw-play needs `--raw` to read PCM from stdin; without it libsndfile
        // rejects "-" and the tone is silently dropped. Verify each candidate
        // actually exits cleanly, otherwise fall through to the next one.
        let candidates: Vec<(String, Vec<&str>)> = match c.values.get("PAPLAY_BIN") {
            Some(bin) => vec![(
                bin.clone(),
                vec!["--raw", "--format=s16le", "--rate=16000", "--channels=1"],
            )],
            None => vec![
                (
                    "pw-play".into(),
                    vec![
                        "--raw",
                        "--format=s16",
                        "--rate=16000",
                        "--channels=1",
                        "--latency=20ms",
                        "-",
                    ],
                ),
                (
                    "paplay".into(),
                    vec!["--raw", "--format=s16le", "--rate=16000", "--channels=1"],
                ),
            ],
        };
        for (bin, args) in candidates {
            if play(&bin, &args, &data) {
                return;
            }
        }
    });
}
/// Play raw PCM through `bin`, returning `true` only if it exits successfully.
/// A binary that spawns but fails (bad args, no sink) must not count as a beep.
fn play(bin: &str, args: &[&str], data: &[u8]) -> bool {
    let Ok(child) = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let mut p = Process(child);
    if let Some(mut input) = p.0.stdin.take() {
        let _ = input.write_all(data);
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        match p.0.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return false,
        }
    }
    // Still running after 2 s: assume it is audibly playing; drop kills it.
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_click_trim_and_wave() {
        assert!(trim(&vec![0; CHUNK * 20], 300., 100).is_none());
        let mut pcm = vec![0; CHUNK * 30];
        for (i, sample) in pcm[CHUNK * 12..CHUNK * 13].iter_mut().enumerate() {
            *sample = if i % 2 == 0 { 1000 } else { -1000 };
        }
        assert!(trim(&pcm, 300., 100).is_none());
        for (i, sample) in pcm[CHUNK * 13..CHUNK * 14].iter_mut().enumerate() {
            *sample = if i % 2 == 0 { 1000 } else { -1000 };
        }
        let cut = trim(&pcm, 300., 100).unwrap();
        assert_eq!(cut.len(), CHUNK * 14);
        let out = wav(cut);
        assert_eq!(&out[..4], b"RIFF");
        assert_eq!(out.len(), 44 + cut.len() * 2);
    }
    #[test]
    fn adaptive_floor_rejects_noise_but_keeps_speech() {
        // Steady room noise alone is not speech, even under the raw threshold.
        assert!(trim(&vec![200; CHUNK * 20], 300., 100).is_none());
        // Speech over the same floor still passes.
        let mut pcm = vec![200; CHUNK * 20];
        for (i, sample) in pcm[CHUNK * 8..CHUNK * 12].iter_mut().enumerate() {
            *sample = if i % 2 == 0 { 1500 } else { -1500 };
        }
        assert!(trim(&pcm, 300., 100).is_some());
    }
    #[test]
    fn dc_offset_loud_steady_noise_and_feedback_are_not_speech() {
        assert!(trim(&vec![1000; CHUNK * 20], 300., 200).is_none());
        let noise: Vec<_> = (0..CHUNK * 20)
            .map(|i| if i % 2 == 0 { 800 } else { -800 })
            .collect();
        assert!(trim(&noise, 300., 200).is_none());
        for hz in [880., 660., 320.] {
            let mut pcm = vec![0; CHUNK * 6];
            pcm.extend((0..CHUNK * 8).map(|i| {
                (9000. * (2. * std::f64::consts::PI * hz * i as f64 / RATE as f64).sin()) as i16
            }));
            pcm.extend(vec![0; CHUNK * 6]);
            assert!(trim(&pcm, 300., 200).is_none(), "cue {hz} passed");
            // Real sound following the cue must still be retained.
            pcm.extend((0..CHUNK * 8).map(|i| if i % 2 == 0 { 2000 } else { -2000 }));
            assert!(trim(&pcm, 300., 200).is_some());
        }
    }
    #[test]
    fn cue_with_speaker_distortion_and_tail_is_not_speech() {
        let mut pcm = vec![0; CHUNK * 10];
        pcm.extend((0..CHUNK * 6).map(|i| {
            let phase = std::f64::consts::TAU * 880. * i as f64 / RATE as f64;
            (900. * phase.sin() + 500. * (2. * phase).sin()) as i16
        }));
        pcm.extend(vec![0; CHUNK * 10]);
        assert!(trim(&pcm, 300., 200).is_none());
    }
    #[test]
    fn full_scale_does_not_overflow() {
        assert_eq!(rms(&[i16::MIN; i16::MAX as usize]), 32768.);
    }
}
