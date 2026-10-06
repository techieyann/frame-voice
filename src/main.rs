use anyhow::{bail, Context as _, Result};
use frame_voice::{
    audio::{self, Control},
    cadence,
    config::Config,
    context::{Context, Snapshot},
    gesture::{Event, Gesture},
    inject,
    openvr::OpenVr,
    pipeline,
    speech::Output,
    usage,
    worker::Worker,
};
use std::{
    fs::OpenOptions,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
static SHUTDOWN: AtomicBool = AtomicBool::new(false);
// Badge states (see shim fv_badge): 0 hidden, 1 recording, 2 processing,
// 3 canceled/no speech. CANCEL_NOTICE_MS keeps the off-mic visible briefly.
const BADGE_HIDDEN: i32 = 0;
const BADGE_RECORDING: i32 = 1;
const BADGE_PROCESSING: i32 = 2;
const BADGE_CANCELED: i32 = 3;
const CANCEL_NOTICE_MS: u64 = 900;
/// Default time the recording badge takes to grow to full size. The start tone
/// waits for the growth after capture/output startup, so cold startup cannot
/// leave a full-sized mic waiting for its first tone.
/// Override with `VRBTN_GROW_MS`.
const BADGE_GROWTH_MS: u64 = 180;
/// The start tone is triggered this many ms before the mic reaches full size,
/// so the sound (which has output latency) lands as the icon completes.
/// Override with `VRBTN_BEEP_LEAD_MS`.
const BEEP_LEAD_MS: u64 = 30;
extern "C" fn shutdown(_: libc::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}
fn signals() {
    unsafe {
        libc::signal(libc::SIGINT, shutdown as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, shutdown as *const () as libc::sighandler_t);
    }
}
/// PID of the SteamVR server process. When SteamVR restarts, the daemon's
/// OpenVR session goes stale: input silently stops and overlay calls start
/// returning RequestFailed. Watching the server lets us exit so systemd rebinds.
fn vrserver_pid() -> Option<i32> {
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        if let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) {
            if comm.trim() == "vrserver" {
                return entry.file_name().to_str()?.parse().ok();
            }
        }
    }
    None
}
fn lock(c: &Config) -> Result<std::fs::File> {
    let fallback = format!("/run/user/{}", unsafe { libc::getuid() });
    let runtime = Path::new(c.get("XDG_RUNTIME_DIR", &fallback));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(runtime.join("frame-voice.lock"))
        .context("open daemon lock in XDG_RUNTIME_DIR")?;
    // Kernel-owned lock: no stale PID signaling, no unlink races.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("another frame-voice instance is running");
    }
    Ok(file)
}
fn eligible(start: &Snapshot, current: &Snapshot, control: &Control) -> bool {
    !control.canceled()
        && start.focus.is_some()
        && start.revision == current.revision
        && start.focus == current.focus
}
fn armed(until: Option<Instant>, now: Instant) -> bool {
    until.is_some_and(|t| now < t)
}
struct Job {
    feedback: Option<frame_voice::feedback::Lease>,
    control: Control,
    focus: Snapshot,
    worker: Worker<Result<Output>>,
    /// The hold reached the arm threshold: this will be transcribed if released normally.
    committed: bool,
    /// The start tone has been played for this job.
    beeped: bool,
}
impl Job {
    fn join(&mut self) {
        self.worker.join();
    }
    /// Committed with the capture backend actually delivering PCM: mic is live.
    fn live_ready(&self) -> bool {
        self.committed
            && !self.control.canceled()
            && !self.control.stop.load(Ordering::SeqCst)
            && self.control.live.load(Ordering::SeqCst)
    }
}
// The start tone fires once, on the first live-committed capture, led slightly
// ahead of the mic reaching full size so the audible tone lands with it.
fn ready_cue(job: &Job) -> bool {
    job.live_ready() && !job.beeped
}
/// Keep the small badge visible while capture/output start. Growth starts once
/// both are ready, rather than spending its animation budget on cold startup.
fn activation_elapsed(start: &mut Instant, now: Instant, ready: bool) -> Duration {
    if !ready {
        *start = now;
        Duration::ZERO
    } else {
        now.saturating_duration_since(*start)
    }
}
fn touch_available(active: &[u8; 5]) -> bool {
    active[0] != 0 || active[1] != 0
}
fn input_invalidated(previous: &[u8; 5], current: &[u8; 5], hand: Option<usize>) -> bool {
    touch_available(previous) != touch_available(current)
        || hand.is_some_and(|i| previous[i] != 0 && current[i] == 0)
}
impl Drop for Job {
    fn drop(&mut self) {
        self.control.cancel();
        self.join();
    }
}
/// Start a capture worker. It records immediately (prearm) and is committed
/// later, when the gesture reaches the arm threshold.
fn spawn(c: &Config, focus: &Snapshot) -> Job {
    let control = Control::default();
    let worker = control.clone();
    let config = c.clone();
    let worker = Worker::spawn(move || {
        std::panic::catch_unwind(|| pipeline::run(&config, &worker, None))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("dictation worker panicked")))
    });
    Job {
        feedback: frame_voice::feedback::warm(c),
        control,
        focus: focus.clone(),
        worker,
        committed: false,
        beeped: false,
    }
}
// A canceled HTTP operation may still be waiting for its bounded timeout. Keep
// it off the input thread. Admission limits the total to three outstanding jobs.
const MAX_RETIRED: usize = 2;
fn retire(job: &mut Option<Job>, retired: &mut Vec<Job>) {
    if let Some(j) = job.take() {
        j.control.cancel();
        retired.push(j);
    }
}
/// Regenerate the SteamVR controller binding from configuration so submit/clear
/// buttons and per-hand enable/disable are plain config. Run before OpenVR reads
/// the manifest, so SteamVR loads the new binding at startup (no live reload).
fn write_controller_binding(c: &Config, manifest: &str) -> Result<()> {
    let dir = Path::new(manifest)
        .parent()
        .context("controller manifest has no parent directory")?;
    let right_buttons = ["a", "b", "x", "y"];
    let left_buttons = ["dpad_up", "dpad_down", "dpad_left", "dpad_right"];
    let pick = |key: &str, default: &str, allowed: &[&str]| -> Option<String> {
        let value = c.get(key, default).trim().to_ascii_lowercase();
        if value.is_empty() || value == "none" || value == "off" {
            None
        } else if allowed.contains(&value.as_str()) {
            Some(value)
        } else {
            eprintln!("controller binding: ignoring invalid {key}={value}");
            None
        }
    };
    let joystick = |hand: &str, touch: &str| {
        serde_json::json!({
            "inputs": {
                "touch": { "output": touch },
                "click": { "output": "/actions/voicedict/in/cancel" }
            },
            "mode": "joystick",
            "path": format!("/user/hand/{hand}/input/thumbstick")
        })
    };
    let button = |hand: &str, name: &str, action: &str| {
        serde_json::json!({
            "inputs": { "click": { "output": action } },
            "mode": "button",
            "path": format!("/user/hand/{hand}/input/{name}")
        })
    };
    let mut sources = Vec::new();
    if c.boolean("VRBTN_DICTATE_LEFT", true)? {
        sources.push(joystick("left", "/actions/voicedict/in/touch_left"));
    }
    if c.boolean("VRBTN_DICTATE_RIGHT", true)? {
        sources.push(joystick("right", "/actions/voicedict/in/touch_right"));
    }
    if let Some(name) = pick("VRBTN_SUBMIT_LEFT", "dpad_right", &left_buttons) {
        sources.push(button("left", &name, "/actions/voicedict/in/submit"));
    }
    if let Some(name) = pick("VRBTN_CLEAR_LEFT", "dpad_left", &left_buttons) {
        sources.push(button("left", &name, "/actions/voicedict/in/clear"));
    }
    if let Some(name) = pick("VRBTN_SUBMIT_RIGHT", "a", &right_buttons) {
        sources.push(button("right", &name, "/actions/voicedict/in/submit"));
    }
    if let Some(name) = pick("VRBTN_CLEAR_RIGHT", "b", &right_buttons) {
        sources.push(button("right", &name, "/actions/voicedict/in/clear"));
    }
    let doc = serde_json::json!({
        "action_manifest_version": 0,
        "bindings": { "/actions/voicedict": { "sources": sources } },
        "category": "steamvr_input",
        "controller_type": "frame_controller",
        "interaction_profile": "/interaction_profiles/valve/frame_controller_valve"
    });
    let path = dir.join("frame_controller_binding.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&doc)?)
        .with_context(|| format!("write controller binding {}", path.display()))?;
    Ok(())
}
fn daemon(c: Config) -> Result<()> {
    let _lock = lock(&c)?;
    let manifest = c.values.get("VRBTN_MANIFEST").cloned().unwrap_or_else(|| {
        c.home
            .join(".local/share/frame-voice/actions.json")
            .to_string_lossy()
            .into_owned()
    });
    write_controller_binding(&c, &manifest)?;
    let mut vr = OpenVr::new(
        c.get(
            "OPENVR_LIBRARY",
            "/opt/steamvr/bin/linuxarm64/libopenvr_api.so",
        ),
        &manifest,
    )?;
    // SteamVR restarting leaves our OpenVR session stale, and the daemon has no
    // way to notice from the action data alone. Watch the server process on a
    // separate thread and exit nonzero so systemd re-binds us automatically.
    std::thread::spawn(|| {
        let mut pid = vrserver_pid();
        while !SHUTDOWN.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_secs(2));
            if SHUTDOWN.load(Ordering::SeqCst) {
                break;
            }
            let now = vrserver_pid();
            if now != pid {
                eprintln!(
                    "SteamVR runtime changed (vrserver {pid:?} -> {now:?}); exiting to re-bind"
                );
                std::process::exit(1);
            }
            pid = now;
        }
    });
    let context = Context::start();
    let mut gesture = Gesture::default();
    gesture.arm_ms = c.timing("VRBTN_ARM_MS", 150)?;
    gesture.tap_ms = c.timing("VRBTN_TAP_MS", 250)?;
    gesture.ready_ms = c.timing("VRBTN_READY_MS", 600)?;
    let beep_lead = Duration::from_millis(c.timing("VRBTN_BEEP_LEAD_MS", BEEP_LEAD_MS)?);
    // Even custom hold thresholds must leave room to queue the cue before full size.
    let badge_growth = Duration::from_millis(c.timing("VRBTN_GROW_MS", BADGE_GROWTH_MS)?)
        .max(Duration::from_millis(gesture.arm_ms).saturating_add(beep_lead));
    let action_window = match c.values.get("VOICE_ACTION_WINDOW_MS") {
        Some(v) => v
            .parse::<u64>()
            .context("VOICE_ACTION_WINDOW_MS must be numeric")?,
        None => 10_000,
    };
    let debug = c.boolean("VRBTN_DEBUG", false)?;
    let mut retired: Vec<Job> = Vec::new();
    let mut next_badge_tick = Instant::now();
    let mut job: Option<Job> = None;
    let clock = Instant::now();
    let mut previous = [0; 5];
    let mut active = [0; 5];
    // Action indices: 0 touch_left, 1 touch_right, 2 submit, 3 clear,
    // 4 cancel. Submit/clear are driven by A/B and by the left D-pad, whose
    // directions mirror the right face buttons.
    let mut baseline = true;
    let mut revision = 0;
    let mut focus = Snapshot::default();
    let mut actions_armed_until: Option<Instant> = None;
    let mut badge_hand: i32 = -1;
    let mut cancel_until: Option<Instant> = None;
    // When the hold releases, the processing icon only starts after this short
    // delay, so an immediate silence-cancel goes straight from blue to the
    // off-mic icon. The deadline bounds it so a hung request cannot leave the
    // processing icon stuck on.
    let mut warming_at: Option<Instant> = None;
    let mut processing_at: Option<Instant> = None;
    let mut processing_deadline: Option<Instant> = None;
    let notify = c.boolean("VOICE_NOTIFY", true).unwrap_or(true);
    // SteamVR can accept the action manifest before its controller bindings are
    // ready, leaving the action set permanently inactive (a boot-order race). A
    // fresh process re-binds cleanly (a manual restart is the known fix), so if
    // the action set is not active shortly after startup, exit and let systemd
    // relaunch us. Rebinding in-process leaves a flapping, unusable action set.
    // Wait until the action set is active. SteamVR can accept the manifest before
    // its controller bindings load (a boot-order race), leaving every action
    // inactive; a fresh process re-binds cleanly, so exit and let systemd relaunch
    // when the HMD is tracked but no action is active. While the HMD is not
    // tracked (headset off, controllers idle) just wait, so we never restart in a
    // loop when nothing is happening.
    let mut tracked_inactive = 0u32;
    let mut touch_hand: Option<usize> = None;
    loop {
        if SHUTDOWN.load(Ordering::SeqCst) {
            return Ok(());
        }
        if vr
            .poll()
            .map(|s| touch_available(&s.active))
            .unwrap_or(false)
        {
            break;
        }
        if vr.hmd_tracked() {
            tracked_inactive += 1;
            if tracked_inactive >= 20 {
                bail!(
                    "OpenVR controller bindings not active with the HMD tracked; re-initializing"
                );
            }
        } else {
            tracked_inactive = 0;
        }
        thread::sleep(Duration::from_millis(500));
    }
    eprintln!("OpenVR controller bindings active");
    eprintln!("frame-voice ready: cap tap then hold; A submit; B clear");
    while !SHUTDOWN.load(Ordering::SeqCst) {
        retired.retain(|j| !j.worker.is_finished());
        // Deliver usage warnings through OpenVR so they appear in the headset /
        // Steam UI. Only a real success arms the cooldown.
        if let Some((summary, body)) = usage::take_pending() {
            let ok = vr.notify(&summary, &body).is_ok();
            if !ok {
                eprintln!("usage notification failed");
            }
            usage::delivered(ok);
        }
        // Retire the transient "canceled/no speech" badge after its notice.
        if let Some(until) = cancel_until {
            if Instant::now() >= until {
                cancel_until = None;
                if notify {
                    let _ = vr.badge(BADGE_HIDDEN, -1);
                }
            }
        }
        if context.revision() != focus.revision {
            focus = context.snapshot();
        }
        if debug && focus.revision != revision {
            eprintln!("context: rev={} focus={:?}", focus.revision, focus.focus);
        }
        if focus.revision != revision || focus.focus.is_none() {
            if let Some(j) = &job {
                j.control.cancel();
            }
            gesture.reset();
            warming_at = None;
            baseline = true;
            revision = focus.revision;
            actions_armed_until = None;
            let _ = vr.badge(BADGE_HIDDEN, -1);
        }
        if focus.focus.is_none() {
            context.wait_for_change(focus.revision);
            continue;
        }
        let snapshot = match vr.poll() {
            Ok(s) => s,
            Err(e) => {
                if let Some(j) = &job {
                    j.control.cancel();
                }
                return Err(e);
            }
        };
        if debug && snapshot.state != previous {
            eprintln!(
                "input: active={:?} state={:?}",
                snapshot.active, snapshot.state
            );
        }
        let availability_changed = snapshot.active != active;
        let owning_hand = if job.as_ref().is_some_and(|j| j.committed) || warming_at.is_some() {
            usize::try_from(badge_hand).ok().filter(|&i| i < 2)
        } else {
            touch_hand
        };
        if debug && (baseline || availability_changed) {
            eprintln!(
                "availability: active={:?} state={:?}",
                snapshot.active, snapshot.state
            );
        }
        if baseline || input_invalidated(&active, &snapshot.active, owning_hand) {
            if debug {
                eprintln!(
                    "availability: active={:?} state={:?}",
                    snapshot.active, snapshot.state
                );
            }
            if !baseline {
                if let Some(j) = &job {
                    j.control.cancel();
                }
                let _ = vr.badge(BADGE_HIDDEN, -1);
            }
            gesture.reset();
            touch_hand = None;
            previous = snapshot.state;
            active = snapshot.active;
            baseline = false;
            gesture.update(
                clock.elapsed().as_millis() as u64,
                snapshot.state[0] != 0 || snapshot.state[1] != 0,
            );
            thread::sleep(cadence::input_interval(
                snapshot.state[0] != 0 || snapshot.state[1] != 0,
                snapshot.active[0] != 0 || snapshot.active[1] != 0,
            ));
            continue;
        }
        if availability_changed {
            // Newly connected buttons are baselined; their initial held state
            // must not submit/clear/cancel an existing job. The other hand's
            // gesture remains usable when a controller sleeps or reconnects.
            for i in 2..5 {
                if active[i] == 0 && snapshot.active[i] != 0 {
                    previous[i] = snapshot.state[i];
                }
            }
            active = snapshot.active;
        }
        if let Some(hand) = snapshot.state[..2].iter().position(|&state| state != 0) {
            touch_hand = Some(hand);
        }
        // Losing all touch action data invalidates the recording; never treat it as release.
        if snapshot.active[0] == 0 && snapshot.active[1] == 0 {
            if debug && availability_changed {
                eprintln!("availability lost: active={:?}", snapshot.active);
            }
            if let Some(j) = &job {
                j.control.cancel();
            }
            gesture.reset();
            let _ = vr.badge(BADGE_HIDDEN, -1);
        } else {
            match gesture.update(
                clock.elapsed().as_millis() as u64,
                snapshot.state[0] != 0 || snapshot.state[1] != 0,
            ) {
                Some(Event::Arm) => {
                    if debug {
                        eprintln!("gesture: arm");
                    }
                    // Prearm the mic on the tap. A committed dictation is left
                    // alone (a bare tap must not cancel it); a stale prearm is replaced.
                    if job.as_ref().is_none_or(|j| !j.committed) && retired.len() < MAX_RETIRED {
                        warming_at = None;
                        retire(&mut job, &mut retired);
                        job = Some(spawn(&c, &focus));
                    }
                }
                Some(Event::Hold) => {
                    if job
                        .as_ref()
                        .is_some_and(|j| !j.committed && !j.control.canceled())
                    {
                        warming_at = Some(Instant::now());
                        cancel_until = None;
                        badge_hand = if snapshot.state[0] != 0 { 0 } else { 1 };
                        if notify {
                            let _ = vr.badge_progress(0.0, badge_hand);
                        }
                    }
                }
                Some(Event::Start) => {
                    if debug {
                        eprintln!("gesture: start");
                    }
                    actions_armed_until = None;
                    // A new recording supersedes any pending badge timer.
                    cancel_until = None;
                    processing_at = None;
                    processing_deadline = None;
                    // The held cap decides which controller carries the badge.
                    let hand = if snapshot.state[0] != 0 {
                        0
                    } else if snapshot.state[1] != 0 {
                        1
                    } else {
                        -1
                    };
                    badge_hand = hand;
                    match job.as_mut() {
                        Some(j) if !j.committed => {
                            j.committed = true;
                        }
                        Some(_) => {
                            warming_at = None;
                            // A previous dictation is still recording/processing.
                            if debug {
                                eprintln!("gesture: start-busy");
                            }
                            retire(&mut job, &mut retired);
                            let _ = vr.badge(BADGE_HIDDEN, -1);
                            audio::beep(&c, 320.);
                            eprintln!("busy dictation canceled; retry after processing finishes");
                        }
                        None if retired.len() < MAX_RETIRED => {
                            // No prearm was available; start a committed capture now.
                            let mut j = spawn(&c, &focus);
                            j.committed = true;
                            job = Some(j);
                            warming_at = Some(Instant::now());
                        }
                        None => {
                            audio::beep(&c, 320.);
                            eprintln!("canceled requests still finishing; retry shortly");
                        }
                    }
                }
                Some(Event::Stop) => {
                    warming_at = None;
                    if debug {
                        eprintln!("gesture: stop");
                    }
                    if let Some(j) = &job {
                        j.control.stop();
                    }
                    // Released: show "processing" shortly, unless the result
                    // arrives first (e.g. an immediate silence cancel).
                    processing_at = Some(Instant::now());
                }
                Some(Event::Abort) => {
                    warming_at = None;
                    if debug {
                        eprintln!("gesture: abort");
                    }
                    retire(&mut job, &mut retired);
                    let _ = vr.badge(BADGE_HIDDEN, -1);
                }
                None => {}
            }
            // Cold startup holds the small badge. Once capture and output are
            // ready, the growth and cue use the same clock, with playback led
            // slightly so audible onset lands as the mic reaches full size.
            if let Some(j) = job.as_mut() {
                let activation_ready = j.control.live.load(Ordering::SeqCst)
                    && j.feedback.as_ref().is_none_or(|lease| lease.ready());
                let elapsed = warming_at
                    .as_mut()
                    .map(|at| activation_elapsed(at, Instant::now(), activation_ready));
                if activation_ready
                    && ready_cue(j)
                    && elapsed.is_none_or(|e| e + beep_lead >= badge_growth)
                {
                    j.beeped = true;
                    audio::beep(&c, 880.);
                }
                if let Some(elapsed) = elapsed {
                    if j.live_ready() && elapsed >= badge_growth && (!c.beep || j.beeped) {
                        warming_at = None;
                        if notify {
                            let _ = vr.badge(BADGE_RECORDING, badge_hand);
                        }
                    } else if notify && !j.control.canceled() && Instant::now() >= next_badge_tick {
                        let progress =
                            (elapsed.as_secs_f32() / badge_growth.as_secs_f32()).min(1.0);
                        let _ = vr.badge_progress(progress, badge_hand);
                        next_badge_tick = Instant::now() + cadence::BADGE_INTERVAL;
                    }
                }
            }
        }
        // Bound the processing badge so a hung request cannot leave it stuck on.
        if let Some(deadline) = processing_deadline {
            if Instant::now() >= deadline {
                processing_deadline = None;
                if notify {
                    let _ = vr.badge(BADGE_HIDDEN, -1);
                }
            }
        }
        // Promote to "processing" once the release has lasted past the delay.
        // The stop tone is emitted here, not in the worker, so the icon and the
        // tone share one decision. Silence never sets `voiced`, so it still goes
        // straight to the cancel cue without a preceding stop tone.
        if let Some(at) = processing_at {
            if !job.as_ref().is_some_and(|j| j.committed) {
                processing_at = None;
            } else if Instant::now() >= at + Duration::from_millis(120) {
                processing_at = None;
                let voiced = job
                    .as_ref()
                    .is_some_and(|j| j.control.voiced.load(Ordering::SeqCst));
                if voiced {
                    if notify {
                        let _ = vr.badge(BADGE_PROCESSING, badge_hand);
                        processing_deadline = Some(Instant::now() + Duration::from_secs(15));
                    }
                    audio::beep(&c, 660.);
                }
            }
        }
        // Translation follows the controller in the compositor. Re-billboard at
        // up to 125 Hz instead of issuing duplicate pose/overlay calls at 250 Hz.
        if notify
            && (job.as_ref().is_some_and(|j| j.committed) || cancel_until.is_some())
            && Instant::now() >= next_badge_tick
        {
            vr.badge_tick(badge_hand);
            next_badge_tick = Instant::now() + cadence::BADGE_INTERVAL;
        }
        // Thumbstick click cancels a committed dictation before it can upload.
        if snapshot.state[4] != 0 && previous[4] == 0 && job.as_ref().is_some_and(|j| j.committed) {
            warming_at = None;
            retire(&mut job, &mut retired);
            if notify {
                let _ = vr.badge(BADGE_CANCELED, badge_hand);
            }
            cancel_until = Some(Instant::now() + Duration::from_millis(CANCEL_NOTICE_MS));
            audio::beep(&c, 320.);
            if debug {
                eprintln!("gesture: cancel");
            }
        }
        // After a successful dictation, submit/clear are armed. They are driven
        // by A/B and by the left D-pad (down = A, left = X→clear), so either
        // hand can finish without reaching across.
        if armed(actions_armed_until, Instant::now()) {
            for (index, output) in [(2, Output::Submit), (3, Output::Clear)] {
                if snapshot.state[index] != 0 && previous[index] == 0 {
                    actions_armed_until = None;
                    if let Some(j) = &job {
                        j.control.cancel();
                    }
                    let _ = vr.badge(BADGE_HIDDEN, -1);
                    let now = context.snapshot();
                    if now.revision == focus.revision && now.focus.is_some() {
                        if let Err(e) =
                            inject::send_guarded(&c, &output, now.focus.as_ref(), || {
                                context.revision() == now.revision
                            })
                        {
                            eprintln!("controller action: {e}");
                        }
                    }
                    break;
                }
            }
        } else {
            actions_armed_until = None;
        }
        // Process fresh action availability and A/B cancellation before accepting
        // an ASR result that may have completed during the same poll interval.
        if let Some(result) = job.as_ref().and_then(|j| j.worker.try_recv()) {
            warming_at = None;
            processing_at = None;
            processing_deadline = None;
            if let Some(mut j) = job.take() {
                j.join();
                // The transient off-mic (from an explicit cancel) stays up; do
                // not overwrite it with another state until it retires.
                let cancel_showing = cancel_until.is_some();
                // Injection blocks this thread, which would freeze the badge in
                // place, so end the processing icon before delivery begins.
                if notify && !cancel_showing {
                    let _ = vr.badge(BADGE_HIDDEN, -1);
                }
                let show_canceled = |vr: &mut OpenVr, cancel_until: &mut Option<Instant>| {
                    if notify && cancel_until.is_none() {
                        let _ = vr.badge(BADGE_CANCELED, badge_hand);
                        *cancel_until =
                            Some(Instant::now() + Duration::from_millis(CANCEL_NOTICE_MS));
                    }
                };
                if eligible(&j.focus, &context.snapshot(), &j.control) {
                    match result {
                        Ok(output) => {
                            match inject::send_guarded(&c, &output, j.focus.focus.as_ref(), || {
                                !j.control.canceled() && context.matches(&j.focus)
                            }) {
                                Ok(()) => {
                                    // No success tone: the injected text is its own
                                    // confirmation. Only start/stop/error tones remain.
                                    if matches!(output, Output::Text(_)) {
                                        actions_armed_until = Some(
                                            Instant::now() + Duration::from_millis(action_window),
                                        );
                                    }
                                    if matches!(output, Output::Cancel) {
                                        // Silence or a canceled recording: nothing sent.
                                        if !cancel_showing {
                                            audio::beep(&c, 320.);
                                        }
                                        show_canceled(&mut vr, &mut cancel_until);
                                    } else if !cancel_showing && notify {
                                        let _ = vr.badge(BADGE_HIDDEN, -1);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("injection: {e}");
                                    show_canceled(&mut vr, &mut cancel_until);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("dictation: {e}");
                            audio::beep(&c, 320.);
                            show_canceled(&mut vr, &mut cancel_until);
                        }
                    }
                } else if !cancel_showing && notify {
                    let _ = vr.badge(BADGE_HIDDEN, -1);
                }
            }
        }
        previous = snapshot.state;
        inject::reap_helpers();
        thread::sleep(cadence::input_interval(
            gesture.engaged(),
            snapshot.active[0] != 0 || snapshot.active[1] != 0,
        ));
    }
    for j in &retired {
        j.control.cancel();
    }
    drop(job);
    drop(retired);
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    // Installer capability check works before first-run credentials/config exist.
    if args == ["--build-info"] {
        println!(
            "frame-voice {}; OpenVR compiled={}",
            env!("CARGO_PKG_VERSION"),
            cfg!(feature = "openvr")
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("frame-voice [--no-beep] [--no-cleanup]\nframe-voice --text TEXT [--dry-run]\nframe-voice --record-only PATH\nframe-voice --check\n\nDefault: Linux OpenVR daemon. --text is literal (never spoken commands).\nConfig: ~/.config/frame-voice/env; see README.md for build and deployment.");
        return Ok(());
    }
    let mut c = Config::load()?;
    let mut text = None;
    let mut record = None;
    let mut check = false;
    let mut dry = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--text" | "--record-only" => {
                let flag = &args[i];
                i += 1;
                let value = args.get(i).context("flag requires a value")?.clone();
                if flag == "--text" {
                    text = Some(value);
                } else {
                    record = Some(PathBuf::from(value));
                }
            }
            "--check" => check = true,
            "--dry-run" => dry = true,
            "--no-beep" => c.beep = false,
            "--no-cleanup" => c.cleanup = false,
            flag => bail!("unknown argument {flag}; use --help"),
        }
        i += 1;
    }
    if usize::from(text.is_some()) + usize::from(record.is_some()) + usize::from(check) > 1 {
        bail!("choose one of --text, --record-only, --check");
    }
    if dry && text.is_none() {
        bail!("--dry-run requires --text");
    }
    if check {
        println!(
            "configuration valid; backend={}; OpenVR compiled={}",
            c.get("VOICE_BACKEND", "groq"),
            cfg!(feature = "openvr")
        );
        if c.get("VOICE_BACKEND", "groq") == "groq" && c.get("GROQ_API_KEY", "").is_empty() {
            bail!("GROQ_API_KEY missing");
        }
        return Ok(());
    }
    if let Some(text) = text {
        if dry {
            println!("{text}");
            return Ok(());
        }
        return inject::send(&c, &Output::Text(text), None);
    }
    signals();
    if let Some(path) = record {
        let _lock = lock(&c)?;
        let control = Control::default();
        let worker_control = control.clone();
        let worker = thread::spawn(move || pipeline::run(&c, &worker_control, Some(&path)));
        while !worker.is_finished() {
            if SHUTDOWN.load(Ordering::SeqCst) {
                control.cancel();
            }
            thread::sleep(Duration::from_millis(50));
        }
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("recording worker panicked"))??;
        return Ok(());
    }
    daemon(c)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn either_controller_works_and_only_the_owning_hand_can_invalidate_it() {
        let left = [1, 0, 1, 1, 1];
        let right = [0, 1, 1, 1, 1];
        let both = [1, 1, 1, 1, 1];
        assert!(touch_available(&left));
        assert!(touch_available(&right));
        assert!(!touch_available(&[0, 0, 1, 1, 1]));
        assert!(!input_invalidated(&left, &both, Some(0)));
        assert!(!input_invalidated(&both, &left, Some(0)));
        assert!(!input_invalidated(&both, &right, Some(1)));
        assert!(input_invalidated(&both, &left, Some(1)));
        assert!(input_invalidated(&left, &[0; 5], Some(0)));
    }
    #[test]
    fn canceled_or_changed_focus_never_injects() {
        let focus = Snapshot {
            revision: 1,
            focus: Some(frame_voice::context::Focus {
                display: ":0".into(),
                window: 1,
                class: "konsole".into(),
            }),
        };
        let control = Control::default();
        assert!(eligible(&focus, &focus, &control));
        let mut changed = focus.clone();
        changed.revision = 3;
        assert!(!eligible(&focus, &changed, &control));
        control.cancel();
        assert!(!eligible(&focus, &focus, &control));
        assert!(!eligible(
            &Snapshot::default(),
            &Snapshot::default(),
            &Control::default()
        ));
    }
    #[test]
    fn cancel_retires_worker_without_waiting_for_its_request() {
        let (release, gate) = std::sync::mpsc::channel();
        let control = Control::default();
        let mut job = Some(Job {
            feedback: None,
            control: control.clone(),
            focus: Snapshot::default(),
            worker: Worker::spawn(move || {
                gate.recv().unwrap();
                Ok(Output::Text("stale".into()))
            }),
            committed: true,
            beeped: true,
        });
        let mut retired = Vec::new();
        retire(&mut job, &mut retired);
        assert!(job.is_none());
        assert!(control.canceled());
        assert!(!retired[0].worker.is_finished());
        release.send(()).unwrap();
        drop(retired);
    }
    #[test]
    fn recording_cue_waits_for_live_committed_capture_and_only_fires_once() {
        let mut job = Job {
            feedback: None,
            control: Control::default(),
            focus: Snapshot::default(),
            worker: Worker::spawn(|| Ok(Output::Cancel)),
            committed: false,
            beeped: false,
        };
        assert!(!ready_cue(&job));
        job.control.live.store(true, Ordering::SeqCst);
        assert!(!ready_cue(&job));
        job.committed = true;
        assert!(ready_cue(&job));
        job.beeped = true;
        assert!(!ready_cue(&job));
        job.beeped = false;
        job.control.stop();
        assert!(!ready_cue(&job));
        job.control.stop.store(false, Ordering::SeqCst);
        job.control.cancel();
        assert!(!ready_cue(&job));
    }
    #[test]
    fn cold_start_does_not_consume_the_activation_animation() {
        let held = Instant::now();
        let mut animation = held;
        let cold = held + Duration::from_millis(400);
        assert_eq!(
            activation_elapsed(&mut animation, cold, false),
            Duration::ZERO
        );
        // Readiness alone must not jump a cold capture to a completed badge.
        assert_eq!(
            activation_elapsed(&mut animation, cold, true),
            Duration::ZERO
        );
        assert_eq!(
            activation_elapsed(&mut animation, cold + Duration::from_millis(150), true),
            Duration::from_millis(150)
        );
        // An already-warm capture keeps the existing quick activation timing.
        let mut warm = held;
        assert_eq!(
            activation_elapsed(&mut warm, held + Duration::from_millis(150), true),
            Duration::from_millis(150)
        );
    }
    #[test]
    fn action_window_expires_and_defaults_closed() {
        let now = Instant::now();
        assert!(!armed(None, now));
        assert!(armed(Some(now + Duration::from_secs(1)), now));
        assert!(!armed(Some(now - Duration::from_secs(1)), now));
    }
}
