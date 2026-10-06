#include "../shim/input_poll.h"
#include <cassert>
#include <initializer_list>

struct FakeInput {
    int missing = -1;
    vr::EVRInputError failure = vr::VRInputError_NoData;
    vr::EVRInputError UpdateActionState(vr::VRActiveActionSet_t*, uint32_t, uint32_t) { return vr::VRInputError_None; }
    vr::EVRInputError GetDigitalActionData(vr::VRActionHandle_t action, vr::InputDigitalActionData_t* data, uint32_t, vr::VRInputValueHandle_t) {
        if (static_cast<int>(action) == missing) return failure;
        data->bActive = true;
        data->bState = action < 2;
        data->activeOrigin = action+100;
        return vr::VRInputError_None;
    }
};
int main() {
    vr::VRActionHandle_t actions[5] = {0,1,2,3,4};
    for (auto error : {vr::VRInputError_NoData, vr::VRInputError_InvalidDevice}) {
        for (int hand : {0,1}) {
            FakeInput source;
            source.missing = hand;
            source.failure = error;
            uint8_t active[5] = {1,1,1,1,1}, state[5] = {1,1,1,1,1};
            vr::VRInputValueHandle_t origins[2] = {};
            assert(fv_poll_inputs(&source, 1, actions, active, state, origins) == 0);
            assert(active[hand] == 0 && state[hand] == 0);
            assert(active[1-hand] == 1 && state[1-hand] == 1);
            assert(origins[1-hand] == static_cast<vr::VRInputValueHandle_t>(101-hand));
        }
    }
    FakeInput source;
    source.missing = 1;
    source.failure = vr::VRInputError_InvalidHandle;
    uint8_t active[5] = {}, state[5] = {};
    vr::VRInputValueHandle_t origins[2] = {};
    assert(fv_poll_inputs(&source, 1, actions, active, state, origins) == static_cast<int>(vr::VRInputError_InvalidHandle));
}
