use anyhow::{bail, Result};
#[derive(Default)]
pub struct Snapshot {
    pub active: [u8; 5],
    pub state: [u8; 5],
}
// The C++ shim owns one global runtime and must stay on its owning thread.
pub struct OpenVr {
    _thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}
#[cfg(feature = "openvr")]
static OWNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "openvr")]
unsafe extern "C" {
    fn fv_open(library: *const std::ffi::c_char, manifest: *const std::ffi::c_char) -> i32;
    fn fv_clear_hand() -> i32;
    fn fv_poll(active: *mut u8, state: *mut u8) -> i32;
    fn fv_hmd_tracked() -> i32;
    fn fv_notify(text: *const std::ffi::c_char) -> i32;
    fn fv_badge(state: i32, hand: i32) -> i32;
    fn fv_badge_progress(progress: f32, hand: i32) -> i32;
    fn fv_badge_tick(hand: i32) -> i32;
    fn fv_close();
}
impl OpenVr {
    pub fn new(library: &str, manifest: &str) -> Result<Self> {
        #[cfg(feature = "openvr")]
        {
            let library = std::ffi::CString::new(library)?;
            let manifest = std::ffi::CString::new(
                std::fs::canonicalize(manifest)?
                    .to_string_lossy()
                    .as_bytes(),
            )?;
            if OWNED
                .compare_exchange(
                    false,
                    true,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                )
                .is_err()
            {
                bail!("OpenVR runtime already owned in this process");
            }
            let runtime = Self {
                _thread_bound: std::marker::PhantomData,
            };
            // SAFETY: strings outlive the call; the shim owns the runtime until Drop.
            let error = unsafe { fv_open(library.as_ptr(), manifest.as_ptr()) };
            if error != 0 {
                bail!("OpenVR initialization failed: {error}");
            }
            Ok(runtime)
        }
        #[cfg(not(feature = "openvr"))]
        {
            let _ = (library, manifest);
            bail!("daemon needs a Linux build with --features openvr (see README.md)")
        }
    }
    /// Queue a transient notification in the SteamVR/Steam UI (two lines max).
    pub fn notify(&mut self, summary: &str, body: &str) -> Result<()> {
        #[cfg(feature = "openvr")]
        {
            let text = std::ffi::CString::new(format!("{summary}\n{body}"))?;
            // SAFETY: the string outlives the call; the shim owns the runtime.
            let error = unsafe { fv_notify(text.as_ptr()) };
            if error != 0 {
                bail!("OpenVR notification failed: {error}");
            }
            Ok(())
        }
        #[cfg(not(feature = "openvr"))]
        {
            let _ = (summary, body);
            bail!("OpenVR not compiled in")
        }
    }
    /// Set the badge state: 0 hidden, 1 recording, 2 processing, 3 canceled.
    /// `hand` is 0 = left, 1 = right, anything else = headset fallback.
    pub fn badge(&mut self, state: i32, hand: i32) -> Result<()> {
        #[cfg(feature = "openvr")]
        {
            // SAFETY: the shim owns the runtime on this thread.
            let error = unsafe { fv_badge(state, hand) };
            if error != 0 {
                eprintln!("OpenVR badge error: {error}");
                bail!("OpenVR badge error: {error}");
            }
            Ok(())
        }
        #[cfg(not(feature = "openvr"))]
        {
            let _ = (state, hand);
            bail!("OpenVR not compiled in")
        }
    }
    /// Grow the microphone only; the white frame remains at full size.
    pub fn badge_progress(&mut self, progress: f32, hand: i32) -> Result<()> {
        #[cfg(feature = "openvr")]
        {
            // SAFETY: the shim owns the runtime on this thread and validates progress.
            let error = unsafe { fv_badge_progress(progress, hand) };
            if error != 0 {
                bail!("OpenVR badge animation error: {error}");
            }
            Ok(())
        }
        #[cfg(not(feature = "openvr"))]
        {
            let _ = (progress, hand);
            bail!("OpenVR not compiled in")
        }
    }
    /// Re-billboard the visible recording badge toward `hand` (0=left, 1=right).
    /// Called frequently; errors are ignored so a transient pose gap is silent.
    pub fn badge_tick(&mut self, hand: i32) {
        #[cfg(feature = "openvr")]
        unsafe {
            fv_badge_tick(hand);
        }
        #[cfg(not(feature = "openvr"))]
        let _ = hand;
    }
    /// Whether the HMD pose is valid (headset worn/tracked).
    pub fn hmd_tracked(&mut self) -> bool {
        #[cfg(feature = "openvr")]
        {
            // SAFETY: the shim owns the runtime on this thread.
            unsafe { fv_hmd_tracked() != 0 }
        }
        #[cfg(not(feature = "openvr"))]
        {
            false
        }
    }
    pub fn clear_hand(&mut self) -> Option<usize> {
        #[cfg(feature = "openvr")]
        {
            // SAFETY: called on the owning thread after the action-state update.
            match unsafe { fv_clear_hand() } {
                0 => Some(0),
                1 => Some(1),
                _ => None,
            }
        }
        #[cfg(not(feature = "openvr"))]
        {
            None
        }
    }
    pub fn poll(&mut self) -> Result<Snapshot> {
        #[cfg(feature = "openvr")]
        {
            let mut snapshot = Snapshot::default();
            // SAFETY: both output arrays contain five writable bytes.
            let error =
                unsafe { fv_poll(snapshot.active.as_mut_ptr(), snapshot.state.as_mut_ptr()) };
            if error != 0 {
                bail!("OpenVR poll failed: {error}");
            }
            Ok(snapshot)
        }
        #[cfg(not(feature = "openvr"))]
        {
            bail!("OpenVR not compiled in")
        }
    }
}
impl Drop for OpenVr {
    fn drop(&mut self) {
        #[cfg(feature = "openvr")]
        unsafe {
            fv_close();
            OWNED.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
