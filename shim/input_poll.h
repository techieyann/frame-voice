#pragma once
#include <openvr.h>
#include <cstdint>

template<class Input>
int fv_poll_inputs(Input* source, vr::VRActionSetHandle_t action_set,
                   const vr::VRActionHandle_t* actions, uint8_t* active,
                   uint8_t* state, vr::VRInputValueHandle_t* origins) {
    vr::VRActiveActionSet_t set = {};
    set.ulActionSet = action_set;
    auto error = source->UpdateActionState(&set, sizeof(set), 1);
    if (error != vr::VRInputError_None) return static_cast<int>(error);
    for (int i = 0; i < 5; ++i) {
        vr::InputDigitalActionData_t data = {};
        error = source->GetDigitalActionData(actions[i], &data, sizeof(data), vr::k_ulInvalidInputValueHandle);
        // An absent controller must not suppress the other controller's data.
        if (error == vr::VRInputError_NoData || error == vr::VRInputError_InvalidDevice) {
            active[i] = state[i] = 0;
            continue;
        }
        if (error != vr::VRInputError_None) return static_cast<int>(error);
        active[i] = data.bActive;
        state[i] = data.bActive && data.bState;
        if (i < 2 && data.bActive) origins[i] = data.activeOrigin;
    }
    return 0;
}
