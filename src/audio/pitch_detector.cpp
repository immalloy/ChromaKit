#include "audio/pitch_detector.hpp"

#include <algorithm>
#include <cmath>
#include <numeric>
#include <stdexcept>
#include <vector>

namespace chromakit {
namespace {

constexpr double kMinHz = 15.0;
constexpr double kMaxHz = 1200.0;
constexpr double kThreshold = 0.18;
constexpr double kPi = 3.14159265358979323846;
constexpr size_t kAnalysisWindow = 29184;

double clamp(double value, double low, double high) {
    return std::max(low, std::min(high, value));
}

}

PitchEstimate detect_pitch(const AudioBuffer& audio) {
    if (audio.sample_rate == 0 || audio.channels == 0 || audio.frames() < 64) {
        throw std::runtime_error("sample is too short for pitch detection");
    }

    const size_t frame_length = std::min(kAnalysisWindow, audio.frames());
    const size_t hop = std::max<size_t>(256, frame_length / 4);
    const size_t min_lag = std::max<size_t>(2, static_cast<size_t>(audio.sample_rate / kMaxHz));
    const size_t max_lag = std::min(frame_length / 2 - 1,
                                    static_cast<size_t>(audio.sample_rate / kMinHz));
    if (max_lag <= min_lag + 2) throw std::runtime_error("sample is too short for the configured pitch range");

    std::vector<double> mono(frame_length);
    std::vector<double> differences(max_lag + 1);
    std::vector<double> cmnd(max_lag + 1);
    std::vector<double> estimates;
    std::vector<double> weights;
    std::vector<double> confidences;

    for (size_t start = 0; start + frame_length <= audio.frames(); start += hop) {
        double mean = 0.0;
        for (size_t i = 0; i < frame_length; ++i) {
            double sample = 0.0;
            for (uint16_t channel = 0; channel < audio.channels; ++channel) {
                sample += audio.samples[(start + i) * audio.channels + channel];
            }
            mono[i] = sample / audio.channels;
            mean += mono[i];
        }
        mean /= frame_length;

        double energy = 0.0;
        for (size_t i = 0; i < frame_length; ++i) {
            const double window = 0.5 - 0.5 * std::cos(2.0 * kPi * i / (frame_length - 1));
            mono[i] = (mono[i] - mean) * window;
            energy += mono[i] * mono[i];
        }
        if (energy < 1e-8) continue;

        std::fill(differences.begin(), differences.end(), 0.0);
        std::fill(cmnd.begin(), cmnd.end(), 1.0);
        double running = 0.0;
        size_t best = min_lag;
        double best_value = 1.0;
        for (size_t lag = 1; lag <= max_lag; ++lag) {
            double difference = 0.0;
            for (size_t i = 0; i + lag < frame_length; ++i) {
                const double delta = mono[i] - mono[i + lag];
                difference += delta * delta;
            }
            differences[lag] = difference;
            running += difference;
            if (running > 1e-20) cmnd[lag] = difference * lag / running;
            if (lag >= min_lag && cmnd[lag] < best_value) {
                best_value = cmnd[lag];
                best = lag;
            }
        }

        size_t selected = best;
        for (size_t lag = min_lag + 1; lag < max_lag; ++lag) {
            if (cmnd[lag] < kThreshold && cmnd[lag] <= cmnd[lag - 1] && cmnd[lag] <= cmnd[lag + 1]) {
                selected = lag;
                break;
            }
        }

        double refined = static_cast<double>(selected);
        if (selected > 1 && selected < max_lag) {
            const double left = cmnd[selected - 1];
            const double center = cmnd[selected];
            const double right = cmnd[selected + 1];
            const double denominator = left - 2.0 * center + right;
            if (std::abs(denominator) > 1e-12) refined += 0.5 * (left - right) / denominator;
        }
        const double confidence = clamp(1.0 - cmnd[selected], 0.0, 1.0);
        if (confidence >= 0.35) {
            estimates.push_back(audio.sample_rate / refined);
            weights.push_back(std::sqrt(energy) * confidence * confidence);
            confidences.push_back(confidence);
        }
    }

    if (estimates.empty()) throw std::runtime_error("no confident fundamental frequency detected");
    std::vector<size_t> order(estimates.size());
    std::iota(order.begin(), order.end(), 0);
    std::sort(order.begin(), order.end(), [&](size_t a, size_t b) { return estimates[a] < estimates[b]; });
    constexpr double kConsensusCents = 100.0;
    const double consensus_log_width = std::log(std::pow(2.0, kConsensusCents / 1200.0));
    size_t cluster_seed = 0;
    double best_cluster_weight = -1.0;
    for (size_t candidate = 0; candidate < estimates.size(); ++candidate) {
        double cluster_weight = 0.0;
        for (size_t other = 0; other < estimates.size(); ++other) {
            if (std::abs(std::log(estimates[other] / estimates[candidate])) <= consensus_log_width) {
                cluster_weight += weights[other];
            }
        }
        if (cluster_weight > best_cluster_weight) {
            best_cluster_weight = cluster_weight;
            cluster_seed = candidate;
        }
    }

    std::vector<size_t> cluster;
    for (size_t candidate = 0; candidate < estimates.size(); ++candidate) {
        if (std::abs(std::log(estimates[candidate] / estimates[cluster_seed])) <= consensus_log_width) {
            cluster.push_back(candidate);
        }
    }

    double total_weight = 0.0;
    for (size_t index : cluster) total_weight += weights[index];
    double cumulative = 0.0;
    size_t median_index = cluster.front();
    for (size_t index : order) {
        if (std::find(cluster.begin(), cluster.end(), index) == cluster.end()) continue;
        cumulative += weights[index];
        if (cumulative >= total_weight * 0.5) {
            median_index = index;
            break;
        }
    }
    double confidence_weight = 0.0;
    for (size_t index : cluster) confidence_weight += weights[index] * confidences[index];
    return {estimates[median_index], clamp(confidence_weight / total_weight, 0.0, 1.0)};
}

}

