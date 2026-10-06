// The SDK owns all C++ ABI details. Rust receives only a fixed-width snapshot.
#include <openvr.h>
#include "input_poll.h"
#include <dlfcn.h>
#include <cmath>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <unistd.h>
static void* library = nullptr;
static vr::IVRSystem* vr_system = nullptr;
static vr::IVRInput* input = nullptr;
static vr::IVROverlay* overlay = nullptr;
static vr::IVRNotifications* notifications = nullptr;
static vr::IVRRenderModels* render_models = nullptr;
static vr::VROverlayHandle_t notify_overlay = 0;
static vr::VROverlayHandle_t recording_overlay = 0;  // static white corner
static vr::VROverlayHandle_t action_overlay = 0;     // mic / spinning gear / mic-off
static bool recording_visible = false;
static int badge_state = 0;  // 0 hidden, 1 recording, 2 processing, 3 canceled
static float badge_progress = 1.0f;
static std::chrono::steady_clock::time_point processing_started;
static void (*shutdown_vr)() = nullptr;
static vr::VRActionSetHandle_t action_set = 0;
static vr::VRActionHandle_t actions[5] = {};
static const char* const action_names[5] = {
    "/actions/voicedict/in/touch_left", "/actions/voicedict/in/touch_right",
    "/actions/voicedict/in/submit", "/actions/voicedict/in/clear",
    "/actions/voicedict/in/cancel"};
// Manifest path, retained so the manifest can be re-set if SteamVR accepted it
// before its bindings were ready (which leaves the action set inactive).
static char manifest_path[512] = {0};
// Input origins for the two thumbstick touches, so we can locate the joystick
// component on the controller.
static vr::VRInputValueHandle_t touch_origin[2] = {};
// Last known joystick component transform per hand, kept so the badge stays on
// the joystick after release (when the touch origin is no longer active).
static vr::HmdMatrix34_t badge_component[2] = {};
static bool badge_component_valid[2] = {};

#include "badge_icons.h"
using namespace frame_voice_icons;
struct BadgeIcons {
    static const int W = 64, H = 64;
    unsigned char white_mic[W * H * 4];    // notifications
    unsigned char frame_corner[W * H * 4]; // static white corner
    unsigned char mic[W * H * 4];          // recording action
    unsigned char gear[W * H * 4];         // processing action; rotated by transform
    unsigned char micoff[W * H * 4];       // canceled action
    vr::NotificationBitmap_t white_bmp;
    BadgeIcons() {
        build_icon(white_mic, mic_shape, 255, 255, 255);
        build_icon(frame_corner, frame_only_shape, 255, 255, 255);
        build_icon(mic, mic_only_shape, 26, 159, 255);
        build_icon(gear, gear_only_shape, 26, 159, 255);
        build_icon(micoff, micoff_only_shape, 240, 90, 90);
        white_bmp = vr::NotificationBitmap_t();
        white_bmp.m_pImageData = white_mic;
        white_bmp.m_nWidth = W;
        white_bmp.m_nHeight = H;
        white_bmp.m_nBytesPerPixel = 4;
    }
};
static BadgeIcons* icons = nullptr;
extern "C" void fv_close() {
    if (overlay && recording_overlay) { overlay->DestroyOverlay(recording_overlay); }
    if (overlay && action_overlay) { overlay->DestroyOverlay(action_overlay); }
    recording_overlay = 0;
    action_overlay = 0;
    recording_visible = false;
    badge_state = 0;
    if (overlay && notify_overlay) { overlay->DestroyOverlay(notify_overlay); notify_overlay = 0; }
    overlay = nullptr;
    notifications = nullptr;
    render_models = nullptr;
    input = nullptr;
    vr_system = nullptr;
    if (shutdown_vr) { shutdown_vr(); shutdown_vr = nullptr; }
    if (library) { dlclose(library); library = nullptr; }
}
// Queue a transient SteamVR notification. Returns 0 on success, negative if the
// notification subsystem is unavailable, or the EVRNotificationError code.
extern "C" int fv_notify(const char* text) {
    if (!notifications || notify_overlay == 0 || !text) return -1;
    vr::VRNotificationId id = 0;
    auto e = notifications->CreateNotification(
        notify_overlay, 0, vr::EVRNotificationType_Transient, text,
        vr::EVRNotificationStyle_Application, icons ? &icons->white_bmp : nullptr, &id);
    return e == vr::VRNotificationError_OK ? 0 : static_cast<int>(e);
}
// Position of the thumbstick component in the controller's local frame, so the
// badge can sit on the joystick rather than the handle origin.
static bool component_matrix(vr::TrackedDeviceIndex_t device, vr::VRInputValueHandle_t origin,
                             vr::HmdMatrix34_t* out) {
    if (!render_models || !vr_system || origin == vr::k_ulInvalidInputValueHandle) return false;
    vr::InputOriginInfo_t info = {};
    if (input->GetOriginTrackedDeviceInfo(origin, &info, sizeof(info)) != vr::VRInputError_None) return false;
    if (info.trackedDeviceIndex != device) return false;
    char model[128] = {0};
    vr::ETrackedPropertyError perr = vr::TrackedProp_Success;
    vr_system->GetStringTrackedDeviceProperty(device, vr::Prop_RenderModelName_String, model,
                                              sizeof(model), &perr);
    if (perr != vr::TrackedProp_Success || model[0] == 0) return false;
    vr::RenderModel_ComponentState_t comp = {};
    if (!render_models->GetComponentStateForDevicePath(model, info.rchRenderModelComponentName,
                                                       info.devicePath, nullptr, &comp)) {
        return false;
    }
    *out = comp.mTrackingToComponentLocal;
    return true;
}
static void normalize3(float v[3]) {
    float n = std::sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if (n > 1e-6f) { v[0] /= n; v[1] /= n; v[2] /= n; }
}
static void cross3(const float a[3], const float b[3], float out[3]) {
    out[0] = a[1] * b[2] - a[2] * b[1];
    out[1] = a[2] * b[0] - a[0] * b[2];
    out[2] = a[0] * b[1] - a[1] * b[0];
}
// Set an overlay's width and tracked-device-relative transform in one place.
static int apply_badge(vr::VROverlayHandle_t handle, float width,
                       vr::TrackedDeviceIndex_t device, const vr::HmdMatrix34_t& placement) {
    auto e = overlay->SetOverlayWidthInMeters(handle, width);
    if (e == vr::VROverlayError_None)
        e = overlay->SetOverlayTransformTrackedDeviceRelative(handle, device, &placement);
    if (e != vr::VROverlayError_None)
        fprintf(stderr, "shim: overlay transform dev=%u failed %d\n", (unsigned)device, (int)e);
    return static_cast<int>(e);
}
static int apply_action_badge(float width, vr::TrackedDeviceIndex_t device,
                              const vr::HmdMatrix34_t& placement) {
    auto action_placement = placement;
    if (badge_state == 2) {
        const double elapsed = std::chrono::duration<double>(
            std::chrono::steady_clock::now() - processing_started).count();
        rotate_action_axes(action_placement.m, gear_spin_angle(elapsed));
    }
    return apply_badge(action_overlay, width, device, action_placement);
}
// Place both badge overlays. They share one position at the joystick component,
// so the compositor moves them together; the corner keeps its size while the
// action width is the animated value. `progress` scales the action (1 = full).
static int place_badge(int hand, float progress) {
    vr::HmdMatrix34_t placement = {};
    float width = 0.03f;
    if (vr_system && (hand == 0 || hand == 1)) {
        auto role = hand == 0 ? vr::TrackedControllerRole_LeftHand
                              : vr::TrackedControllerRole_RightHand;
        auto device = vr_system->GetTrackedDeviceIndexForControllerRole(role);
        if (device != vr::k_unTrackedDeviceIndexInvalid) {
            vr::HmdMatrix34_t comp = {};
            if (component_matrix(device, touch_origin[hand], &comp)) {
                badge_component[hand] = comp;
                badge_component_valid[hand] = true;
            }
            vr::TrackedDevicePose_t poses[vr::k_unMaxTrackedDeviceCount];
            vr_system->GetDeviceToAbsoluteTrackingPose(
                vr::TrackingUniverseStanding, 0.0f, poses, vr::k_unMaxTrackedDeviceCount);
            const auto& head = poses[vr::k_unTrackedDeviceIndex_Hmd];
            const auto& hand_pose = poses[device];
            if (head.bPoseIsValid && hand_pose.bPoseIsValid) {
                const auto& c = hand_pose.mDeviceToAbsoluteTracking;
                const auto& h = head.mDeviceToAbsoluteTracking;
                float lc[3] = {0, 0, 0};
                if (badge_component_valid[hand]) {
                    lc[0] = badge_component[hand].m[0][3];
                    lc[1] = badge_component[hand].m[1][3];
                    lc[2] = badge_component[hand].m[2][3];
                }
                // Component position in absolute space.
                float pc[3] = {
                    c.m[0][0] * lc[0] + c.m[0][1] * lc[1] + c.m[0][2] * lc[2] + c.m[0][3],
                    c.m[1][0] * lc[0] + c.m[1][1] * lc[1] + c.m[1][2] * lc[2] + c.m[1][3],
                    c.m[2][0] * lc[0] + c.m[2][1] * lc[1] + c.m[2][2] * lc[2] + c.m[2][3],
                };
                // Face the HMD: overlay +Z points from the badge toward the eye.
                float d[3] = {h.m[0][3] - pc[0], h.m[1][3] - pc[1], h.m[2][3] - pc[2]};
                normalize3(d);
                // Direction and world up in the controller's frame (R_c^T * v).
                float dc[3] = {
                    c.m[0][0] * d[0] + c.m[1][0] * d[1] + c.m[2][0] * d[2],
                    c.m[0][1] * d[0] + c.m[1][1] * d[1] + c.m[2][1] * d[2],
                    c.m[0][2] * d[0] + c.m[1][2] * d[1] + c.m[2][2] * d[2],
                };
                normalize3(dc);
                float up[3] = {c.m[1][0], c.m[1][1], c.m[1][2]};
                normalize3(up);
                float xc[3];
                cross3(up, dc, xc);
                if (std::sqrt(xc[0] * xc[0] + xc[1] * xc[1] + xc[2] * xc[2]) < 1e-4f) {
                    xc[0] = 1.0f;
                    xc[1] = 0.0f;
                    xc[2] = 0.0f;
                    cross3(up, dc, xc);
                }
                normalize3(xc);
                float yc[3];
                cross3(dc, xc, yc);
                // Float toward the eye along the view direction. Scale the lift
                // by how far the joystick points away from the eye: tight when
                // head-on, pushed out when edge-on (the controller body would
                // otherwise clip it). Overlays depth-test against the scene, so
                // distance is the only lever.
                float normal[3] = {
                    -badge_component[hand].m[0][2],
                    -badge_component[hand].m[1][2],
                    -badge_component[hand].m[2][2],
                };
                float cosang = normal[0] * dc[0] + normal[1] * dc[1] + normal[2] * dc[2];
                if (cosang < 0.12f) cosang = 0.12f;
                float lift = 0.012f / cosang;
                if (lift > 0.07f) lift = 0.07f;
                placement.m[0][0] = xc[0];
                placement.m[1][0] = xc[1];
                placement.m[2][0] = xc[2];
                placement.m[0][1] = yc[0];
                placement.m[1][1] = yc[1];
                placement.m[2][1] = yc[2];
                placement.m[0][2] = dc[0];
                placement.m[1][2] = dc[1];
                placement.m[2][2] = dc[2];
                placement.m[0][3] = lc[0] + dc[0] * lift;
                placement.m[1][3] = lc[1] + dc[1] * lift;
                placement.m[2][3] = lc[2] + dc[2] * lift;
                int e = apply_badge(recording_overlay, width, device, placement);
                if (e == 0)
                    e = apply_action_badge(width * progress, device, placement);
                if (e == 0) return 0;
                // A controller can be momentarily untracked; fall through to the
                // head-relative placement rather than leaving the badge hidden.
            }
        }
    }
    // Fallback: head-relative, bottom-center of view.
    placement.m[0][0] = placement.m[1][1] = placement.m[2][2] = 1.0f;
    placement.m[0][3] = 0.0f;
    placement.m[1][3] = -0.25f;
    placement.m[2][3] = -0.35f;
    width = 0.025f;
    int e = apply_badge(recording_overlay, width, vr::k_unTrackedDeviceIndex_Hmd, placement);
    if (e == 0)
        e = apply_action_badge(width * progress, vr::k_unTrackedDeviceIndex_Hmd, placement);
    return e;
}
// Create and configure one badge overlay. The recording badge is a static
// corner overlay plus an action overlay so the action can be scaled separately.
static int create_badge_overlay(vr::VROverlayHandle_t* handle, const char* suffix, int sort) {
    char key[64];
    snprintf(key, sizeof(key), "frame-voice.%s.%ld", suffix, (long)getpid());
    auto e = overlay->CreateOverlay(key, "Frame Voice", handle);
    if (e != vr::VROverlayError_None) return static_cast<int>(e);
    // Never take keyboard or pointer focus. Stop at the first error and destroy
    // the incomplete overlay so the next use may retry.
    if (e == vr::VROverlayError_None)
        e = overlay->SetOverlayInputMethod(*handle, vr::VROverlayInputMethod_None);
    if (e == vr::VROverlayError_None)
        e = overlay->SetOverlayFlag(*handle, vr::VROverlayFlags_VisibleInDashboard, true);
    if (e == vr::VROverlayError_None)
        e = overlay->SetOverlaySortOrder(*handle, sort);
    if (e != vr::VROverlayError_None) {
        overlay->DestroyOverlay(*handle);
        *handle = 0;
    }
    return static_cast<int>(e);
}
static int set_badge_texture(vr::VROverlayHandle_t handle, const unsigned char* pixels) {
    auto e = overlay->SetOverlayRaw(handle, const_cast<unsigned char*>(pixels),
                                    BadgeIcons::W, BadgeIcons::H, 4);
    if (e != vr::VROverlayError_None)
        fprintf(stderr, "shim: SetOverlayRaw failed %d\n", (int)e);
    return static_cast<int>(e);
}
// Show the badge in a given state: 0 hidden, 1 recording (blue mic),
// 2 processing (spinning blue gear), 3 canceled/no speech (red mic-off). `hand` is
// 0 = left, 1 = right, else headset; `progress` scales the action 0..1.
static int show_badge(int state, int hand, float progress) {
    if (state == 0) {
        if (!recording_visible && badge_state == 0) return 0;
        badge_state = 0;
        if (overlay && recording_overlay) overlay->HideOverlay(recording_overlay);
        if (overlay && action_overlay) overlay->HideOverlay(action_overlay);
        recording_visible = false;
        return 0;
    }
    if (!overlay || !icons) return -1;
    if (!recording_overlay) {
        int e = create_badge_overlay(&recording_overlay, "badge", 100);
        if (e == 0) e = create_badge_overlay(&action_overlay, "action", 101);
        // The corner never changes, so upload it once here.
        if (e == 0) e = set_badge_texture(recording_overlay, icons->frame_corner);
        if (e != 0) return e;
    }
    if (state != badge_state) {
        const unsigned char* pixels = state == 1 ? icons->mic
                                    : state == 2 ? icons->gear
                                                 : icons->micoff;
        int e = set_badge_texture(action_overlay, pixels);
        if (e != 0) return e;
        if (state == 2) processing_started = std::chrono::steady_clock::now();
        badge_state = state;
    }
    badge_progress = progress;
    int e = place_badge(hand, progress);
    if (e != 0) return e;
    if (!recording_visible) {
        e = overlay->ShowOverlay(recording_overlay);
        if (e == vr::VROverlayError_None) e = overlay->ShowOverlay(action_overlay);
        if (e != vr::VROverlayError_None) {
            fprintf(stderr, "shim: ShowOverlay %d\n", (int)e);
            return static_cast<int>(e);
        }
        recording_visible = true;
    }
    return 0;
}
extern "C" int fv_badge(int state, int hand) {
    return show_badge(state, hand, 1.0f);
}
extern "C" int fv_badge_progress(float progress, int hand) {
    if (!std::isfinite(progress) || progress < 0 || progress > 1) return -5;
    return show_badge(1, hand, progress);
}
// Re-billboard the visible badge toward the current hand position.
extern "C" int fv_badge_tick(int hand) {
    if (!recording_visible || !overlay || !recording_overlay) return 0;
    return place_badge(hand, badge_progress);
}
// Re-set the action manifest. SteamVR can accept the manifest before its
// controller bindings load (logged as "Failed to load binding file"), leaving
// every action inactive forever; re-setting nudges it to reload. Safe to repeat.
extern "C" int fv_rebind() {
    if (!input || manifest_path[0] == 0) return -1;
    auto e = input->SetActionManifestPath(manifest_path);
    if (e != vr::VRInputError_None) return static_cast<int>(e);
    e = input->GetActionSetHandle("/actions/voicedict", &action_set);
    if (e != vr::VRInputError_None) return static_cast<int>(e);
    for (int i = 0; i < 5; ++i) {
        e = input->GetActionHandle(action_names[i], &actions[i]);
        if (e != vr::VRInputError_None) return static_cast<int>(e);
    }
    return 0;
}
extern "C" int fv_open(const char* path, const char* manifest) {
    library = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (!library) { fprintf(stderr, "OpenVR loader: %s\n", dlerror()); return -1; }
    auto init = reinterpret_cast<uint32_t(*)(vr::EVRInitError*, vr::EVRApplicationType)>(dlsym(library, "VR_InitInternal"));
    auto get = reinterpret_cast<void*(*)(const char*, vr::EVRInitError*)>(dlsym(library, "VR_GetGenericInterface"));
    auto stop = reinterpret_cast<void(*)()>(dlsym(library, "VR_ShutdownInternal"));
    if (!init || !get || !stop) { fv_close(); return -2; }
    vr::EVRInitError error = vr::VRInitError_None;
    init(&error, vr::VRApplication_Background);
    if (error != vr::VRInitError_None) { fv_close(); return static_cast<int>(error); }
    shutdown_vr = stop;
    input = static_cast<vr::IVRInput*>(get(vr::IVRInput_Version, &error));
    if (!input || error != vr::VRInitError_None) { fv_close(); return -3; }
    snprintf(manifest_path, sizeof(manifest_path), "%s", manifest);
    auto e = input->SetActionManifestPath(manifest);
    if (e != vr::VRInputError_None) { fv_close(); return static_cast<int>(e); }
    e = input->GetActionSetHandle("/actions/voicedict", &action_set);
    if (e != vr::VRInputError_None) { fv_close(); return static_cast<int>(e); }
    for (int i = 0; i < 5; ++i) {
        e = input->GetActionHandle(action_names[i], &actions[i]);
        if (e != vr::VRInputError_None) { fv_close(); return static_cast<int>(e); }
    }
    if (!icons) icons = new BadgeIcons();
    // Notifications are best-effort: never fail daemon startup over them.
    vr_system = static_cast<vr::IVRSystem*>(get(vr::IVRSystem_Version, &error));
    if (!vr_system || error != vr::VRInitError_None) { vr_system = nullptr; }
    overlay = static_cast<vr::IVROverlay*>(get(vr::IVROverlay_Version, &error));
    if (!overlay || error != vr::VRInitError_None) { overlay = nullptr; }
    notifications = static_cast<vr::IVRNotifications*>(get(vr::IVRNotifications_Version, &error));
    if (!notifications || error != vr::VRInitError_None) { notifications = nullptr; }
    render_models = static_cast<vr::IVRRenderModels*>(get(vr::IVRRenderModels_Version, &error));
    if (!render_models || error != vr::VRInitError_None) { render_models = nullptr; }
    if (overlay && notifications) {
        // Unique key so a stale overlay from a previous run cannot collide.
        char key[64];
        snprintf(key, sizeof(key), "frame-voice.notify.%ld", (long)getpid());
        if (overlay->CreateOverlay(key, "Frame Voice", &notify_overlay) != vr::VROverlayError_None) {
            notify_overlay = 0;
        }
    }
    return 0;
}
extern "C" int fv_poll(uint8_t* active, uint8_t* state) {
    if (!input || !active || !state) return -4;
    return fv_poll_inputs(input, action_set, actions, active, state, touch_origin);
}
// Whether the HMD pose is currently valid. Used to tell "SteamVR bindings have
// not loaded" (HMD tracked, but no action active) apart from "the user is not
// wearing the headset / controllers are idle", so the daemon does not restart in
// a loop when nothing is happening.
extern "C" int fv_hmd_tracked() {
    if (!vr_system) return 0;
    vr::TrackedDevicePose_t poses[vr::k_unMaxTrackedDeviceCount];
    vr_system->GetDeviceToAbsoluteTrackingPose(
        vr::TrackingUniverseStanding, 0.0f, poses, vr::k_unMaxTrackedDeviceCount);
    return poses[vr::k_unTrackedDeviceIndex_Hmd].bPoseIsValid ? 1 : 0;
}
