//! Exercise process boundaries without SteamVR, microphone, network, or real typing.
use frame_voice::{
    audio::{self, Control},
    config::Config,
    inject, pipeline,
    speech::Output,
};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    time::{Duration, Instant},
};
fn script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn config(home: &Path) -> Config {
    Config {
        home: home.into(),
        values: BTreeMap::new(),
        max_seconds: 5.0,
        threshold: 300.,
        cleanup: false,
        beep: false,
        paste_chunk: 0,
        min_speech_ms: 200,
    }
}
#[test]
fn capture_local_asr_and_literal_injection() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    let capture = dir.path().join("capture");
    let whisper = dir.path().join("whisper");
    let injector = dir.path().join("inject");
    let fixture = dir.path().join("speech.raw");
    fs::write(&fixture, include_bytes!("fixtures/webrtc-speech-8k.raw")).unwrap();
    script(&capture, &format!("#!/usr/bin/env python3\nimport sys,struct,time\nb=open({:?},'rb').read()\nbase=struct.unpack('<'+'h'*(len(b)//2),b)\nvoice=[v for v in base for _ in range(2)]\nsamples=[0]*8000 + voice*6 + [0]*8000\nsys.stdout.buffer.write(struct.pack('<'+'h'*len(samples),*samples))\nsys.stdout.buffer.flush()\ntime.sleep(10)\n",fixture.to_str().unwrap()));
    script(&whisper, "#!/bin/sh\nprintf '%s' 'use foo(bar) and [x].'\n");
    // Arguments go directly to exec, including shell metacharacters and leading hyphens.
    script(
        &injector,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            dir.path().join("args").display()
        ),
    );
    c.values
        .insert("FFMPEG_BIN".into(), capture.to_str().unwrap().into());
    c.values
        .insert("WHISPER_BIN".into(), whisper.to_str().unwrap().into());
    c.values.insert("VOICE_BACKEND".into(), "local".into());
    c.values
        .insert("YDOTOOL_BIN".into(), injector.to_str().unwrap().into());
    let output = pipeline::run(&c, &Control::default(), None).unwrap();
    assert_eq!(output, Output::Text("use foo(bar) and [x]".into()));
    inject::send(&c, &Output::Text("--submit $(touch bad)".into()), None).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("args")).unwrap(),
        // A trailing space is appended so consecutive dictations don't run together.
        "type\n-d\n2\n-H\n2\n--\n--submit $(touch bad) \n"
    );
    script(&whisper, "#!/bin/sh\nprintf 'submit!'\n");
    assert_eq!(
        pipeline::run(&c, &Control::default(), None).unwrap(),
        Output::Submit
    );
}
#[test]
fn bird_like_noise_never_starts_transcription_or_processing() {
    let mut pcm = vec![0; 8000];
    pcm.extend((0..8000).map(|i| {
        let t = i as f64 / 16000.;
        (2000. * (std::f64::consts::TAU * (3500. * t + 900. * t * t)).sin()) as i16
    }));
    pcm.extend(vec![0; 8000]);
    if frame_voice::vad::contains_speech(&pcm, 300., 200).is_none() {
        return; // Portable fallback is covered separately; native SteamOS runs this guard.
    }
    assert!(audio::trim(&pcm, 300., 200).is_some());
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    c.max_seconds = 1.5;
    let raw = dir.path().join("noise.raw");
    fs::write(
        &raw,
        pcm.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>(),
    )
    .unwrap();
    let capture = dir.path().join("capture");
    script(&capture, &format!("#!/usr/bin/env python3\nimport sys,time\nsys.stdout.buffer.write(open({:?},'rb').read())\nsys.stdout.buffer.flush()\ntime.sleep(10)\n", raw.to_str().unwrap()));
    let asr = dir.path().join("asr");
    let called = dir.path().join("asr-called");
    script(
        &asr,
        &format!("#!/bin/sh\ntouch '{}'\nprintf 'Yeah'\n", called.display()),
    );
    c.values
        .insert("FFMPEG_BIN".into(), capture.to_str().unwrap().into());
    c.values
        .insert("WHISPER_BIN".into(), asr.to_str().unwrap().into());
    c.values.insert("VOICE_BACKEND".into(), "local".into());
    let control = Control::default();
    assert_eq!(pipeline::run(&c, &control, None).unwrap(), Output::Cancel);
    assert!(!control.voiced.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!called.exists());
}
#[test]
fn stalled_capture_can_be_canceled_and_child_is_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    c.max_seconds = 30.;
    let capture = dir.path().join("capture");
    let pidfile = dir.path().join("pid");
    script(
        &capture,
        &format!(
            "#!/bin/sh\necho $$ > '{}'\nexec sleep 30\n",
            pidfile.display()
        ),
    );
    c.values
        .insert("FFMPEG_BIN".into(), capture.to_str().unwrap().into());
    let control = Control::default();
    let worker_control = control.clone();
    let worker = std::thread::spawn(move || audio::record(&c, &worker_control));
    let start = Instant::now();
    while !pidfile.exists() {
        assert!(start.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(10));
    }
    let pid: i32 = fs::read_to_string(pidfile).unwrap().trim().parse().unwrap();
    control.cancel();
    let canceled = Instant::now();
    assert!(worker.join().unwrap().unwrap().is_empty());
    assert!(canceled.elapsed() < Duration::from_secs(1));
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}
#[test]
fn canceled_before_start_never_launches_capture() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    c.values
        .insert("FFMPEG_BIN".into(), "/does/not/exist".into());
    let control = Control::default();
    control.cancel();
    assert_eq!(pipeline::run(&c, &control, None).unwrap(), Output::Cancel);
}
#[test]
fn microphone_gain_is_restored_after_capture_opens_before_the_live_cue() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    c.max_seconds = 2.;
    let capture = dir.path().join("capture");
    script(&capture, "#!/usr/bin/env python3\nimport sys,struct,time\nsys.stdout.buffer.write(struct.pack('<h',1000)*40000)\nsys.stdout.buffer.flush()\ntime.sleep(3)\n");
    let gain = dir.path().join(".local/share/frame-voice/mic-gain.sh");
    fs::create_dir_all(gain.parent().unwrap()).unwrap();
    let marker = dir.path().join("gain-ready");
    script(
        &gain,
        &format!(
            "#!/bin/sh\n[ \"$1\" = --once ] || exit 1\nprintf ready > '{}'\n",
            marker.display()
        ),
    );
    c.values
        .insert("FFMPEG_BIN".into(), capture.to_str().unwrap().into());
    let control = Control::default();
    assert!(!audio::record(&c, &control).unwrap().is_empty());
    assert!(marker.exists());
    assert!(control.live.load(std::sync::atomic::Ordering::SeqCst));
    script(&gain, "#!/bin/sh\nexit 1\n");
    let control = Control::default();
    assert!(audio::record(&c, &control)
        .unwrap_err()
        .to_string()
        .contains("microphone gain setup failed"));
    assert!(!control.live.load(std::sync::atomic::Ordering::SeqCst));
}
#[test]
fn unicode_text_uses_clipboard_and_refuses_classless_typing() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    let clipboard = dir.path().join("clipboard");
    let payload = dir.path().join("payload");
    script(&clipboard, &format!("#!/usr/bin/env python3\nimport sys,time\nopen({:?},'wb').write(sys.stdin.buffer.read())\nsys.stdout.write('ready\\n')\nsys.stdout.flush()\ntime.sleep(10)\n", payload.to_str().unwrap()));
    let injector = dir.path().join("injector");
    let arguments = dir.path().join("arguments");
    script(
        &injector,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            arguments.display()
        ),
    );
    c.values
        .insert("CLIPBOARD_BIN".into(), clipboard.to_str().unwrap().into());
    c.values
        .insert("YDOTOOL_BIN".into(), injector.to_str().unwrap().into());
    let mut focus = frame_voice::context::Focus {
        display: ":0".into(),
        window: 10,
        class: "firefox".into(),
    };
    inject::send(&c, &Output::Text("こんにちは".into()), Some(&focus)).unwrap();
    assert_eq!(fs::read_to_string(payload).unwrap(), "こんにちは ");
    assert!(fs::read_to_string(&arguments).unwrap().starts_with("key\n"));
    fs::remove_file(&arguments).unwrap();
    focus.class.clear();
    assert!(inject::send(&c, &Output::Text("café".into()), Some(&focus)).is_err());
    assert!(!arguments.exists());
}
#[test]
fn desktop_clear_counts_the_appended_separator() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    let injector = dir.path().join("injector");
    let arguments = dir.path().join("arguments");
    script(
        &injector,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            arguments.display()
        ),
    );
    c.values
        .insert("YDOTOOL_BIN".into(), injector.to_str().unwrap().into());
    let focus = frame_voice::context::Focus {
        display: ":0".into(),
        window: 10,
        class: String::new(),
    };
    inject::send(&c, &Output::Text("hello".into()), Some(&focus)).unwrap();
    assert!(fs::read_to_string(&arguments)
        .unwrap()
        .ends_with("hello \n"));
    inject::send(&c, &Output::Clear, Some(&focus)).unwrap();
    let keys = fs::read_to_string(arguments).unwrap();
    assert_eq!(keys.lines().filter(|&line| line == "14:1").count(), 6);
    assert_eq!(keys.lines().filter(|&line| line == "14:0").count(), 6);
}
#[test]
fn capture_and_injection_failures_propagate() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    let fail = dir.path().join("fail");
    script(&fail, "#!/bin/sh\nexit 42\n");
    c.values
        .insert("FFMPEG_BIN".into(), fail.to_str().unwrap().into());
    c.values
        .insert("YDOTOOL_BIN".into(), fail.to_str().unwrap().into());
    assert!(audio::record(&c, &Control::default()).is_err());
    assert!(inject::send(&c, &Output::Submit, None).is_err());
}

#[test]
fn focus_loss_cancels_remaining_injection() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    let injector = dir.path().join("inject");
    let pidfile = dir.path().join("pid");
    script(
        &injector,
        &format!(
            "#!/bin/sh\necho $$ > '{}'\nexec sleep 30\n",
            pidfile.display()
        ),
    );
    c.values
        .insert("YDOTOOL_BIN".into(), injector.to_str().unwrap().into());
    let output = Output::Text("hello".into());
    assert!(inject::send_guarded(&c, &output, None, || false).is_err());
    assert!(!pidfile.exists());
    // Once the fake injector has started, simulate the target losing focus.
    let start = Instant::now();
    assert!(inject::send_guarded(&c, &output, None, || !pidfile.exists()).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    let pid: i32 = fs::read_to_string(pidfile).unwrap().trim().parse().unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}

#[test]
fn paste_helper_partial_readiness_cancels_and_reaps_on_focus_loss() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(dir.path());
    let helper = dir.path().join("clipboard");
    let pidfile = dir.path().join("pid");
    let displayfile = dir.path().join("display");
    script(&helper, &format!(
        "#!/usr/bin/env python3\nimport os, sys, time\nopen('{}','w').write(str(os.getpid()))\nopen('{}','w').write(os.environ.get('DISPLAY',''))\nsys.stdin.buffer.read()\nsys.stdout.write('re')\nsys.stdout.flush()\ntime.sleep(30)\n",
        pidfile.display(), displayfile.display()));
    c.values.insert("VOICE_INJECT".into(), "paste".into());
    c.values
        .insert("CLIPBOARD_BIN".into(), helper.to_str().unwrap().into());
    c.values
        .insert("YDOTOOL_BIN".into(), "/must/not/run".into());
    let focus = frame_voice::context::Focus {
        display: ":99".into(),
        window: 7,
        class: "Konsole".into(),
    };
    let start = Instant::now();
    assert!(
        inject::send_guarded(&c, &Output::Text("hello".into()), Some(&focus), || {
            !displayfile.exists()
        })
        .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    let pid: i32 = fs::read_to_string(pidfile).unwrap().parse().unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(fs::read_to_string(displayfile).unwrap(), ":99");
}
