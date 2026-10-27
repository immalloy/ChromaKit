#pragma once

#include "audio/wav.hpp"

namespace chromakit {

AudioBuffer pitch_shift(const AudioBuffer& input, double pitch_scale);

}

