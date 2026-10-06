#pragma once
#include <cmath>

// SDK-independent procedural icons; all outputs are straight-alpha RGBA.
namespace frame_voice_icons {
// Frame Voice vectors (assets/icons/*.svg), rasterized to 64x64 RGBA.
// All states share a white top-left frame corner with the action inside its square.
// Physical
// size and placement remain controlled by the existing overlay code.
[[maybe_unused]] static bool near_segment(double x, double y, double x0, double y0, double x1, double y1, double t) {
    double vx = x1 - x0, vy = y1 - y0, wx = x - x0, wy = y - y0;
    double c1 = vx * wx + vy * wy;
    if (c1 <= 0) return std::sqrt(wx * wx + wy * wy) <= t;
    double c2 = vx * vx + vy * vy;
    if (c2 <= c1) { double ex = x - x1, ey = y - y1; return std::sqrt(ex * ex + ey * ey) <= t; }
    double b = c1 / c2, px = x0 + b * vx, py = y0 + b * vy;
    double ex = x - px, ey = y - py;
    return std::sqrt(ex * ex + ey * ey) <= t;
}
// Shape coordinates use the vectors' 72x72 viewBox. Stroke caps are round.
[[maybe_unused]] static bool frame_shape(double x, double y) {
    return near_segment(x, y, 14, 58, 14, 14, 2) ||
           near_segment(x, y, 14, 14, 58, 14, 2);
}
// Inverse of the SVG's 1.45x scale and -45-degree rotation, placed at
// (40,40). The action fills the open square and points into the frame corner.
static void action_coordinates(double& x, double& y) {
    const double c = std::sqrt(0.5);
    double dx = (x - 40) / 1.45, dy = (y - 40) / 1.45;
    x = 43 + c * (dx - dy);
    y = 43 + c * (dx + dy);
}
[[maybe_unused]] static bool mic_glyph(double x, double y) {
    action_coordinates(x, y);
    double dy = y < 35 ? 35 - y : (y > 41 ? y - 41 : 0);
    if (std::abs(std::hypot(x - 43, dy) - 4) <= 1.4) return true;
    if (y >= 42 && std::abs(std::hypot(x - 43, y - 42) - 9) <= 1.4) return true;
    return near_segment(x, y, 34, 39, 34, 42, 1.4) ||
           near_segment(x, y, 52, 39, 52, 42, 1.4) ||
           near_segment(x, y, 43, 51, 43, 56, 1.4) ||
           near_segment(x, y, 39, 56, 47, 56, 1.4);
}
[[maybe_unused]] static bool near_quadratic(double x, double y, double x0, double y0,
                           double cx, double cy, double x1, double y1) {
    double prev_x = x0, prev_y = y0;
    for (int step = 1; step <= 32; ++step) {
        double t = step / 32.0, u = 1 - t;
        double next_x = u * u * x0 + 2 * u * t * cx + t * t * x1;
        double next_y = u * u * y0 + 2 * u * t * cy + t * t * y1;
        if (near_segment(x, y, prev_x, prev_y, next_x, next_y, 1.4)) return true;
        prev_x = next_x; prev_y = next_y;
    }
    return false;
}
[[maybe_unused]] static bool mic_shape(double x, double y) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    return frame_shape(x, y) || mic_glyph(x, y);
}
[[maybe_unused]] static bool wifi_shape(double x, double y) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    if (frame_shape(x, y)) return true;
    action_coordinates(x, y);
    return std::hypot(x - 43, y - 50) <= 1.8 ||
           near_quadratic(x, y, 31, 40, 43, 29, 55, 40) ||
           near_quadratic(x, y, 35, 45, 43, 38, 51, 45);
}
[[maybe_unused]] static bool micoff_shape(double x, double y) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    // Transparent clearance around the slash preserves legibility at small sizes.
    return frame_shape(x, y) || near_segment(x, y, 22.6, 57.4, 57.4, 22.6, 2.175) ||
           (mic_glyph(x, y) && !near_segment(x, y, 22.6, 57.4, 57.4, 22.6, 4.7125));
}
// Grow only the blue action about its placement center; the frame stays fixed.
[[maybe_unused]] static bool growing_mic_shape(double x, double y, double progress) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    if (frame_shape(x, y)) return true;
    if (progress <= 0) return false;
    return mic_glyph(40 + (x - 40) / progress, 40 + (y - 40) / progress);
}
// The badge is drawn as two overlays: a static white corner plus an action glyph
// whose overlay width the compositor animates. The drawing is recentered so the
// mic sits at the texture center, letting the action scale about its own center;
// the corner shifts by the same amount to keep their relative position.
static const double BADGE_RECENTER = 4.0;  // 72-space units (mic center 40 -> 36)
[[maybe_unused]] static bool frame_only_shape(double x, double y) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    return frame_shape(x + BADGE_RECENTER, y + BADGE_RECENTER);
}
[[maybe_unused]] static bool mic_only_shape(double x, double y) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    return mic_glyph(x + BADGE_RECENTER, y + BADGE_RECENTER);
}
[[maybe_unused]] static bool wifi_only_shape(double x, double y) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    x += BADGE_RECENTER; y += BADGE_RECENTER;
    action_coordinates(x, y);
    return std::hypot(x - 43, y - 50) <= 1.8 ||
           near_quadratic(x, y, 31, 40, 43, 29, 55, 40) ||
           near_quadratic(x, y, 35, 45, 43, 38, 51, 45);
}
// Eight teeth, a solid body and an open hub. The gear is centered on the
// action overlay, so rotation leaves its position and the white frame unchanged.
[[maybe_unused]] static bool gear_glyph(double x, double y) {
    const double dx = x - 40, dy = y - 40;
    const double radius = std::hypot(dx, dy);
    if (radius <= 5.5) return false;
    if (radius <= 11.5) return true;
    for (int tooth = 0; tooth < 8; ++tooth) {
        const double angle = tooth * std::acos(-1.0) / 4;
        const double radial = dx * std::cos(angle) + dy * std::sin(angle);
        const double tangent = -dx * std::sin(angle) + dy * std::cos(angle);
        if (radial >= 10 && radial <= 16 && std::abs(tangent) <= 2.8) return true;
    }
    return false;
}
[[maybe_unused]] static bool gear_shape(double x, double y) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    return frame_shape(x, y) || gear_glyph(x, y);
}
[[maybe_unused]] static bool gear_only_shape(double x, double y) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    return gear_glyph(x + BADGE_RECENTER, y + BADGE_RECENTER);
}
// Clockwise, one revolution every two seconds; no dependence on polling rate.
[[maybe_unused]] static double gear_spin_angle(double elapsed_seconds) {
    return -std::fmod(elapsed_seconds, 2.0) * std::acos(-1.0);
}
// Rotate the action's local X/Y basis only. Keep translation and billboard
// normal intact, including when the controller is tilted or tracking falls back.
[[maybe_unused]] static void rotate_action_axes(float (&matrix)[3][4], double angle) {
    const float c = static_cast<float>(std::cos(angle));
    const float s = static_cast<float>(std::sin(angle));
    for (int row = 0; row < 3; ++row) {
        const float x = matrix[row][0], y = matrix[row][1];
        matrix[row][0] = c * x + s * y;
        matrix[row][1] = -s * x + c * y;
    }
}
[[maybe_unused]] static bool micoff_only_shape(double x, double y) {
    x *= 72.0 / 64; y *= 72.0 / 64;
    x += BADGE_RECENTER; y += BADGE_RECENTER;
    bool slash = near_segment(x, y, 22.6, 57.4, 57.4, 22.6, 2.175);
    bool mic = mic_glyph(x, y) && !near_segment(x, y, 22.6, 57.4, 57.4, 22.6, 4.7125);
    return slash || mic;
}
template <typename Shape>
static void build_icon(unsigned char* out, Shape shape, int r, int g, int b) {
    const int W = 64, H = 64;
    for (int py = 0; py < H; ++py) {
        for (int px = 0; px < W; ++px) {
            int hits = 0;
            int red = 0, green = 0, blue = 0;
            for (int sy = 0; sy < 4; ++sy) {
                for (int sx = 0; sx < 4; ++sx) {
                    double x = px + (sx + 0.5) / 4.0;
                    double y = py + (sy + 0.5) / 4.0;
                    if (shape(x, y)) {
                        ++hits;
                        bool frame = frame_shape(x * 72.0 / 64, y * 72.0 / 64);
                        red += frame ? 255 : r;
                        green += frame ? 255 : g;
                        blue += frame ? 255 : b;
                    }
                }
            }
            unsigned char* p = &out[(py * W + px) * 4];
            p[0] = static_cast<unsigned char>(hits ? red / hits : r);
            p[1] = static_cast<unsigned char>(hits ? green / hits : g);
            p[2] = static_cast<unsigned char>(hits ? blue / hits : b);
            p[3] = static_cast<unsigned char>(hits * 255 / 16);
        }
    }
}
} // namespace frame_voice_icons
