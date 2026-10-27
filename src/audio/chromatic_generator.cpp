#include "audio/chromatic_generator.hpp"

#include "audio/pitch_shifter.hpp"
#include "audio/wav.hpp"

#include <algorithm>
#include <cmath>
#include <fstream>
#include <future>
#include <iomanip>
#include <sstream>
#include <stdexcept>
#include <thread>

namespace chromakit {
namespace {

void report(const ProgressCallback& progress, int percent, const std::string& message) {
    if (progress) progress(percent, message);
}

std::vector<std::filesystem::path> numbered_samples(const std::filesystem::path& folder) {
    std::vector<std::filesystem::path> result;
    for (int index = 1;; ++index) {
        const auto path = folder / (std::to_string(index) + ".wav");
        if (!std::filesystem::exists(path)) break;
        result.push_back(path);
    }
    if (result.empty()) throw std::runtime_error("no numbered WAV files found in sample folder");
    return result;
}

void append(std::vector<float>& destination, const AudioBuffer& source) {
    destination.insert(destination.end(), source.samples.begin(), source.samples.end());
}

void clear_generated_samples(const std::filesystem::path& folder) {
    if (!std::filesystem::exists(folder)) return;
    for (const auto& entry : std::filesystem::directory_iterator(folder)) {
        const auto name = entry.path().filename().string();
        if (entry.is_regular_file() && entry.path().extension() == ".wav" && name.starts_with("pitched_")) {
            std::filesystem::remove(entry.path());
        }
    }
}

}

double midi_to_hz(int midi) {
    return 440.0 * std::pow(2.0, (midi - 69) / 12.0);
}

double pitch_ratio(int target_midi, double source_hz) {
    if (!std::isfinite(source_hz) || source_hz <= 0.0) throw std::runtime_error("source pitch must be positive");
    return midi_to_hz(target_midi) / source_hz;
}

GenerationReport generate_chromatic(const GeneratorSettings& settings, ProgressCallback progress) {
    if (!std::filesystem::is_directory(settings.sample_folder)) throw std::runtime_error("sample folder does not exist");
    if (settings.range <= 0 || settings.range > 512) throw std::runtime_error("range must be between 1 and 512 notes");
    if (!std::isfinite(settings.gap_seconds) || settings.gap_seconds < 0.0 || settings.gap_seconds > 60.0) {
        throw std::runtime_error("gap must be between 0 and 60 seconds");
    }

    const auto paths = numbered_samples(settings.sample_folder);
    std::vector<AudioBuffer> originals;
    GenerationReport result;
    result.chromatic_path = settings.sample_folder / "chromatic.wav";
    originals.reserve(paths.size());
    result.samples.reserve(paths.size());

    for (size_t i = 0; i < paths.size(); ++i) {
        report(progress, static_cast<int>(i * 20 / paths.size()), "Reading " + paths[i].filename().string());
        originals.push_back(read_wav(paths[i]));
        if (settings.pitched) result.samples.push_back({paths[i], detect_pitch(originals.back())});
        else result.samples.push_back({paths[i], {0.0, 0.0}});
    }

    const uint32_t sample_rate = originals.front().sample_rate;
    const uint16_t channels = originals.front().channels;
    for (const auto& audio : originals) {
        if (audio.sample_rate != sample_rate || audio.channels != channels) {
            throw std::runtime_error("all numbered WAVs must have the same sample rate and channel count");
        }
    }
    if (settings.pitched) {
        for (int note = 0; note < settings.range; ++note) {
            const size_t source_index = static_cast<size_t>(note) % originals.size();
            const double ratio = pitch_ratio(settings.start_midi + note, result.samples[source_index].pitch.hz);
            if (!std::isfinite(ratio) || ratio <= 0.01 || ratio > 100.0) {
                throw std::runtime_error("requested range contains a note outside the supported 0.01x-100x pitch range");
            }
        }
    }

    std::vector<float> chromatic;
    const size_t gap_frames = static_cast<size_t>(std::llround(settings.gap_seconds * sample_rate));
    std::vector<AudioBuffer> generated(static_cast<size_t>(settings.range));
    const auto render_note = [&](int note) {
        const size_t source_index = static_cast<size_t>(note) % originals.size();
        const auto& source = originals[source_index];
        if (!settings.pitched) return source;

        const double source_hz = result.samples[source_index].pitch.hz;
        if (!(source_hz > 0.0)) throw std::runtime_error("pitch detector returned no usable frequency");
        return pitch_shift(source, pitch_ratio(settings.start_midi + note, source_hz));
    };

    const size_t parallelism = std::max<size_t>(1, std::thread::hardware_concurrency());
    for (size_t batch_start = 0; batch_start < generated.size(); batch_start += parallelism) {
        const size_t batch_end = std::min(generated.size(), batch_start + parallelism);
        std::vector<std::future<AudioBuffer>> pending;
        pending.reserve(batch_end - batch_start);
        for (size_t note = batch_start; note < batch_end; ++note) {
            pending.push_back(std::async(std::launch::async, render_note, static_cast<int>(note)));
        }
        for (size_t offset = 0; offset < pending.size(); ++offset) {
            const size_t note = batch_start + offset;
            generated[note] = pending[offset].get();
            append(chromatic, generated[note]);
            if (note + 1 < generated.size()) chromatic.insert(chromatic.end(), gap_frames * channels, 0.0f);
            report(progress, 20 + (static_cast<int>(note + 1) * 75 / settings.range),
                   "Rendering note " + std::to_string(settings.start_midi + static_cast<int>(note)));
        }
    }

    write_wav(result.chromatic_path, {sample_rate, channels, std::move(chromatic)});
    if (settings.dump_individual) {
        const auto output_folder = settings.sample_folder / "pitched_samples";
        std::filesystem::create_directories(output_folder);
        clear_generated_samples(output_folder);
        for (size_t index = 0; index < generated.size(); ++index) {
            std::ostringstream name;
            name << "pitched_" << std::setw(3) << std::setfill('0') << index + 1 << ".wav";
            write_wav(output_folder / name.str(), generated[index]);
        }
    }
    result.output_notes = generated.size();
    report(progress, 100, "Finished: " + result.chromatic_path.string());
    return result;
}

}

