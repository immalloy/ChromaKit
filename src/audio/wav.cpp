#include "audio/wav.hpp"

#include <algorithm>
#include <cmath>
#include <cstring>
#include <fstream>
#include <limits>
#include <stdexcept>

namespace chromakit {
namespace {

uint16_t read_u16(const std::vector<uint8_t>& data, size_t offset) {
    if (offset + 2 > data.size()) throw std::runtime_error("truncated WAV header");
    return static_cast<uint16_t>(data[offset] | (data[offset + 1] << 8));
}

uint32_t read_u32(const std::vector<uint8_t>& data, size_t offset) {
    if (offset + 4 > data.size()) throw std::runtime_error("truncated WAV header");
    return static_cast<uint32_t>(data[offset]) |
           (static_cast<uint32_t>(data[offset + 1]) << 8) |
           (static_cast<uint32_t>(data[offset + 2]) << 16) |
           (static_cast<uint32_t>(data[offset + 3]) << 24);
}

int32_t read_i24(const uint8_t* p) {
    int32_t value = static_cast<int32_t>(p[0]) |
                    (static_cast<int32_t>(p[1]) << 8) |
                    (static_cast<int32_t>(p[2]) << 16);
    if ((value & 0x00800000) != 0) value |= 0xff000000;
    return value;
}

void require(bool condition, const std::string& message) {
    if (!condition) throw std::runtime_error(message);
}

}

AudioBuffer read_wav(const std::filesystem::path& path) {
    std::ifstream file(path, std::ios::binary);
    if (!file) throw std::runtime_error("cannot open WAV: " + path.string());

    file.seekg(0, std::ios::end);
    const auto size = file.tellg();
    require(size >= 12, "WAV is too small: " + path.string());
    file.seekg(0, std::ios::beg);
    std::vector<uint8_t> bytes(static_cast<size_t>(size));
    file.read(reinterpret_cast<char*>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
    require(file.good() || file.eof(), "cannot read WAV: " + path.string());
    require(std::memcmp(bytes.data(), "RIFF", 4) == 0 && std::memcmp(bytes.data() + 8, "WAVE", 4) == 0,
            "not a RIFF/WAVE file: " + path.string());

    uint16_t format = 0;
    uint16_t channels = 0;
    uint32_t sample_rate = 0;
    uint16_t bits = 0;
    size_t data_offset = 0;
    size_t data_size = 0;

    size_t offset = 12;
    while (offset + 8 <= bytes.size()) {
        const char* id = reinterpret_cast<const char*>(bytes.data() + offset);
        const uint32_t chunk_size = read_u32(bytes, offset + 4);
        const size_t chunk_start = offset + 8;
        require(chunk_start <= bytes.size() && chunk_size <= bytes.size() - chunk_start, "truncated WAV chunk");
        if (std::memcmp(id, "fmt ", 4) == 0) {
            require(chunk_size >= 16, "invalid WAV fmt chunk");
            format = read_u16(bytes, chunk_start);
            channels = read_u16(bytes, chunk_start + 2);
            sample_rate = read_u32(bytes, chunk_start + 4);
            bits = read_u16(bytes, chunk_start + 14);
        } else if (std::memcmp(id, "data", 4) == 0) {
            data_offset = chunk_start;
            data_size = chunk_size;
        }
        offset = chunk_start + chunk_size + (chunk_size & 1u);
    }

    require((format == 1 || format == 3) && channels > 0 && sample_rate > 0 && bits > 0,
            "unsupported or missing WAV format");
    require(data_size > 0, "WAV has no audio data");
    const size_t bytes_per_sample = (bits + 7) / 8;
    require((bits == 8 || bits == 16 || bits == 24 || bits == 32 || (format == 3 && bits == 64)) &&
                data_size % (bytes_per_sample * channels) == 0,
            "unsupported WAV sample format");

    AudioBuffer audio;
    audio.sample_rate = sample_rate;
    audio.channels = channels;
    audio.samples.resize(data_size / bytes_per_sample, 0.0f);
    const uint8_t* source = bytes.data() + data_offset;
    for (size_t i = 0; i < audio.samples.size(); ++i) {
        if (format == 3 && bits == 32) {
            float value;
            std::memcpy(&value, source + i * 4, 4);
            audio.samples[i] = std::isfinite(value) ? value : 0.0f;
        } else if (format == 3 && bits == 64) {
            double value;
            std::memcpy(&value, source + i * 8, 8);
            audio.samples[i] = std::isfinite(value) ? static_cast<float>(value) : 0.0f;
        } else if (bits == 8) {
            audio.samples[i] = (static_cast<int>(source[i]) - 128) / 128.0f;
        } else if (bits == 16) {
            const int16_t value = static_cast<int16_t>(source[i * 2] | (source[i * 2 + 1] << 8));
            audio.samples[i] = static_cast<float>(value) / 32768.0f;
        } else if (bits == 24) {
            audio.samples[i] = static_cast<float>(read_i24(source + i * 3)) / 8388608.0f;
        } else {
            const int32_t value = static_cast<int32_t>(read_u32(bytes, data_offset + i * 4));
            audio.samples[i] = static_cast<float>(static_cast<double>(value) / 2147483648.0);
        }
    }
    return audio;
}

void write_wav(const std::filesystem::path& path, const AudioBuffer& audio) {
    require(audio.sample_rate > 0 && audio.channels > 0, "cannot write invalid audio format");
    require(audio.samples.size() % audio.channels == 0, "audio samples are not frame-aligned");
    const uint32_t data_size = static_cast<uint32_t>(audio.samples.size() * sizeof(float));
    const uint32_t riff_size = 36 + data_size;

    std::ofstream file(path, std::ios::binary);
    if (!file) throw std::runtime_error("cannot create WAV: " + path.string());
    auto write_u16 = [&file](uint16_t value) { file.put(static_cast<char>(value & 0xff)); file.put(static_cast<char>((value >> 8) & 0xff)); };
    auto write_u32 = [&file](uint32_t value) { for (int i = 0; i < 4; ++i) file.put(static_cast<char>((value >> (8 * i)) & 0xff)); };
    file.write("RIFF", 4); write_u32(riff_size); file.write("WAVE", 4);
    file.write("fmt ", 4); write_u32(16); write_u16(3); write_u16(audio.channels);
    write_u32(audio.sample_rate); write_u32(audio.sample_rate * audio.channels * sizeof(float));
    write_u16(static_cast<uint16_t>(audio.channels * sizeof(float))); write_u16(32);
    file.write("data", 4); write_u32(data_size);
    file.write(reinterpret_cast<const char*>(audio.samples.data()), static_cast<std::streamsize>(data_size));
    if (!file) throw std::runtime_error("failed writing WAV: " + path.string());
}

}

