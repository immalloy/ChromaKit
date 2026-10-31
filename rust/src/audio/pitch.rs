use std::{cmp::Ordering, error::Error, sync::Arc};

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use super::wav::AudioBuffer;

const MIN_HZ: f32 = 15.0;
const MAX_HZ: f32 = 1_200.0;
const CMND_THRESHOLD: f32 = 0.18;
const ANALYSIS_WINDOW: usize = 29_184;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchEstimate {
    pub hz: f32,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitchMethod {
    DirectYin,
    FftYin,
}

pub fn detect_pitch(
    audio: &AudioBuffer,
    method: PitchMethod,
) -> Result<PitchEstimate, Box<dyn Error + Send + Sync>> {
    detect_pitch_in_range(audio, method, MIN_HZ, MAX_HZ)
}

pub fn detect_pitch_in_range(
    audio: &AudioBuffer,
    method: PitchMethod,
    min_hz: f32,
    max_hz: f32,
) -> Result<PitchEstimate, Box<dyn Error + Send + Sync>> {
    if audio.sample_rate == 0 || audio.channels == 0 || audio.frames() < 64 {
        return Err("audio is too short or has an invalid format for pitch detection".into());
    }
    if !min_hz.is_finite()
        || !max_hz.is_finite()
        || min_hz <= 0.0
        || max_hz <= min_hz
        || max_hz >= audio.sample_rate as f32 / 2.0
    {
        return Err("pitch detection range must be positive and below Nyquist".into());
    }

    let length = ANALYSIS_WINDOW.min(audio.frames());
    let hop = (length / 4).max(256);
    let min_lag = ((audio.sample_rate as f32 / max_hz) as usize).max(2);
    let max_lag = (length / 2 - 1).min((audio.sample_rate as f32 / min_hz) as usize);
    if max_lag <= min_lag + 2 {
        return Err("audio is too short for the supported pitch range".into());
    }

    let mut fft: Option<(
        Arc<dyn Fft<f32>>,
        Arc<dyn Fft<f32>>,
        Vec<Complex<f32>>,
        Vec<Complex<f32>>,
        Vec<Complex<f32>>,
    )> = None;
    if method == PitchMethod::FftYin {
        let size = (length * 2).next_power_of_two();
        let mut planner = FftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let forward_scratch = vec![Complex::default(); forward.get_inplace_scratch_len()];
        let inverse_scratch = vec![Complex::default(); inverse.get_inplace_scratch_len()];
        fft = Some((
            forward,
            inverse,
            forward_scratch,
            inverse_scratch,
            vec![Complex::default(); size],
        ));
    }

    let mut candidates = Vec::new();
    let mut mono = vec![0.0f32; length];
    let mut prefix_energy = vec![0.0f64; length + 1];
    let mut differences = vec![0.0f64; max_lag + 1];
    let mut cmnd = vec![1.0f32; max_lag + 1];

    for start in (0..=audio.frames() - length).step_by(hop) {
        let mut mean = 0.0f64;
        for (frame, value) in mono.iter_mut().enumerate() {
            let mut sum = 0.0f64;
            for channel in 0..usize::from(audio.channels) {
                sum += f64::from(
                    audio.samples[(start + frame) * usize::from(audio.channels) + channel],
                );
            }
            *value = (sum / f64::from(audio.channels)) as f32;
            mean += f64::from(*value);
        }
        mean /= length as f64;

        let mut energy = 0.0f64;
        for (index, value) in mono.iter_mut().enumerate() {
            let window =
                0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / (length - 1) as f32).cos();
            *value = ((*value as f64 - mean) as f32) * window;
            let square = f64::from(*value) * f64::from(*value);
            energy += square;
            prefix_energy[index + 1] = prefix_energy[index] + square;
        }
        if energy < 1e-8 {
            continue;
        }

        match method {
            PitchMethod::DirectYin => {
                for lag in 1..=max_lag {
                    let mut difference = 0.0f64;
                    for index in 0..length - lag {
                        let delta = f64::from(mono[index]) - f64::from(mono[index + lag]);
                        difference += delta * delta;
                    }
                    differences[lag] = difference;
                }
            }
            PitchMethod::FftYin => {
                let (forward, inverse, forward_scratch, inverse_scratch, buffer) =
                    fft.as_mut().unwrap();
                buffer.fill(Complex::default());
                for (complex, &sample) in buffer.iter_mut().zip(&mono) {
                    complex.re = sample;
                }
                forward.process_with_scratch(buffer, forward_scratch);
                for value in buffer.iter_mut() {
                    *value = Complex::new(value.norm_sqr(), 0.0);
                }
                inverse.process_with_scratch(buffer, inverse_scratch);
                let scale = 1.0 / buffer.len() as f64;
                for lag in 1..=max_lag {
                    let left = prefix_energy[length - lag];
                    let right = prefix_energy[length] - prefix_energy[lag];
                    let correlation = f64::from(buffer[lag].re) * scale;
                    differences[lag] = (left + right - 2.0 * correlation).max(0.0);
                }
            }
        }

        cmnd.fill(1.0);
        let mut running = 0.0f64;
        for lag in 1..=max_lag {
            running += differences[lag];
            if running > 1e-20 {
                cmnd[lag] = (differences[lag] * lag as f64 / running) as f32;
            }
        }

        let selected = (min_lag + 1..max_lag)
            .find(|&lag| {
                cmnd[lag] < CMND_THRESHOLD
                    && cmnd[lag] <= cmnd[lag - 1]
                    && cmnd[lag] <= cmnd[lag + 1]
            })
            .unwrap_or_else(|| {
                (min_lag..=max_lag)
                    .min_by(|&a, &b| cmnd[a].total_cmp(&cmnd[b]))
                    .unwrap()
            });

        let center = cmnd[selected];
        let offset = if selected > 1 && selected < max_lag {
            let left = cmnd[selected - 1];
            let right = cmnd[selected + 1];
            let denominator = left - 2.0 * center + right;
            if denominator.abs() > 1e-12 {
                0.5 * (left - right) / denominator
            } else {
                0.0
            }
        } else {
            0.0
        };
        let refined_lag = (selected as f32 + offset).max(1.0);
        let confidence = (1.0 - center).clamp(0.0, 1.0);
        if confidence >= 0.35 {
            let weight = energy.sqrt() as f32 * confidence * confidence;
            candidates.push((audio.sample_rate as f32 / refined_lag, confidence, weight));
        }
    }

    if candidates.is_empty() {
        return Err("no confident fundamental frequency detected".into());
    }
    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    let total_weight: f32 = candidates.iter().map(|candidate| candidate.2).sum();
    let mut cumulative = 0.0;
    let mut selected = candidates[0];
    for &candidate in &candidates {
        cumulative += candidate.2;
        selected = candidate;
        if cumulative >= total_weight * 0.5 {
            break;
        }
    }
    let confidence = candidates
        .iter()
        .map(|candidate| candidate.1 * candidate.2)
        .sum::<f32>()
        / total_weight;
    Ok(PitchEstimate {
        hz: selected.0,
        confidence,
    })
}
