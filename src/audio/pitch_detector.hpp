#pragma once

#include "audio/wav.hpp"

namespace chromakit {

struct PitchEstimate {
    double hz = 0.0;
    double confidence = 0.0;
};

PitchEstimate detect_pitch(const AudioBuffer& audio);

}

