//! Optional WebRTC voice evidence from SteamOS's existing audio-processing library.
//! A detector is private to each recording, so adaptive state cannot leak across jobs.
use std::ffi::{c_int, c_void};

type Create = unsafe extern "C" fn() -> *mut c_void;
type Init = unsafe extern "C" fn(*mut c_void) -> c_int;
type Mode = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type Process = unsafe extern "C" fn(*mut c_void, c_int, *const i16, usize) -> c_int;
type Free = unsafe extern "C" fn(*mut c_void);

struct Detector {
    library: *mut c_void,
    instance: *mut c_void,
    process: Process,
    free: Free,
}
impl Detector {
    fn new() -> Option<Self> {
        // These are the same ABI/version used by SteamOS; don't try legacy ABIs.
        let libraries: &[&std::ffi::CStr] = if cfg!(target_os = "linux") {
            &[
                c"libwebrtc-audio-processing-2.so.1",
                c"libwebrtc-audio-processing-2.so",
            ]
        } else {
            &[]
        };
        for name in libraries {
            // SAFETY: fixed library/symbol names and the public WebRtcVad C ABI.
            unsafe {
                let library = libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                if library.is_null() {
                    continue;
                }
                let create = libc::dlsym(library, c"WebRtcVad_Create".as_ptr());
                let init = libc::dlsym(library, c"WebRtcVad_Init".as_ptr());
                let mode = libc::dlsym(library, c"WebRtcVad_set_mode".as_ptr());
                let process = libc::dlsym(library, c"WebRtcVad_Process".as_ptr());
                let free = libc::dlsym(library, c"WebRtcVad_Free".as_ptr());
                if [create, init, mode, process, free]
                    .iter()
                    .any(|symbol| symbol.is_null())
                {
                    libc::dlclose(library);
                    continue;
                }
                let create = std::mem::transmute::<*mut c_void, Create>(create);
                let init = std::mem::transmute::<*mut c_void, Init>(init);
                let mode = std::mem::transmute::<*mut c_void, Mode>(mode);
                let free = std::mem::transmute::<*mut c_void, Free>(free);
                let instance = create();
                if instance.is_null() {
                    libc::dlclose(library);
                    continue;
                }
                if init(instance) != 0 || mode(instance, 3) != 0 {
                    free(instance);
                    libc::dlclose(library);
                    continue;
                }
                return Some(Self {
                    library,
                    instance,
                    process: std::mem::transmute::<*mut c_void, Process>(process),
                    free,
                });
            }
        }
        None
    }
}
impl Drop for Detector {
    fn drop(&mut self) {
        // SAFETY: this detector owns both handles, with no concurrent calls.
        unsafe {
            (self.free)(self.instance);
            libc::dlclose(self.library);
        }
    }
}

/// Require a contiguous voice run as well as the separate energy/cue gate.
/// None means unavailable/failed: callers retain the portable energy gate.
pub fn contains_speech(samples: &[i16], threshold: f64, min_speech_ms: u64) -> Option<bool> {
    let detector = Detector::new()?;
    let required = min_speech_ms.div_ceil(20).max(2) as usize;
    let candidates = crate::audio::speech_candidates(samples, threshold);
    let mut run = 0;
    for (index, frame) in samples.chunks_exact(320).enumerate() {
        // 20 ms at 16 kHz.
        // SAFETY: valid initialized detector, mono signed PCM, supported frame size.
        let voice =
            unsafe { (detector.process)(detector.instance, 16000, frame.as_ptr(), frame.len()) };
        if voice < 0 {
            return None;
        }
        // Both guards must agree on the same audio, so a leading cue cannot
        // supply the VAD evidence for an unrelated bird/noise burst later.
        let candidate = candidates[index * 320 / crate::audio::CHUNK];
        run = if voice > 0 && candidate { run + 1 } else { 0 };
        if run >= required {
            return Some(true);
        }
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voice_evidence_rejects_silence_and_chirps_but_keeps_quiet_speech() {
        if Detector::new().is_none() {
            return; // Linux integration runs also verify that the library is available.
        }
        assert_eq!(contains_speech(&vec![0; 32000], 300., 200), Some(false));
        let mut chirp = vec![0; 8000];
        chirp.extend((0..8000).map(|i| {
            let t = i as f64 / 16000.;
            (2000. * (std::f64::consts::TAU * (3500. * t + 900. * t * t)).sin()) as i16
        }));
        chirp.extend(vec![0; 8000]);
        // This sustained non-voice sound passes a plain volume/duration gate.
        assert!(crate::audio::trim(&chirp, 300., 200).is_some());
        assert_eq!(contains_speech(&chirp, 300., 200), Some(false));
        let mut cue_then_chirp = vec![0; 8000];
        cue_then_chirp.extend((0..4000).map(|i| {
            let phase = std::f64::consts::TAU * 880. * i as f64 / 16000.;
            (900. * phase.sin() + 500. * (phase * 2.).sin()) as i16
        }));
        cue_then_chirp.extend_from_slice(&chirp[8000..]);
        assert!(crate::audio::trim(&cue_then_chirp, 300., 200).is_some());
        assert_eq!(contains_speech(&cue_then_chirp, 300., 200), Some(false));
        let bytes = include_bytes!("../tests/fixtures/webrtc-speech-8k.raw");
        for scale in [1., 0.25, 0.1] {
            let samples: Vec<_> = bytes
                .chunks_exact(2)
                .flat_map(|b| {
                    let sample = (i16::from_le_bytes([b[0], b[1]]) as f64 * scale) as i16;
                    [sample, sample] // 8 kHz fixture converted to 16 kHz for this test.
                })
                .collect();
            crate::audio::trim(&samples, 300., 200).expect("real speech energy");
            assert_eq!(
                contains_speech(&samples, 300., 200),
                Some(true),
                "speech scale {scale}"
            );
        }
    }
}
