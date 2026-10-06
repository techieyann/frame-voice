fn main() {
    println!("cargo:rerun-if-env-changed=FRAME_VOICE_BUILD_REVISION");
    println!("cargo:rerun-if-env-changed=FRAME_VOICE_BUILD_DIRTY");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let revision = std::env::var("FRAME_VOICE_BUILD_REVISION")
        .ok()
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .filter(|value| value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".into());
    let dirty = std::env::var("FRAME_VOICE_BUILD_DIRTY").unwrap_or_else(|_| {
        if git(&["status", "--porcelain"]).is_some_and(|status| !status.is_empty()) {
            "true"
        } else {
            "false"
        }
        .into()
    });
    println!("cargo:rustc-env=FRAME_VOICE_BUILD_REVISION={revision}");
    println!("cargo:rustc-env=FRAME_VOICE_BUILD_DIRTY={dirty}");
    for item in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", item]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git(&["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    println!("cargo:rerun-if-changed=shim/openvr_shim.cpp");
    println!("cargo:rerun-if-changed=shim/badge_icons.h");
    println!("cargo:rerun-if-changed=shim/input_poll.h");
    println!("cargo:rerun-if-env-changed=OPENVR_INCLUDE_DIR");
    if std::env::var_os("CARGO_FEATURE_OPENVR").is_none() {
        return;
    }
    assert_eq!(
        std::env::var("CARGO_CFG_TARGET_OS").unwrap(),
        "linux",
        "OpenVR daemon requires Linux"
    );
    assert_ne!(
        std::env::var("CARGO_CFG_TARGET_ENV").unwrap(),
        "musl",
        "SteamVR uses glibc: build a GNU target, not static musl"
    );
    let include = std::env::var("OPENVR_INCLUDE_DIR")
        .expect("set OPENVR_INCLUDE_DIR to the SteamVR SDK headers directory");
    println!("cargo:rerun-if-changed={include}/openvr.h");
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(include)
        .file("shim/openvr_shim.cpp")
        .compile("frame_openvr");
    println!("cargo:rustc-link-lib=dl");
}
