//! Keep the speaker stream warm only while a dictation job owns a lease.
//! This avoids reopening the audio graph for each start/stop/cancel tone.
use crate::{audio, config::Config};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, OnceLock,
    },
    time::{Duration, Instant},
};

enum Event {
    Hold(Arc<AtomicBool>),
    Release,
    Tone(f32, Instant),
}
static SENDER: OnceLock<mpsc::Sender<Event>> = OnceLock::new();
pub struct Lease {
    sender: mpsc::Sender<Event>,
    ready: Arc<AtomicBool>,
}
impl Lease {
    /// The output startup attempt has finished; animation may now approach its cue.
    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.sender.send(Event::Release);
    }
}
fn sender(c: &Config) -> mpsc::Sender<Event> {
    SENDER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            let config = c.clone();
            std::thread::spawn(move || run(config, rx));
            tx
        })
        .clone()
}
pub fn warm(c: &Config) -> Option<Lease> {
    if !c.beep {
        return None;
    }
    let tx = sender(c);
    let ready = Arc::new(AtomicBool::new(false));
    if tx.send(Event::Hold(ready.clone())).is_err() {
        // Let the UI continue if the optional feedback worker is unavailable.
        ready.store(true, Ordering::Release);
    }
    Some(Lease { sender: tx, ready })
}
pub fn beep(c: &Config, hz: f32) {
    if c.beep {
        let _ = sender(c).send(Event::Tone(hz, Instant::now()));
    }
}
fn tone(hz: f32) -> VecDeque<i16> {
    (0..2880)
        .map(|i| {
            let envelope = (i as f32 / 80.).min(1.).min((2880 - i) as f32 / 80.);
            ((i as f32 * hz * std::f32::consts::TAU / audio::RATE as f32).sin() * 9000. * envelope)
                as i16
        })
        .collect()
}
fn run(c: Config, rx: mpsc::Receiver<Event>) {
    let mut output = native::Output::default();
    let mut holds = 0usize;
    let mut samples = VecDeque::new();
    let mut pending = None;
    let mut last_tone = Instant::now();
    loop {
        let timeout = if output.open() {
            Duration::ZERO // pa_simple_write paces the stream; sleeping here causes underruns.
        } else {
            Duration::from_secs(60)
        };
        match rx.recv_timeout(timeout) {
            Ok(Event::Hold(ready)) => {
                holds += 1;
                output.connect(&c);
                // Also release the animation for a failed/overridden native backend:
                // those cues retain the legacy player fallback.
                ready.store(true, Ordering::Release);
            }
            Ok(Event::Release) => {
                holds = holds.saturating_sub(1);
            }
            Ok(Event::Tone(hz, queued)) => {
                output.connect(&c);
                if output.open() {
                    samples = tone(hz); // A new cancel cue supersedes remaining start audio.
                    pending = Some((hz, queued));
                    last_tone = Instant::now();
                } else {
                    audio::legacy_beep(&c, hz);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if output.open() {
            if holds == 0 && samples.is_empty() && last_tone.elapsed() > Duration::from_millis(250)
            {
                output.close();
                continue;
            }
            let mut data = Vec::with_capacity(320);
            for _ in 0..160 {
                data.extend(samples.pop_front().unwrap_or(0).to_le_bytes());
            }
            if !output.write(&data) {
                output.close();
                samples.clear();
                if let Some((hz, _)) = pending.take() {
                    audio::legacy_beep(&c, hz);
                }
            } else if let Some((hz, queued)) = pending.take() {
                if c.boolean("VRBTN_DEBUG", false).unwrap_or(false) {
                    eprintln!(
                        "feedback: {hz:.0} Hz queued to warm stream in {} ms",
                        queued.elapsed().as_millis()
                    );
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod native {
    use crate::config::Config;
    use std::{
        ffi::{c_char, c_int, c_void},
        ptr,
        time::Instant,
    };
    #[repr(C)]
    struct Spec {
        format: c_int,
        rate: u32,
        channels: u8,
    }
    #[repr(C)]
    struct Attr {
        maxlength: u32,
        tlength: u32,
        prebuf: u32,
        minreq: u32,
        fragsize: u32,
    }
    type New = unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        c_int,
        *const c_char,
        *const c_char,
        *const Spec,
        *const c_void,
        *const Attr,
        *mut c_int,
    ) -> *mut c_void;
    type Write = unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_int) -> c_int;
    type Free = unsafe extern "C" fn(*mut c_void);
    pub struct Output {
        library: *mut c_void,
        stream: *mut c_void,
        new: Option<New>,
        write: Option<Write>,
        free: Option<Free>,
    }
    impl Default for Output {
        fn default() -> Self {
            Self {
                library: ptr::null_mut(),
                stream: ptr::null_mut(),
                new: None,
                write: None,
                free: None,
            }
        }
    }
    impl Output {
        pub fn open(&self) -> bool {
            !self.stream.is_null()
        }
        pub fn connect(&mut self, c: &Config) {
            if self.open() || c.values.contains_key("PAPLAY_BIN") {
                return;
            }
            let started = Instant::now();
            // SAFETY: fixed system library/symbol names, ABI copied from libpulse-simple.
            unsafe {
                if self.library.is_null() {
                    self.library = libc::dlopen(
                        c"libpulse-simple.so.0".as_ptr(),
                        libc::RTLD_NOW | libc::RTLD_LOCAL,
                    );
                    if self.library.is_null() {
                        return;
                    }
                    let new = libc::dlsym(self.library, c"pa_simple_new".as_ptr());
                    let write = libc::dlsym(self.library, c"pa_simple_write".as_ptr());
                    let free = libc::dlsym(self.library, c"pa_simple_free".as_ptr());
                    if new.is_null() || write.is_null() || free.is_null() {
                        libc::dlclose(self.library);
                        self.library = ptr::null_mut();
                        return;
                    }
                    self.new = Some(std::mem::transmute::<*mut c_void, New>(new));
                    self.write = Some(std::mem::transmute::<*mut c_void, Write>(write));
                    self.free = Some(std::mem::transmute::<*mut c_void, Free>(free));
                }
                let spec = Spec {
                    format: 3,
                    rate: 16000,
                    channels: 1,
                };
                let attr = Attr {
                    maxlength: u32::MAX,
                    tlength: 640,
                    prebuf: 0,
                    minreq: 160,
                    fragsize: u32::MAX,
                };
                let mut error = 0;
                self.stream = self.new.unwrap()(
                    ptr::null(),
                    c"Frame Voice".as_ptr(),
                    1,
                    ptr::null(),
                    c"Dictation cues".as_ptr(),
                    &spec,
                    ptr::null(),
                    &attr,
                    &mut error,
                );
            }
            if self.open() && c.boolean("VRBTN_DEBUG", false).unwrap_or(false) {
                eprintln!(
                    "feedback: speaker prearmed in {} ms",
                    started.elapsed().as_millis()
                );
            }
        }
        pub fn write(&self, data: &[u8]) -> bool {
            let mut error = 0;
            // SAFETY: stream is owned by this worker; data remains alive through the call.
            unsafe {
                self.write.unwrap()(self.stream, data.as_ptr().cast(), data.len(), &mut error) == 0
            }
        }
        pub fn close(&mut self) {
            if self.open() {
                unsafe {
                    self.free.unwrap()(self.stream);
                }
                self.stream = ptr::null_mut();
            }
        }
    }
    impl Drop for Output {
        fn drop(&mut self) {
            self.close();
            if !self.library.is_null() {
                unsafe {
                    libc::dlclose(self.library);
                }
            }
        }
    }
}
#[cfg(not(target_os = "linux"))]
mod native {
    use crate::config::Config;
    #[derive(Default)]
    pub struct Output {
        _private: (),
    }
    impl Output {
        pub fn open(&self) -> bool {
            false
        }
        pub fn connect(&mut self, _: &Config) {}
        pub fn write(&self, _: &[u8]) -> bool {
            false
        }
        pub fn close(&mut self) {}
    }
}
