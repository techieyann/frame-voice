// Portable raster regression, no OpenVR SDK required:
// c++ -O2 -std=c++17 -Wall -Wextra -Werror tests/badge_icons.cpp -o /tmp/badge-icons-test
#include "../shim/badge_icons.h"
#include <cassert>
#include <cstring>
#include <cstdio>
using namespace frame_voice_icons;
int main() {
    unsigned char reference[64 * 64 * 4], frame[64 * 64 * 4], corner[64 * 64 * 4];
    build_icon(reference, mic_shape, 26, 159, 255);
    build_icon(corner, [](double x, double y) { return growing_mic_shape(x, y, 0); }, 26, 159, 255);
    for (int step = 0; step <= 10; ++step) {
        const double progress = step / 10.0;
        build_icon(frame, [progress](double x, double y) { return growing_mic_shape(x, y, progress); }, 26, 159, 255);
        for (int pixel = 0; pixel < 64 * 64; ++pixel) {
            if (corner[pixel * 4 + 3]) assert(std::memcmp(frame + pixel * 4, corner + pixel * 4, 4) == 0);
        }
        if (step == 0) assert(std::memcmp(frame, corner, sizeof(frame)) == 0);
        if (step == 10) assert(std::memcmp(frame, reference, sizeof(frame)) == 0);
    }
    // Compile/check the other state generators as part of the portable module.
    build_icon(frame, gear_shape, 26, 159, 255);
    assert(gear_glyph(56, 40));  // tooth
    assert(gear_glyph(48, 40));  // body
    assert(!gear_glyph(40, 40)); // open hub
    assert(!gear_glyph(59, 40)); // outside
    build_icon(frame, gear_only_shape, 26, 159, 255);
    for (int pixel = 0; pixel < 64 * 64; ++pixel) {
        if (frame[pixel * 4 + 3]) {
            assert(frame[pixel * 4] == 26);
            assert(frame[pixel * 4 + 1] == 159);
            assert(frame[pixel * 4 + 2] == 255);
        }
    }
    // Tilted local basis: rotation must preserve translation and the normal.
    float basis[3][4] = {{1, 0, 0, 3}, {0, 0, -1, 4}, {0, 1, 0, 5}};
    rotate_action_axes(basis, gear_spin_angle(0.5));
    assert(std::abs(basis[0][0]) < 1e-6 && std::abs(basis[2][0] + 1) < 1e-6);
    assert(std::abs(basis[0][1] - 1) < 1e-6 && std::abs(basis[2][1]) < 1e-6);
    assert(basis[1][2] == -1);
    for (int row = 0; row < 3; ++row) assert(basis[row][3] == 3 + row);
    assert(std::abs(gear_spin_angle(2.0)) < 1e-6);
    assert(std::abs(gear_spin_angle(1000.5) - gear_spin_angle(0.5)) < 1e-6);
    build_icon(frame, micoff_shape, 240, 90, 90);
    puts("Mic growth, blue gear, and clockwise rotation with fixed position/normal pass.");
}
