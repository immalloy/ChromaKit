#pragma once

#include "audio/pitch_detector.hpp"

#include <filesystem>
#include <functional>
#include <vector>

namespace chromakit {

struct GeneratorSettings {
    std::filesystem::path sample_folder;
    int start_midi = 36;
    int range = 24;
    double gap_seconds = 0.1;
    bool pitched = true;
    bool dump_individual = true;
};

struct DetectedSample {
    std::filesystem::path path;
    PitchEstimate pitch;
};

struct GenerationReport {
    std::filesystem::path chromatic_path;
    std::vector<DetectedSample> samples;
    size_t output_notes = 0;
};

using ProgressCallback = std::function<void(int percent, const std::string& message)>;

GenerationReport generate_chromatic(const GeneratorSettings& settings, ProgressCallback progress = {});

double midi_to_hz(int midi);
double pitch_ratio(int target_midi, double source_hz);

}

