#include "audio/pitch_shifter.hpp"

#include <rubberband/RubberBandStretcher.h>

#include <algorithm>
#include <cmath>
#include <memory>
#include <stdexcept>
#include <vector>

namespace chromakit {

AudioBuffer pitch_shift(const AudioBuffer& input, double pitch_scale) {
    if (input.sample_rate == 0 || input.channels == 0 || input.frames() == 0) {
        throw std::runtime_error("cannot pitch-shift empty audio");
    }
    if (!std::isfinite(pitch_scale) || pitch_scale <= 0.01 || pitch_scale > 100.0) {
        throw std::runtime_error("invalid pitch ratio");
    }

    using namespace RubberBand;
    const auto options = RubberBandStretcher::OptionProcessOffline |
                         RubberBandStretcher::OptionEngineFiner |
                         RubberBandStretcher::OptionPitchHighConsistency |
                         RubberBandStretcher::OptionFormantPreserved |
                         RubberBandStretcher::OptionChannelsTogether;
    RubberBandStretcher stretcher(input.sample_rate, input.channels, options, 1.0, pitch_scale);
    stretcher.setMaxProcessSize(input.frames());

    std::vector<std::vector<float>> channels(input.channels, std::vector<float>(input.frames()));
    std::vector<const float*> input_ptrs(input.channels);
    for (size_t frame = 0; frame < input.frames(); ++frame) {
        for (uint16_t channel = 0; channel < input.channels; ++channel) {
            channels[channel][frame] = input.samples[frame * input.channels + channel];
        }
    }
    for (uint16_t channel = 0; channel < input.channels; ++channel) input_ptrs[channel] = channels[channel].data();

    stretcher.study(input_ptrs.data(), input.frames(), true);
    stretcher.process(input_ptrs.data(), input.frames(), true);

    std::vector<std::vector<float>> output_channels(input.channels, std::vector<float>(input.frames() + input.sample_rate));
    std::vector<float*> output_ptrs(input.channels);
    for (uint16_t channel = 0; channel < input.channels; ++channel) output_ptrs[channel] = output_channels[channel].data();

    size_t written = 0;
    while (stretcher.available() > 0 && written < output_channels[0].size()) {
        const size_t request = std::min<size_t>(stretcher.available(), output_channels[0].size() - written);
        std::vector<float*> destinations(input.channels);
        for (uint16_t channel = 0; channel < input.channels; ++channel) destinations[channel] = output_channels[channel].data() + written;
        const size_t got = stretcher.retrieve(destinations.data(), request);
        if (got == 0) break;
        written += got;
    }

    AudioBuffer output{input.sample_rate, input.channels, std::vector<float>(input.frames() * input.channels, 0.0f)};
    const size_t kept = std::min(input.frames(), written);
    for (size_t frame = 0; frame < kept; ++frame) {
        for (uint16_t channel = 0; channel < input.channels; ++channel) {
            output.samples[frame * input.channels + channel] = output_channels[channel][frame];
        }
    }
    return output;
}

}

