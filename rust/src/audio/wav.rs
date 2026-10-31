use std::{
    error::Error,
    fs::{self, File},
    io::{BufWriter, Seek, SeekFrom, Write},
    path::Path,
    slice,
};

use hound::{SampleFormat, WavReader};

pub type AudioResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }
}

pub fn read_wav(path: impl AsRef<Path>) -> AudioResult<AudioBuffer> {
    let path = path.as_ref();
    let mut reader = match WavReader::open(path) {
        Ok(reader) => reader,
        Err(error) => {
            if let Some(audio) = read_float64_wav(path)? {
                return Ok(audio);
            }
            return Err(error.into());
        }
    };
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err("WAV must have at least one channel and a nonzero sample rate".into());
    }

    let samples = match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .map(|sample| sample.map(|value| if value.is_finite() { value } else { 0.0 }))
            .collect::<Result<Vec<_>, _>>()?,
        (SampleFormat::Int, 8) => reader
            .samples::<i8>()
            .map(|sample| sample.map(|value| f32::from(value) / 128.0))
            .collect::<Result<Vec<_>, _>>()?,
        (SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|sample| sample.map(|value| f32::from(value) / 32768.0))
            .collect::<Result<Vec<_>, _>>()?,
        (SampleFormat::Int, 24) => reader
            .samples::<i32>()
            .map(|sample| sample.map(|value| value as f32 / 8_388_608.0))
            .collect::<Result<Vec<_>, _>>()?,
        (SampleFormat::Int, 32) => reader
            .samples::<i32>()
            .map(|sample| sample.map(|value| value as f32 / 2_147_483_648.0))
            .collect::<Result<Vec<_>, _>>()?,
        (SampleFormat::Float, 64) => {
            drop(reader);
            return read_float64_wav(path)?.ok_or_else(|| "invalid 64-bit float WAV".into());
        }
        _ => {
            return Err(format!(
                "unsupported WAV format: {:?}, {} bits",
                spec.sample_format, spec.bits_per_sample
            )
            .into());
        }
    };

    if samples.len() % usize::from(spec.channels) != 0 {
        return Err("WAV sample count is not aligned to complete frames".into());
    }

    Ok(AudioBuffer {
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        samples,
    })
}

fn read_float64_wav(path: &Path) -> AudioResult<Option<AudioBuffer>> {
    let bytes = fs::read(path)?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Ok(None);
    }

    let mut format = None;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits = 0u16;
    let mut data_chunk = None;
    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let chunk_size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into()?) as usize;
        let start = offset + 8;
        let end = start
            .checked_add(chunk_size)
            .ok_or("WAV chunk size overflow")?;
        if end > bytes.len() {
            return Err("truncated WAV chunk".into());
        }
        match &bytes[offset..offset + 4] {
            b"fmt " if chunk_size >= 16 => {
                format = Some(u16::from_le_bytes(bytes[start..start + 2].try_into()?));
                channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into()?);
                sample_rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into()?);
                bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into()?);
            }
            b"data" => data_chunk = Some((start, chunk_size)),
            _ => {}
        }
        offset = end
            .checked_add(chunk_size & 1)
            .ok_or("WAV chunk offset overflow")?;
    }

    if format != Some(3) || bits != 64 {
        return Ok(None);
    }
    if channels == 0 || sample_rate == 0 {
        return Err("invalid 64-bit float WAV format".into());
    }
    let (start, data_size) = data_chunk.ok_or("WAV has no audio data")?;
    let bytes_per_frame = usize::from(channels) * 8;
    if data_size == 0 || data_size % bytes_per_frame != 0 {
        return Err("64-bit float WAV data is not aligned to complete frames".into());
    }
    let samples = bytes[start..start + data_size]
        .chunks_exact(8)
        .map(|sample| {
            let value = f64::from_le_bytes(sample.try_into().unwrap());
            if value.is_finite() && value.abs() <= f64::from(f32::MAX) {
                value as f32
            } else {
                0.0
            }
        })
        .collect();
    Ok(Some(AudioBuffer {
        sample_rate,
        channels,
        samples,
    }))
}

pub fn write_wav(path: impl AsRef<Path>, audio: &AudioBuffer) -> AudioResult<()> {
    write_wav_scaled(path, audio, 1.0)
}

pub(crate) fn write_wav_scaled(
    path: impl AsRef<Path>,
    audio: &AudioBuffer,
    gain: f32,
) -> AudioResult<()> {
    let mut writer =
        FloatWavWriter::create(path, audio.sample_rate, audio.channels, audio.frames())?;
    writer.write_samples(&audio.samples, gain)?;
    writer.finalize()?;
    Ok(())
}

pub(crate) struct FloatWavWriter {
    writer: BufWriter<File>,
    expected_samples: usize,
    written_samples: usize,
    channels: u16,
    cue_points: Vec<(u32, String)>,
}

impl FloatWavWriter {
    pub(crate) fn create(
        path: impl AsRef<Path>,
        sample_rate: u32,
        channels: u16,
        frames: usize,
    ) -> AudioResult<Self> {
        if sample_rate == 0 || channels == 0 {
            return Err("cannot write WAV with a zero sample rate or channel count".into());
        }
        let block_align = channels
            .checked_mul(4)
            .ok_or("WAV channel count is too large for float32 output")?;
        let byte_rate = sample_rate
            .checked_mul(u32::from(block_align))
            .ok_or("WAV byte rate exceeds the RIFF limit")?;
        let expected_samples = frames
            .checked_mul(usize::from(channels))
            .ok_or("WAV sample count overflow")?;
        let data_bytes = expected_samples
            .checked_mul(4)
            .filter(|bytes| *bytes <= u32::MAX as usize - 36)
            .ok_or("WAV data exceeds the RIFF 4 GB size limit")? as u32;

        let mut writer = BufWriter::new(File::create(path)?);
        writer.write_all(b"RIFF")?;
        writer.write_all(&(36u32 + data_bytes).to_le_bytes())?;
        writer.write_all(b"WAVEfmt ")?;
        writer.write_all(&16u32.to_le_bytes())?;
        writer.write_all(&3u16.to_le_bytes())?;
        writer.write_all(&channels.to_le_bytes())?;
        writer.write_all(&sample_rate.to_le_bytes())?;
        writer.write_all(&byte_rate.to_le_bytes())?;
        writer.write_all(&block_align.to_le_bytes())?;
        writer.write_all(&32u16.to_le_bytes())?;
        writer.write_all(b"data")?;
        writer.write_all(&data_bytes.to_le_bytes())?;
        Ok(Self {
            writer,
            expected_samples,
            written_samples: 0,
            channels,
            cue_points: Vec::new(),
        })
    }

    pub(crate) fn add_cue(&mut self, label: &str) -> AudioResult<()> {
        if label.as_bytes().contains(&0) {
            return Err("WAV cue labels cannot contain NUL bytes".into());
        }
        let channels = usize::from(self.channels);
        if self.written_samples % channels != 0 {
            return Err("WAV cue position is not aligned to a complete frame".into());
        }
        let frame = (self.written_samples / channels)
            .try_into()
            .map_err(|_| "WAV cue position exceeds the RIFF limit")?;
        self.cue_points.push((frame, label.to_owned()));
        Ok(())
    }

    pub(crate) fn write_samples(&mut self, samples: &[f32], gain: f32) -> AudioResult<()> {
        if !gain.is_finite() || gain < 0.0 {
            return Err("WAV gain must be finite and nonnegative".into());
        }
        let end = self
            .written_samples
            .checked_add(samples.len())
            .filter(|end| *end <= self.expected_samples)
            .ok_or("too many samples written to WAV")?;
        if gain == 1.0
            && cfg!(target_endian = "little")
            && samples.iter().all(|sample| sample.is_finite())
        {
            let bytes =
                unsafe { slice::from_raw_parts(samples.as_ptr().cast::<u8>(), samples.len() * 4) };
            self.writer.write_all(bytes)?;
        } else {
            for &sample in samples {
                let value = if sample.is_finite() {
                    sample * gain
                } else {
                    0.0
                };
                self.writer.write_all(&value.to_le_bytes())?;
            }
        }
        self.written_samples = end;
        Ok(())
    }

    pub(crate) fn write_silence(&mut self, samples: usize) -> AudioResult<()> {
        let end = self
            .written_samples
            .checked_add(samples)
            .filter(|end| *end <= self.expected_samples)
            .ok_or("too many samples written to WAV")?;
        const SILENCE: [u8; 16 * 1024] = [0; 16 * 1024];
        let mut remaining = samples * 4;
        while remaining > 0 {
            let count = remaining.min(SILENCE.len());
            self.writer.write_all(&SILENCE[..count])?;
            remaining -= count;
        }
        self.written_samples = end;
        Ok(())
    }

    pub(crate) fn finalize(mut self) -> AudioResult<()> {
        if self.written_samples != self.expected_samples {
            return Err("WAV ended before all declared samples were written".into());
        }
        self.writer.flush()?;
        if !self.cue_points.is_empty() {
            let cue_size = 4usize
                .checked_add(
                    self.cue_points
                        .len()
                        .checked_mul(24)
                        .ok_or("too many WAV cues")?,
                )
                .ok_or("too many WAV cues")?;
            let mut labels_size = 4usize;
            for (_, label) in &self.cue_points {
                let label_size = 4usize
                    .checked_add(label.len())
                    .and_then(|size| size.checked_add(1))
                    .ok_or("WAV cue label size overflow")?;
                labels_size = labels_size
                    .checked_add(8 + label_size + (label_size & 1))
                    .ok_or("WAV cue label size overflow")?;
            }
            let cue_chunk_bytes = 8usize
                .checked_add(cue_size)
                .ok_or("WAV cue chunk size overflow")?;
            let labels_chunk_bytes = 8usize
                .checked_add(labels_size)
                .ok_or("WAV cue label chunk size overflow")?;
            let data_bytes = self
                .expected_samples
                .checked_mul(4)
                .ok_or("WAV data size overflow")?;
            let riff_size: u32 = 36usize
                .checked_add(data_bytes)
                .and_then(|size| size.checked_add(cue_chunk_bytes))
                .and_then(|size| size.checked_add(labels_chunk_bytes))
                .filter(|size| *size <= u32::MAX as usize)
                .ok_or("WAV with cue markers exceeds the RIFF 4 GB size limit")?
                .try_into()?;

            let cue_count: u32 = self.cue_points.len().try_into()?;
            let cue_size: u32 = cue_size.try_into()?;
            let labels_size: u32 = labels_size.try_into()?;
            self.writer.write_all(b"cue ")?;
            self.writer.write_all(&cue_size.to_le_bytes())?;
            self.writer.write_all(&cue_count.to_le_bytes())?;
            for (index, (frame, _)) in self.cue_points.iter().enumerate() {
                let id: u32 = (index + 1).try_into()?;
                self.writer.write_all(&id.to_le_bytes())?;
                self.writer.write_all(&frame.to_le_bytes())?;
                self.writer.write_all(b"data")?;
                self.writer.write_all(&0u32.to_le_bytes())?;
                self.writer.write_all(&0u32.to_le_bytes())?;
                self.writer.write_all(&frame.to_le_bytes())?;
            }

            self.writer.write_all(b"LIST")?;
            self.writer.write_all(&labels_size.to_le_bytes())?;
            self.writer.write_all(b"adtl")?;
            for (index, (_, label)) in self.cue_points.iter().enumerate() {
                let id: u32 = (index + 1).try_into()?;
                let label_size: u32 = (4 + label.len() + 1).try_into()?;
                self.writer.write_all(b"labl")?;
                self.writer.write_all(&label_size.to_le_bytes())?;
                self.writer.write_all(&id.to_le_bytes())?;
                self.writer.write_all(label.as_bytes())?;
                self.writer.write_all(&[0])?;
                if label_size & 1 != 0 {
                    self.writer.write_all(&[0])?;
                }
            }
            self.writer.flush()?;
            self.writer.get_mut().seek(SeekFrom::Start(4))?;
            self.writer.get_mut().write_all(&riff_size.to_le_bytes())?;
        }
        self.writer.flush()?;
        Ok(())
    }
}
