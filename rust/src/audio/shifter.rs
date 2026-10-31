use std::error::Error;

use rubberband::{Options, Stretcher};

use super::wav::AudioBuffer;

const PROCESS_BLOCK_FRAMES: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchShiftVariant {
    ReferenceR3,
    R3HighQuality,
    R3HighSpeed,
    R3ShortWindow,
    R2Fast,
}

pub fn pitch_shift(
    audio: &AudioBuffer,
    pitch_scale: f64,
) -> Result<AudioBuffer, Box<dyn Error + Send + Sync>> {
    pitch_shift_with_variant(audio, pitch_scale, PitchShiftVariant::ReferenceR3)
}

pub fn pitch_shift_with_variant(
    audio: &AudioBuffer,
    pitch_scale: f64,
    variant: PitchShiftVariant,
) -> Result<AudioBuffer, Box<dyn Error + Send + Sync>> {
    if audio.sample_rate == 0
        || audio.channels == 0
        || audio.samples.is_empty()
        || audio.samples.len() % usize::from(audio.channels) != 0
    {
        return Err("cannot pitch-shift audio with an invalid format or empty samples".into());
    }
    if !pitch_scale.is_finite() || !(0.01..=100.0).contains(&pitch_scale) {
        return Err("pitch scale must be between 0.01 and 100".into());
    }
    if (pitch_scale - 1.0).abs() < 1e-12 {
        return Ok(audio.clone());
    }

    let frames = audio.frames();
    let expected_frames: u32 = frames
        .try_into()
        .map_err(|_| "audio is too long for Rubber Band's frame-count API")?;
    let channels = usize::from(audio.channels);
    let mut planar: Vec<Vec<f32>> = (0..channels).map(|_| Vec::with_capacity(frames)).collect();
    for frame in audio.samples.chunks_exact(channels) {
        for (channel, &sample) in frame.iter().enumerate() {
            planar[channel].push(sample);
        }
    }

    let engine_options = match variant {
        PitchShiftVariant::ReferenceR3 => Options::ENGINE_FINER | Options::PITCH_HIGH_CONSISTENCY,
        PitchShiftVariant::R3HighQuality => Options::ENGINE_FINER | Options::PITCH_HIGH_QUALITY,
        PitchShiftVariant::R3HighSpeed => Options::ENGINE_FINER | Options::PITCH_HIGH_SPEED,
        PitchShiftVariant::R3ShortWindow => {
            Options::ENGINE_FINER | Options::WINDOW_SHORT | Options::PITCH_HIGH_CONSISTENCY
        }
        PitchShiftVariant::R2Fast => Options::ENGINE_FASTER | Options::PITCH_HIGH_SPEED,
    };
    let expected_engine = if variant == PitchShiftVariant::R2Fast {
        2
    } else {
        3
    };
    let options = Options::PROCESS_OFFLINE
        | engine_options
        | Options::FORMANT_PRESERVED
        | Options::CHANNELS_TOGETHER;
    let mut stretcher = Stretcher::new(
        audio.sample_rate,
        u32::from(audio.channels),
        options,
        1.0,
        pitch_scale,
    );
    if stretcher.engine_version() != expected_engine {
        return Err(format!(
            "Rubber Band selected engine {}, expected R{expected_engine}",
            stretcher.engine_version(),
        )
        .into());
    }
    stretcher.set_expected_input_duraction(expected_frames);
    let block_frames = PROCESS_BLOCK_FRAMES.min(stretcher.process_size_limit() as usize);
    if block_frames == 0 {
        return Err("Rubber Band returned an invalid process-size limit".into());
    }
    stretcher.set_max_process_size(block_frames as u32);

    for start in (0..frames).step_by(block_frames) {
        let end = (start + block_frames).min(frames);
        let input: Vec<&[f32]> = planar.iter().map(|channel| &channel[start..end]).collect();
        stretcher.study(&input, end == frames);
    }

    let mut output: Vec<Vec<f32>> = (0..channels).map(|_| Vec::with_capacity(frames)).collect();
    let mut output_buffers = vec![vec![0.0f32; block_frames]; channels];
    for start in (0..frames).step_by(block_frames) {
        let end = (start + block_frames).min(frames);
        let input: Vec<&[f32]> = planar.iter().map(|channel| &channel[start..end]).collect();
        stretcher.process(&input, end == frames);
        loop {
            let Some(available) = stretcher.available() else {
                break;
            };
            if available == 0 {
                break;
            }
            let request = (available as usize).min(block_frames);
            let mut destinations: Vec<&mut [f32]> = output_buffers
                .iter_mut()
                .map(|channel| &mut channel[..request])
                .collect();
            let received = stretcher.retrieve(&mut destinations) as usize;
            if received == 0 {
                break;
            }
            for channel in 0..channels {
                output[channel].extend_from_slice(&output_buffers[channel][..received]);
            }
        }
    }

    let output_frames = output.first().map_or(0, Vec::len);
    if output.iter().any(|channel| channel.len() != output_frames) {
        return Err("Rubber Band returned different output lengths for channels".into());
    }
    if output_frames != frames {
        return Err(
            format!("Rubber Band returned {output_frames} frames; expected {frames}").into(),
        );
    }

    let mut samples = Vec::with_capacity(frames * channels);
    for frame in 0..frames {
        for channel in &output {
            let value = channel[frame];
            samples.push(if value.is_finite() { value } else { 0.0 });
        }
    }
    Ok(AudioBuffer {
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        samples,
    })
}
