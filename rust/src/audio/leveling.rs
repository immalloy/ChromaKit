use std::error::Error;

use super::wav::AudioBuffer;

const NORMALIZE_TARGET_DBFS: f32 = -14.0;
const NORMALIZE_MAX_BOOST_DB: f32 = 36.0;
const ATTACK_WINDOW_MS: usize = 5;
const ATTACK_HOP_MS: usize = 2;
const ATTACK_SCAN_MS: usize = 80;
const ATTACK_HEADROOM_DB: f32 = 3.0;

#[derive(Clone, Copy, Debug)]
pub struct LevelingOptions {
    pub max_gain_db: f32,
    pub trigger_drop_db: f32,
    pub target_headroom_db: f32,
}

impl Default for LevelingOptions {
    fn default() -> Self {
        Self {
            max_gain_db: 3.0,
            trigger_drop_db: 2.0,
            target_headroom_db: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelingReport {
    pub applied: bool,
    pub max_gain_db: f32,
    pub peak_before: f32,
    pub peak_after: f32,
}

pub fn auto_level_sustain(
    audio: &mut AudioBuffer,
    options: LevelingOptions,
) -> Result<LevelingReport, Box<dyn Error + Send + Sync>> {
    if audio.sample_rate == 0
        || audio.channels == 0
        || audio.samples.len() % usize::from(audio.channels) != 0
    {
        return Err("audio format is invalid or sample count is not frame-aligned".into());
    }
    if !options.max_gain_db.is_finite()
        || !(0.0..=6.0).contains(&options.max_gain_db)
        || !options.trigger_drop_db.is_finite()
        || !(0.5..=12.0).contains(&options.trigger_drop_db)
        || !options.target_headroom_db.is_finite()
        || !(0.0..=6.0).contains(&options.target_headroom_db)
    {
        return Err("leveling settings are outside their safe ranges".into());
    }

    let peak_before = audio
        .samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    let frames = audio.frames();
    let window = (audio.sample_rate as usize / 50).max(1);
    let hop = (audio.sample_rate as usize / 100).max(1);
    if frames < window || options.max_gain_db == 0.0 {
        return Ok(LevelingReport {
            peak_before,
            peak_after: peak_before,
            ..LevelingReport::default()
        });
    }

    let channels = usize::from(audio.channels);
    let mut rms_db = Vec::new();
    for start in (0..=frames - window).step_by(hop) {
        let mut square_sum = 0.0f64;
        for sample in &audio.samples[start * channels..(start + window) * channels] {
            square_sum += f64::from(*sample) * f64::from(*sample);
        }
        let rms = (square_sum / (window * channels) as f64).sqrt() as f32;
        rms_db.push(20.0 * rms.max(1e-12).log10());
    }
    if rms_db.len() < 4 {
        return Ok(LevelingReport {
            peak_before,
            peak_after: peak_before,
            ..LevelingReport::default()
        });
    }

    let mut sorted = rms_db.clone();
    sorted.sort_by(f32::total_cmp);
    let p95 = sorted[(sorted.len() - 1) * 95 / 100];
    let active_floor = (p95 - 45.0).max(-70.0);
    let first_active = match rms_db.iter().position(|&level| level >= active_floor) {
        Some(index) => index,
        None => {
            return Ok(LevelingReport {
                peak_before,
                peak_after: peak_before,
                ..LevelingReport::default()
            });
        }
    };
    let last_active = rms_db
        .iter()
        .rposition(|&level| level >= active_floor)
        .unwrap();
    let onset_end =
        (first_active + ((audio.sample_rate as usize / 10) / hop).max(1)).min(last_active + 1);
    let attack_reference = median(&rms_db[first_active..onset_end]);
    let sustain_start = onset_end;

    let fade_guard = (audio.sample_rate as usize / 5) / hop;
    let sustain_end = last_active.saturating_sub(fade_guard).max(sustain_start);
    if sustain_end.saturating_sub(sustain_start) < 3
        || !rms_db[sustain_start..sustain_end]
            .iter()
            .any(|&level| level < attack_reference - options.trigger_drop_db)
    {
        return Ok(LevelingReport {
            peak_before,
            peak_after: peak_before,
            ..LevelingReport::default()
        });
    }

    let mut gain_db = vec![0.0f32; rms_db.len()];
    let mut previous = 0.0f32;
    let mut maximum = 0.0f32;
    for (index, &level) in rms_db.iter().enumerate() {
        let desired = if index >= sustain_start && index < sustain_end && level >= active_floor {
            (attack_reference - level - options.target_headroom_db).clamp(0.0, options.max_gain_db)
        } else {
            0.0
        };
        let time_constant = if desired > previous { 0.20 } else { 0.40 };
        let alpha = 1.0 - (-(hop as f32 / audio.sample_rate as f32) / time_constant).exp();
        previous += alpha * (desired - previous);
        gain_db[index] = previous;
        maximum = maximum.max(previous);
    }
    if maximum < 0.1 {
        return Ok(LevelingReport {
            peak_before,
            peak_after: peak_before,
            ..LevelingReport::default()
        });
    }

    for (index, &gain) in gain_db.iter().enumerate() {
        let start = index * hop;
        let end = (start + hop).min(frames);
        let scale = 10.0f32.powf(gain / 20.0);
        for sample in &mut audio.samples[start * channels..end * channels] {
            *sample *= scale;
        }
    }

    let compensated_peak = audio
        .samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    if compensated_peak > 0.98 {
        let safety_scale = 0.98 / compensated_peak;
        for sample in &mut audio.samples {
            *sample *= safety_scale;
        }
    }
    let peak_after = audio
        .samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    Ok(LevelingReport {
        applied: true,
        max_gain_db: maximum,
        peak_before,
        peak_after,
    })
}

pub fn normalize_sustain(
    audio: &mut AudioBuffer,
) -> Result<LevelingReport, Box<dyn Error + Send + Sync>> {
    if audio.sample_rate == 0
        || audio.channels == 0
        || audio.samples.is_empty()
        || audio.samples.len() % usize::from(audio.channels) != 0
    {
        return Err("audio format is invalid or sample count is not frame-aligned".into());
    }

    let peak_before = audio
        .samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    let channels = usize::from(audio.channels);
    let frames = audio.frames();
    let window = (audio.sample_rate as usize / 50).max(1);
    let hop = (audio.sample_rate as usize / 100).max(1);
    if frames < window * 4 {
        return Ok(LevelingReport {
            peak_before,
            peak_after: peak_before,
            ..LevelingReport::default()
        });
    }

    let mut rms = Vec::new();
    for start in (0..=frames - window).step_by(hop) {
        let samples = &audio.samples[start * channels..(start + window) * channels];
        let square_sum = samples
            .iter()
            .map(|sample| f64::from(*sample) * f64::from(*sample))
            .sum::<f64>();
        rms.push((square_sum / samples.len() as f64).sqrt() as f32);
    }
    let mut sorted = rms.clone();
    sorted.sort_by(f32::total_cmp);
    let active_floor = (sorted[(sorted.len() - 1) * 95 / 100] * 10.0f32.powf(-45.0 / 20.0))
        .max(10.0f32.powf(-70.0 / 20.0));
    let Some(first_active) = rms.iter().position(|&level| level >= active_floor) else {
        return Ok(LevelingReport {
            peak_before,
            peak_after: peak_before,
            ..LevelingReport::default()
        });
    };
    let last_active = rms
        .iter()
        .rposition(|&level| level >= active_floor)
        .unwrap();
    let sustain_start = (first_active + (audio.sample_rate as usize / 10) / hop).min(last_active);
    let sustain_end = last_active
        .saturating_sub((audio.sample_rate as usize / 5) / hop)
        .max(sustain_start);
    let sustain = if sustain_end.saturating_sub(sustain_start) >= 3 {
        &rms[sustain_start..sustain_end]
    } else {
        &rms[first_active..=last_active]
    };
    let sustain_rms = median(sustain);
    if !sustain_rms.is_finite() || sustain_rms <= 1e-12 {
        return Ok(LevelingReport {
            peak_before,
            peak_after: peak_before,
            ..LevelingReport::default()
        });
    }

    let attack_window = ((audio.sample_rate as usize * ATTACK_WINDOW_MS) / 1_000).max(1);
    let attack_hop = ((audio.sample_rate as usize * ATTACK_HOP_MS) / 1_000).max(1);
    let attack_frames = ((audio.sample_rate as usize * ATTACK_SCAN_MS) / 1_000).min(frames);
    let threshold = sustain_rms * 10.0f32.powf(ATTACK_HEADROOM_DB / 20.0);
    let mut envelope = Vec::new();
    let mut previous_gain = 1.0f32;
    for start in (0..attack_frames).step_by(attack_hop) {
        let end = (start + attack_window).min(frames);
        let samples = &audio.samples[start * channels..end * channels];
        let square_sum = samples
            .iter()
            .map(|sample| f64::from(*sample) * f64::from(*sample))
            .sum::<f64>();
        let level = (square_sum / samples.len() as f64).sqrt() as f32;
        let desired_gain = if level > threshold {
            (threshold / level).sqrt()
        } else {
            1.0
        };
        let time_constant = if desired_gain < previous_gain {
            0.002
        } else {
            0.015
        };
        let alpha = 1.0 - (-(attack_hop as f32 / audio.sample_rate as f32) / time_constant).exp();
        previous_gain += alpha * (desired_gain - previous_gain);
        envelope.push(previous_gain);
    }
    for frame in 0..attack_frames {
        let position = frame as f32 / attack_hop as f32;
        let index = (position as usize).min(envelope.len().saturating_sub(1));
        let fraction = position.fract();
        let start_gain = if index == 0 { 1.0 } else { envelope[index - 1] };
        let end_gain = envelope[index];
        let gain = start_gain + (end_gain - start_gain) * fraction;
        for sample in &mut audio.samples[frame * channels..(frame + 1) * channels] {
            *sample *= gain;
        }
    }

    let target_rms = 10.0f32.powf(NORMALIZE_TARGET_DBFS / 20.0);
    let normalization_db =
        (20.0 * (target_rms / sustain_rms).log10()).clamp(-24.0, NORMALIZE_MAX_BOOST_DB);
    let scale = 10.0f32.powf(normalization_db / 20.0);
    for sample in &mut audio.samples {
        *sample *= scale;
    }

    let compensated_peak = audio
        .samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    if compensated_peak > 0.98 {
        let safety_scale = 0.98 / compensated_peak;
        for sample in &mut audio.samples {
            *sample *= safety_scale;
        }
    }
    let peak_after = audio
        .samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    Ok(LevelingReport {
        applied: true,
        max_gain_db: normalization_db,
        peak_before,
        peak_after,
    })
}

fn median(values: &[f32]) -> f32 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[middle - 1] + sorted[middle]) * 0.5
    } else {
        sorted[middle]
    }
}
