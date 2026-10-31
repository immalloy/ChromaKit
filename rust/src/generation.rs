use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

use crate::audio::{
    leveling::{LevelingOptions, LevelingReport, auto_level_sustain, normalize_sustain},
    pitch::{PitchEstimate, PitchMethod, detect_pitch},
    shifter::pitch_shift,
    wav::{AudioBuffer, FloatWavWriter, read_wav, write_wav_scaled},
};

const MAX_NOTES: usize = 512;
const MAX_WORKERS: usize = 32;
const MIN_PITCH_CONFIDENCE: f32 = 0.65;
static NEXT_STAGE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug)]
pub struct GenerationSettings {
    pub input_folder: PathBuf,
    pub output_folder: PathBuf,
    pub start_midi: i32,
    pub range: usize,
    pub gap_seconds: f64,
    pub pitch_samples: bool,
    pub export_individual: bool,
    pub normalize_volume: bool,
    pub add_markers: bool,
    pub auto_level: bool,
    pub worker_count: usize,
}

#[derive(Clone, Debug)]
pub struct GenerationProgress {
    pub percent: u8,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct GenerationReport {
    pub chromatic_path: PathBuf,
    pub individual_paths: Vec<PathBuf>,
    pub source_pitches: Vec<Option<PitchEstimate>>,
    pub leveling: Vec<LevelingReport>,
    pub output_leveling: Vec<LevelingReport>,
    pub note_count: usize,
    pub workers: usize,
    pub sample_rate: u32,
    pub channels: u16,
    pub note_frames: Vec<usize>,
    pub combined_frames: usize,
    pub gap_frames: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerationError {
    Cancelled,
    Failed(String),
}

impl fmt::Display for GenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("generation cancelled"),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl Error for GenerationError {}

pub type GenerationResult<T> = Result<T, GenerationError>;

struct StageDirectory(PathBuf, bool);

enum RenderedAudio {
    Source(usize),
    Owned(AudioBuffer),
}

impl Drop for StageDirectory {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

pub fn midi_to_hz(midi: i32) -> f64 {
    440.0 * 2.0f64.powf((midi as f64 - 69.0) / 12.0)
}

fn midi_note_name(midi: i32) -> String {
    const NOTES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!(
        "{}{}",
        NOTES[midi.rem_euclid(12) as usize],
        midi.div_euclid(12) - 1
    )
}

pub fn generate(
    settings: &GenerationSettings,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(GenerationProgress),
) -> GenerationResult<GenerationReport> {
    validate_settings(settings)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(GenerationError::Cancelled);
    }
    if !settings.input_folder.is_dir() {
        return Err(GenerationError::Failed(format!(
            "sample folder does not exist: {}",
            settings.input_folder.display()
        )));
    }
    fs::create_dir_all(&settings.output_folder)
        .map_err(|error| fail(format!("cannot create output folder: {error}")))?;

    let inputs = numbered_inputs(&settings.input_folder)?;
    validate_generated_outputs(settings)?;

    let stage_path = create_stage(&settings.output_folder)?;
    let mut stage = StageDirectory(stage_path.clone(), false);
    let mut originals = Vec::with_capacity(inputs.len());
    let mut pitches = Vec::with_capacity(inputs.len());
    let mut leveling = Vec::with_capacity(inputs.len());

    for (index, path) in inputs.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GenerationError::Cancelled);
        }
        progress(GenerationProgress {
            percent: ((index * 15) / inputs.len()) as u8,
            message: format!(
                "Reading {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
        });
        let audio = read_wav(path)
            .map_err(|error| fail(format!("cannot read {}: {error}", path.display())))?;
        if let Some(first) = originals.first() {
            let first: &AudioBuffer = first;
            if audio.sample_rate != first.sample_rate || audio.channels != first.channels {
                return Err(fail(
                    "all numbered WAV files must use the same sample rate and channel count",
                ));
            }
        }
        let pitch = if settings.pitch_samples {
            let estimate = detect_pitch(&audio, PitchMethod::FftYin).map_err(|error| {
                fail(format!(
                    "cannot trust pitch estimate for {}: {error}",
                    path.display()
                ))
            })?;
            if estimate.confidence < MIN_PITCH_CONFIDENCE {
                return Err(fail(format!(
                    "pitch estimate for {} is uncertain ({:.2} confidence); use a clearer sample",
                    path.display(),
                    estimate.confidence
                )));
            }
            Some(estimate)
        } else {
            None
        };
        pitches.push(pitch);
        originals.push(audio);
        if settings.auto_level {
            let report =
                auto_level_sustain(originals.last_mut().unwrap(), LevelingOptions::default())
                    .map_err(|error| fail(format!("cannot level {}: {error}", path.display())))?;
            leveling.push(report);
        } else {
            leveling.push(LevelingReport::default());
        }
    }

    let sample_rate = originals[0].sample_rate;
    let channels = originals[0].channels;
    let note_frames: Vec<usize> = (0..settings.range)
        .map(|note| originals[note % originals.len()].frames())
        .collect();
    let gap_frames = (settings.gap_seconds * f64::from(sample_rate)).round() as usize;
    let marker_count = if settings.add_markers {
        settings.range
    } else {
        0
    };
    let combined_frames = validate_output_size(&note_frames, gap_frames, channels, marker_count)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(GenerationError::Cancelled);
    }

    let chromatic_stage_path = stage.0.join("chromatic.wav");
    let mut chromatic_writer = FloatWavWriter::create(
        &chromatic_stage_path,
        sample_rate,
        channels,
        combined_frames,
    )
    .map_err(|error| fail(format!("cannot create staged chromatic WAV: {error}")))?;
    let workers = worker_count(settings.worker_count, settings.range);
    let mut next_to_write = 0usize;
    let mut output_leveling = Vec::with_capacity(settings.range);
    let mut individual_stage_paths = Vec::with_capacity(if settings.export_individual {
        settings.range
    } else {
        0
    });

    for batch_start in (0..settings.range).step_by(workers) {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GenerationError::Cancelled);
        }
        let batch_end = (batch_start + workers).min(settings.range);
        let batch_size = batch_end - batch_start;
        let (sender, receiver) = mpsc::sync_channel::<(
            usize,
            Result<(RenderedAudio, LevelingReport), String>,
        )>(batch_size);
        let batch_result = thread::scope(|scope| {
            for note in batch_start..batch_end {
                let sender = sender.clone();
                let originals = &originals;
                let pitches = &pitches;
                let leveling = &leveling;
                let cancelled = cancelled;
                scope.spawn(move || {
                    let source_index = note % originals.len();
                    let result: Result<RenderedAudio, String> = if cancelled.load(Ordering::Relaxed)
                    {
                        Err("generation cancelled".to_owned())
                    } else {
                        let source = &originals[source_index];
                        if settings.pitch_samples {
                            let estimate = pitches[source_index]
                                .expect("pitch estimates were validated before rendering");
                            let target_hz = midi_to_hz(settings.start_midi + note as i32);
                            pitch_shift(source, target_hz / f64::from(estimate.hz))
                                .map(RenderedAudio::Owned)
                                .map_err(|error| {
                                    format!(
                                        "pitch shift failed for note {}: {error}",
                                        settings.start_midi + note as i32
                                    )
                                })
                        } else {
                            Ok(RenderedAudio::Source(source_index))
                        }
                    };
                    let result = result.and_then(|rendered| {
                        let mut rendered = match rendered {
                            RenderedAudio::Source(index) if settings.normalize_volume => {
                                RenderedAudio::Owned(originals[index].clone())
                            }
                            rendered => rendered,
                        };
                        let report = match &mut rendered {
                            RenderedAudio::Owned(audio) if settings.normalize_volume => {
                                normalize_sustain(audio).map_err(|error| {
                                    format!(
                                        "cannot normalize rendered note {}: {error}",
                                        settings.start_midi + note as i32
                                    )
                                })?
                            }
                            RenderedAudio::Owned(audio)
                                if settings.pitch_samples
                                    && settings.auto_level
                                    && leveling[source_index].applied =>
                            {
                                auto_level_sustain(audio, LevelingOptions::default()).map_err(
                                    |error| {
                                        format!(
                                            "cannot level rendered note {}: {error}",
                                            settings.start_midi + note as i32
                                        )
                                    },
                                )?
                            }
                            _ => LevelingReport::default(),
                        };
                        Ok((rendered, report))
                    });
                    let _ = sender.send((note, result));
                });
            }
            drop(sender);
            let mut batch = std::iter::repeat_with(|| None)
                .take(batch_size)
                .collect::<Vec<Option<Result<(RenderedAudio, LevelingReport), String>>>>();
            for (note, result) in receiver {
                batch[note - batch_start] = Some(result);
            }
            if cancelled.load(Ordering::Relaxed) {
                return Err(GenerationError::Cancelled);
            }
            for result in batch {
                let (rendered, level_report) = result
                    .ok_or_else(|| fail("a render worker exited without returning its result"))?
                    .map_err(fail)?;
                let audio = match &rendered {
                    RenderedAudio::Source(index) => &originals[*index],
                    RenderedAudio::Owned(audio) => audio,
                };
                output_leveling.push(level_report);
                let gain = peak_guard_gain(audio);
                if settings.export_individual {
                    let name = format!("pitched_{:03}.wav", next_to_write + 1);
                    let staged = stage.0.join(&name);
                    write_wav_scaled(&staged, audio, gain)
                        .map_err(|error| fail(format!("cannot write {name}: {error}")))?;
                    individual_stage_paths.push(staged);
                }
                if settings.add_markers {
                    chromatic_writer
                        .add_cue(&midi_note_name(settings.start_midi + next_to_write as i32))
                        .map_err(|error| fail(format!("cannot add note marker: {error}")))?;
                }
                chromatic_writer
                    .write_samples(&audio.samples, gain)
                    .map_err(|error| fail(format!("cannot write chromatic audio: {error}")))?;
                if next_to_write + 1 < settings.range {
                    let gap_samples = gap_frames
                        .checked_mul(usize::from(channels))
                        .ok_or_else(|| fail("chromatic gap size overflow"))?;
                    chromatic_writer
                        .write_silence(gap_samples)
                        .map_err(|error| fail(format!("cannot write chromatic gap: {error}")))?;
                }
                next_to_write += 1;
                progress(GenerationProgress {
                    percent: 15 + ((next_to_write * 80) / settings.range) as u8,
                    message: format!(
                        "Rendered {} ({next_to_write}/{})",
                        midi_note_name(settings.start_midi + next_to_write as i32 - 1),
                        settings.range
                    ),
                });
            }
            Ok::<(), GenerationError>(())
        });
        batch_result?;
    }
    if next_to_write != settings.range {
        return Err(fail("generation ended before all notes were rendered"));
    }
    chromatic_writer
        .finalize()
        .map_err(|error| fail(format!("cannot finalize chromatic WAV: {error}")))?;

    let mut staged_paths = individual_stage_paths;
    staged_paths.push(chromatic_stage_path);
    publish_staged_outputs(
        &staged_paths,
        &settings.output_folder,
        &mut stage,
        cancelled,
    )?;

    let chromatic_path = settings.output_folder.join("chromatic.wav");
    let individual_paths = if settings.export_individual {
        (1..=settings.range)
            .map(|index| {
                settings
                    .output_folder
                    .join(format!("pitched_{index:03}.wav"))
            })
            .collect()
    } else {
        Vec::new()
    };
    progress(GenerationProgress {
        percent: 100,
        message: format!("Finished: {}", chromatic_path.display()),
    });
    Ok(GenerationReport {
        chromatic_path,
        individual_paths,
        source_pitches: pitches,
        leveling,
        output_leveling,
        note_count: settings.range,
        workers,
        sample_rate,
        channels,
        note_frames,
        combined_frames,
        gap_frames,
    })
}

fn validate_settings(settings: &GenerationSettings) -> GenerationResult<()> {
    if !(1..=MAX_NOTES).contains(&settings.range) {
        return Err(fail("range must be between 1 and 512 notes"));
    }
    if !(12..=131).contains(&settings.start_midi)
        || settings.start_midi + settings.range as i32 - 1 > 131
    {
        return Err(fail("requested range must stay within C0 through B9"));
    }
    if !settings.gap_seconds.is_finite() || !(0.0..=60.0).contains(&settings.gap_seconds) {
        return Err(fail("gap must be between 0 and 60 seconds"));
    }
    if settings.worker_count > MAX_WORKERS {
        return Err(fail("worker count must be 0-32"));
    }
    Ok(())
}

fn numbered_inputs(folder: &Path) -> GenerationResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for number in 1..=MAX_NOTES {
        let path = folder.join(format!("{number}.wav"));
        if !path.is_file() {
            break;
        }
        paths.push(path);
    }
    if paths.is_empty() {
        return Err(fail(
            "no numbered WAV files found (expected 1.wav, 2.wav, ...)",
        ));
    }

    for entry in fs::read_dir(folder)
        .map_err(|error| fail(format!("cannot inspect sample folder: {error}")))?
    {
        let entry =
            entry.map_err(|error| fail(format!("cannot inspect sample folder: {error}")))?;
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
            && path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.parse::<usize>().is_ok_and(|n| n > paths.len()))
        {
            return Err(fail(
                "numbered WAV files must start at 1.wav and have no gaps",
            ));
        }
    }
    Ok(paths)
}

fn generated_output_paths(settings: &GenerationSettings) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(settings.range + usize::from(settings.export_individual));
    paths.push(settings.output_folder.join("chromatic.wav"));
    if settings.export_individual {
        paths.extend((1..=settings.range).map(|index| {
            settings
                .output_folder
                .join(format!("pitched_{index:03}.wav"))
        }));
    }
    paths
}

fn validate_generated_outputs(settings: &GenerationSettings) -> GenerationResult<()> {
    for path in generated_output_paths(settings) {
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => {
                return Err(fail(format!(
                    "generated output path is not a regular file: {}",
                    path.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(fail(format!(
                    "cannot inspect generated output {}: {error}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn publish_staged_outputs(
    staged_paths: &[PathBuf],
    output_folder: &Path,
    stage: &mut StageDirectory,
    cancelled: &AtomicBool,
) -> GenerationResult<()> {
    let mut published = Vec::with_capacity(staged_paths.len());
    let mut backups = Vec::with_capacity(staged_paths.len());
    let result = (|| {
        for (index, staged) in staged_paths.iter().enumerate() {
            if cancelled.load(Ordering::Relaxed) {
                return Err(GenerationError::Cancelled);
            }
            let name = staged
                .file_name()
                .ok_or_else(|| fail("staged output has no file name"))?;
            let final_path = output_folder.join(name);
            match fs::symlink_metadata(&final_path) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    let backup = stage.0.join(format!(".backup-{index}"));
                    fs::rename(&final_path, &backup).map_err(|error| {
                        fail(format!(
                            "cannot stage existing output {}: {error}",
                            final_path.display()
                        ))
                    })?;
                    backups.push((backup, final_path.clone()));
                }
                Ok(_) => {
                    return Err(fail(format!(
                        "generated output path is not a regular file: {}",
                        final_path.display()
                    )));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(fail(format!(
                        "cannot inspect generated output {}: {error}",
                        final_path.display()
                    )));
                }
            }
            fs::rename(staged, &final_path).map_err(|error| {
                fail(format!(
                    "cannot publish generated output {}: {error}",
                    final_path.display()
                ))
            })?;
            published.push(final_path);
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err(GenerationError::Cancelled);
        }
        Ok(())
    })();

    if let Err(error) = result {
        let mut rollback_errors = Vec::new();
        for path in published.iter().rev() {
            if let Err(rollback) = fs::remove_file(path) {
                if rollback.kind() != std::io::ErrorKind::NotFound {
                    rollback_errors.push(format!("cannot remove {}: {rollback}", path.display()));
                }
            }
        }
        for (backup, path) in backups.iter().rev() {
            if let Err(rollback) = fs::rename(backup, path) {
                rollback_errors.push(format!("cannot restore {}: {rollback}", path.display()));
            }
        }
        if rollback_errors.is_empty() {
            return Err(error);
        }
        stage.1 = true;
        return Err(fail(format!(
            "{error}; output rollback was incomplete: {}; recovery files retained in {}",
            rollback_errors.join("; "),
            stage.0.display()
        )));
    }

    for (backup, _) in backups {
        if let Ok(metadata) = fs::metadata(&backup)
            && metadata.permissions().readonly()
        {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(false);
            let _ = fs::set_permissions(&backup, permissions);
        }
    }
    Ok(())
}

fn create_stage(output: &Path) -> GenerationResult<PathBuf> {
    for _ in 0..100 {
        let id = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let path = output.join(format!(".chromakit-stage-{}-{id}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(fail(format!(
                    "cannot create output staging folder: {error}"
                )));
            }
        }
    }
    Err(fail("cannot create a unique output staging folder"))
}

fn worker_count(requested: usize, notes: usize) -> usize {
    let available = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(MAX_WORKERS);
    let automatic = if notes >= 48 {
        available.div_ceil(2)
    } else {
        available
    };
    (if requested == 0 { automatic } else { requested })
        .min(notes)
        .max(1)
}

fn validate_output_size(
    note_frames: &[usize],
    gap_frames: usize,
    channels: u16,
    marker_count: usize,
) -> GenerationResult<usize> {
    let frames = note_frames
        .iter()
        .try_fold(0usize, |sum, count| sum.checked_add(*count))
        .and_then(|sum| {
            sum.checked_add(gap_frames.checked_mul(note_frames.len().saturating_sub(1))?)
        })
        .ok_or_else(|| fail("requested output is too large"))?;
    let data_bytes = frames
        .checked_mul(usize::from(channels))
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| fail("requested output is too large"))?;
    let marker_bytes = if marker_count == 0 {
        0
    } else {
        24usize
            .checked_add(
                marker_count
                    .checked_mul(40)
                    .ok_or_else(|| fail("too many note markers"))?,
            )
            .ok_or_else(|| fail("too many note markers"))?
    };
    if data_bytes > (u32::MAX as usize - 36).saturating_sub(marker_bytes) {
        return Err(fail("combined WAV would exceed the RIFF 4 GB size limit"));
    }
    Ok(frames)
}

fn peak_guard_gain(audio: &AudioBuffer) -> f32 {
    let peak = audio
        .samples
        .iter()
        .filter(|sample| sample.is_finite())
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    if peak > 0.98 { 0.98 / peak } else { 1.0 }
}

fn fail(message: impl Into<String>) -> GenerationError {
    GenerationError::Failed(message.into())
}
