fn main() {
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
