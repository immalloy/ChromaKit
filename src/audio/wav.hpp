#pragma once

#include <cstdint>
#include <filesystem>
#include <string>
#include <vector>

namespace chromakit {

struct AudioBuffer {
    uint32_t sample_rate = 0;
    uint16_t channels = 0;
    std::vector<float> samples;

    size_t frames() const { return channels == 0 ? 0 : samples.size() / channels; }
};

AudioBuffer read_wav(const std::filesystem::path& path);
void write_wav(const std::filesystem::path& path, const AudioBuffer& audio);

}

