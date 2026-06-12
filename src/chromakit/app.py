from __future__ import annotations

from dataclasses import dataclass
import math
from pathlib import Path
import random
import struct
import subprocess
import sys
from typing import Callable, Iterable, Sequence

import numpy as np
import parselmouth
from PySide6.QtCore import QThread, Qt, Signal
from PySide6.QtGui import QDragEnterEvent, QDropEvent, QTextCursor
from PySide6.QtWidgets import (
	QApplication,
	QCheckBox,
	QComboBox,
	QDoubleSpinBox,
	QFileDialog,
	QFormLayout,
	QGridLayout,
	QGroupBox,
	QHBoxLayout,
	QLabel,
	QLineEdit,
	QMainWindow,
	QMessageBox,
	QProgressBar,
	QPushButton,
	QSpinBox,
	QTabWidget,
	QTextEdit,
	QVBoxLayout,
	QWidget,
)


NOTES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
OCTAVES = [str(value) for value in range(0, 9)]
SAMPLE_RATES = ["48000", "44100"]
ORDER_MODES = ["sequential", "shuffle", "random"]
DEFAULT_SAMPLE_RATE = 48000
LOW_NOTE_FFT_THRESHOLD = 60.0


@dataclass(frozen=True)
class GenerationSettings:
	sample_path: Path
	start_note_index: int
	start_octave: int
	semitones: int
	gap_seconds: float
	pitch_samples: bool
	dump_samples: bool
	order_mode: str
	trim_silence: bool
	normalize: bool
	fade_ms: int
	fixed_note_length: float
	output_sample_rate: int
	slicex_markers: bool


@dataclass(frozen=True)
class PrepareSettings:
	source_paths: tuple[Path, ...]
	output_dir: Path
	threshold_db: float
	min_region_ms: int
	min_silence_ms: int
	padding_ms: int
	output_sample_rate: int


@dataclass(frozen=True)
class SliceMarker:
	offset: int
	label: str


class CancelledError(Exception):
	pass


def list_source_files(folder: Path) -> list[Path]:
	numbered: list[Path] = []
	index = 1
	while (folder / f"{index}.wav").exists():
		numbered.append(folder / f"{index}.wav")
		index += 1
	if numbered:
		return numbered

	return [
		path
		for path in sorted(folder.glob("*.wav"))
		if path.name.lower() != "chromatic.wav"
	]


def note_label(start_note_index: int, start_octave: int, offset: int) -> str:
	total = start_note_index + offset
	return f"{NOTES[total % 12]}{start_octave + (total // 12)}"


def note_frequency(start_note_index: int, start_octave: int, offset: int) -> float:
	base_midi = (start_octave + 1) * 12 + start_note_index
	midi_note = base_midi + offset
	return 440.0 * math.pow(2.0, (midi_note - 69) / 12.0)


def make_silence(seconds: float, sample_rate: int) -> parselmouth.Sound:
	duration = max(0.0, float(seconds))
	return parselmouth.praat.call(
		"Create Sound from formula",
		"silence",
		1,
		0,
		duration,
		sample_rate,
		"0",
	)


def load_mono(path: Path, sample_rate: int = DEFAULT_SAMPLE_RATE) -> parselmouth.Sound:
	sound = parselmouth.Sound(str(path))
	sound = parselmouth.praat.call(sound, "Resample", sample_rate, 1)
	return parselmouth.praat.call(sound, "Convert to mono")


def resample_if_needed(sound: parselmouth.Sound, sample_rate: int) -> parselmouth.Sound:
	if int(sound.sampling_frequency) == int(sample_rate):
		return sound
	return parselmouth.praat.call(sound, "Resample", sample_rate, 1)


def peak_normalize(sound: parselmouth.Sound, target_peak: float = 0.98) -> parselmouth.Sound:
	values = np.asarray(sound.values, dtype=np.float64)
	peak = float(np.max(np.abs(values))) if values.size else 0.0
	if peak <= 1e-9:
		return sound
	return parselmouth.Sound(values * (target_peak / peak), sound.sampling_frequency)


def trim_edge_silence(sound: parselmouth.Sound, threshold_db: float = -40.0, padding_ms: int = 5) -> parselmouth.Sound:
	values = np.asarray(sound.values, dtype=np.float64)
	if values.size == 0:
		return sound
	mono = np.max(np.abs(values), axis=0)
	active = mono >= db_to_amplitude(threshold_db)
	active_indexes = np.flatnonzero(active)
	if active_indexes.size == 0:
		return sound
	padding = int(round(padding_ms * sound.sampling_frequency / 1000.0))
	start = max(0, int(active_indexes[0]) - padding)
	end = min(values.shape[1], int(active_indexes[-1]) + 1 + padding)
	if start == 0 and end == values.shape[1]:
		return sound
	return parselmouth.Sound(values[:, start:end], sound.sampling_frequency)


def apply_fade(sound: parselmouth.Sound, fade_ms: int) -> parselmouth.Sound:
	if fade_ms <= 0:
		return sound
	values = np.asarray(sound.values, dtype=np.float64).copy()
	total_frames = values.shape[1]
	fade_frames = min(int(round(fade_ms * sound.sampling_frequency / 1000.0)), total_frames // 2)
	if fade_frames <= 0:
		return sound
	fade_in = np.linspace(0.0, 1.0, fade_frames, endpoint=True)
	fade_out = np.linspace(1.0, 0.0, fade_frames, endpoint=True)
	values[:, :fade_frames] *= fade_in
	values[:, total_frames - fade_frames:] *= fade_out
	return parselmouth.Sound(values, sound.sampling_frequency)


def pad_or_trim(sound: parselmouth.Sound, length_seconds: float, sample_rate: int) -> parselmouth.Sound:
	if length_seconds <= 0:
		return sound
	target_frames = max(0, int(round(length_seconds * sample_rate)))
	current = resample_if_needed(sound, sample_rate)
	current_frames = int(current.get_number_of_samples())
	if current_frames == target_frames:
		return current
	if current_frames > target_frames:
		end_time = target_frames / float(sample_rate)
		return parselmouth.praat.call(current, "Extract part", 0, end_time, "rectangular", 1, "yes")

	padding = make_silence((target_frames - current_frames) / float(sample_rate), sample_rate)
	return parselmouth.Sound.concatenate([current, padding])


def retune_sound(sound: parselmouth.Sound, target_frequency: float) -> parselmouth.Sound:
	if target_frequency < LOW_NOTE_FFT_THRESHOLD:
		retuned = retune_with_fft(sound, target_frequency)
		if retuned is not None:
			return retuned
	return retune_with_praat(sound, target_frequency)


def retune_with_praat(sound: parselmouth.Sound, target_frequency: float) -> parselmouth.Sound:
	pitch_floor = 37.5 if target_frequency < LOW_NOTE_FFT_THRESHOLD else 60.0
	pitch_ceiling = 1200.0 if target_frequency < LOW_NOTE_FFT_THRESHOLD else 600.0
	manipulation = parselmouth.praat.call(sound, "To Manipulation", 0.03, pitch_floor, pitch_ceiling)
	pitch_tier = parselmouth.praat.call(manipulation, "Extract pitch tier")
	parselmouth.praat.call(pitch_tier, "Remove points between", sound.xmin, sound.xmax)
	parselmouth.praat.call(pitch_tier, "Add point", sound.xmin, target_frequency)
	parselmouth.praat.call(pitch_tier, "Add point", max(sound.xmax, sound.xmin + 0.000001), target_frequency)
	parselmouth.praat.call([pitch_tier, manipulation], "Replace pitch tier")
	return parselmouth.praat.call(manipulation, "Get resynthesis (overlap-add)")


def retune_with_fft(sound: parselmouth.Sound, target_frequency: float) -> parselmouth.Sound | None:
	try:
		pitch = parselmouth.praat.call(sound, "To Pitch", 0.0, 37.5, 200.0)
		current = float(parselmouth.praat.call(pitch, "Get quantile", 0, 0, 0.5, "Hertz"))
	except Exception:
		return None

	if not current or math.isnan(current):
		return None
	ratio = target_frequency / current
	if not math.isfinite(ratio) or ratio <= 0:
		return None
	if abs(math.log2(ratio)) < 0.0001:
		return sound

	values = np.asarray(sound.values, dtype=np.float64)
	sample_rate = int(sound.sampling_frequency)
	_, frames = values.shape
	padded_length = max(frames * 8, 1)
	frequencies = np.fft.rfftfreq(padded_length, d=1.0 / sample_rate)
	channels = []

	for channel in values:
		padded = np.pad(channel, (0, padded_length - frames))
		spectrum = np.fft.rfft(padded)
		magnitude = np.abs(spectrum)
		phase = np.angle(spectrum)
		scaled = frequencies / ratio
		shifted = np.interp(scaled, frequencies, magnitude, left=0.0, right=0.0)
		shifted_phase = np.interp(scaled, frequencies, phase, left=0.0, right=0.0)
		rendered = np.fft.irfft(shifted * np.exp(1j * shifted_phase), padded_length)
		channels.append(rendered[:frames])

	return parselmouth.Sound(np.clip(np.vstack(channels), -1.0, 1.0), sample_rate)


def sound_to_int16(sound: parselmouth.Sound, sample_rate: int) -> bytes:
	rendered = resample_if_needed(sound, sample_rate)
	values = np.asarray(rendered.values, dtype=np.float64)
	if values.ndim == 1:
		values = values.reshape(1, -1)
	clipped = np.clip(values, -1.0, 1.0)
	pcm = np.rint(clipped * 32767.0).astype("<i2")
	interleaved = np.ascontiguousarray(np.transpose(pcm))
	return interleaved.tobytes()


def pack_chunk(chunk_id: bytes, body: bytes) -> bytes:
	chunk = chunk_id + struct.pack("<I", len(body)) + body
	if len(body) % 2:
		chunk += b"\x00"
	return chunk


class WavStreamWriter:
	def __init__(self, path: Path, sample_rate: int, channels: int = 1) -> None:
		self.path = path
		self.sample_rate = sample_rate
		self.channels = channels
		self.data_size = 0
		self.handle = path.open("wb")
		self.handle.write(b"RIFF\x00\x00\x00\x00WAVE")
		byte_rate = sample_rate * channels * 2
		block_align = channels * 2
		fmt_body = struct.pack("<HHIIHH", 1, channels, sample_rate, byte_rate, block_align, 16)
		self.handle.write(pack_chunk(b"fmt ", fmt_body))
		self.data_size_pos = self.handle.tell() + 4
		self.handle.write(b"data\x00\x00\x00\x00")

	def write_sound(self, sound: parselmouth.Sound) -> int:
		data = sound_to_int16(sound, self.sample_rate)
		self.handle.write(data)
		self.data_size += len(data)
		return len(data) // (self.channels * 2)

	def close(self, markers: Sequence[SliceMarker] = ()) -> None:
		if self.data_size % 2:
			self.handle.write(b"\x00")
		if markers:
			self.handle.write(build_marker_chunks(markers))
		file_size = self.handle.tell() - 8
		self.handle.seek(4)
		self.handle.write(struct.pack("<I", file_size))
		self.handle.seek(self.data_size_pos)
		self.handle.write(struct.pack("<I", self.data_size))
		self.handle.close()


def build_marker_chunks(markers: Sequence[SliceMarker]) -> bytes:
	cue_body = struct.pack("<I", len(markers))
	list_body = b"adtl"
	for cue_id, marker in enumerate(markers, start=1):
		offset = max(0, int(marker.offset))
		cue_body += struct.pack("<II4sIII", cue_id, offset, b"data", 0, 0, offset)
		label_body = struct.pack("<I", cue_id) + marker.label.encode("utf-8") + b"\x00"
		list_body += pack_chunk(b"labl", label_body)
	return pack_chunk(b"cue ", cue_body) + pack_chunk(b"LIST", list_body)


def ordered_sample_indexes(count: int, total: int, mode: str) -> list[int]:
	if mode == "random":
		return [random.randrange(count) for _ in range(total)]
	order = list(range(count))
	if mode == "shuffle":
		random.shuffle(order)
	return [order[index % count] for index in range(total)]


def generate_chromatic(
	settings: GenerationSettings,
	on_progress: Callable[[int, int, str], None],
	on_log: Callable[[str], None],
	should_cancel: Callable[[], bool],
) -> Path:
	if not settings.sample_path.is_dir():
		raise ValueError("Choose a valid sample folder.")

	files = list_source_files(settings.sample_path)
	if not files:
		raise ValueError("No WAV files found in the folder.")

	output_path = settings.sample_path / "chromatic.wav"
	gap = make_silence(settings.gap_seconds, DEFAULT_SAMPLE_RATE) if settings.gap_seconds > 0 else None
	indexes = ordered_sample_indexes(len(files), settings.semitones, settings.order_mode)
	writer = WavStreamWriter(output_path, settings.output_sample_rate, 1)
	markers: list[SliceMarker] = []
	current_offset = 0
	dump_dir: Path | None = None

	if settings.dump_samples:
		dump_dir = settings.sample_path / ("pitched_samples" if settings.pitch_samples else "samples")
		dump_dir.mkdir(exist_ok=True)

	try:
		on_log(f"Found {len(files)} source WAV file(s).")
		on_log(
			f"Semitones: {settings.semitones} | Gap: {settings.gap_seconds:.3f}s | "
			f"Order: {settings.order_mode} | Output: {settings.output_sample_rate} Hz"
		)
		for offset, source_index in enumerate(indexes):
			if should_cancel():
				raise CancelledError()

			source_path = files[source_index]
			label = note_label(settings.start_note_index, settings.start_octave, offset)
			on_log(f"[{offset + 1}/{settings.semitones}] Loading {source_path.name} -> {label}")
			sound = load_mono(source_path, DEFAULT_SAMPLE_RATE)

			if settings.trim_silence:
				sound = trim_edge_silence(sound)
				on_log("  trimmed silence")
			if settings.normalize:
				sound = peak_normalize(sound)
				on_log("  normalized")
			if settings.pitch_samples:
				sound = retune_sound(sound, note_frequency(settings.start_note_index, settings.start_octave, offset))
				on_log("  pitched")
			if settings.fade_ms > 0:
				sound = apply_fade(sound, settings.fade_ms)
				on_log(f"  fade {settings.fade_ms} ms")
			if settings.fixed_note_length > 0:
				sound = pad_or_trim(sound, settings.fixed_note_length, DEFAULT_SAMPLE_RATE)
				on_log(f"  fixed length {settings.fixed_note_length:.3f}s")

			if settings.slicex_markers:
				markers.append(SliceMarker(current_offset, label))

			frames = writer.write_sound(sound)
			current_offset += frames

			if dump_dir is not None:
				dump_path = dump_dir / f"note_{offset + 1}.wav"
				resample_if_needed(sound, settings.output_sample_rate).save(str(dump_path), "WAV")

			if gap is not None and offset < settings.semitones - 1:
				current_offset += writer.write_sound(gap)

			on_progress(offset + 1, settings.semitones, label)

		writer.close(markers if settings.slicex_markers else ())
	except Exception:
		if not writer.handle.closed:
			try:
				writer.close(())
			except Exception:
				writer.handle.close()
		raise

	on_log(f"Saved: {output_path}")
	return output_path


def db_to_amplitude(db_value: float) -> float:
	return math.pow(10.0, db_value / 20.0)


def find_audio_regions(
	values: np.ndarray,
	sample_rate: int,
	threshold_db: float,
	min_region_ms: int,
	min_silence_ms: int,
	padding_ms: int,
) -> list[tuple[int, int]]:
	if values.size == 0:
		return []

	threshold = db_to_amplitude(threshold_db)
	min_region = max(1, int(round(min_region_ms * sample_rate / 1000.0)))
	min_silence = max(1, int(round(min_silence_ms * sample_rate / 1000.0)))
	padding = max(0, int(round(padding_ms * sample_rate / 1000.0)))
	active = np.abs(values) >= threshold
	regions: list[tuple[int, int]] = []
	start: int | None = None
	last_active: int | None = None

	for index, is_active in enumerate(active):
		if is_active:
			if start is None:
				start = index
			last_active = index
		elif start is not None and last_active is not None and index - last_active >= min_silence:
			if last_active - start + 1 >= min_region:
				regions.append((max(0, start - padding), min(len(values), last_active + 1 + padding)))
			start = None
			last_active = None

	if start is not None and last_active is not None and last_active - start + 1 >= min_region:
		regions.append((max(0, start - padding), min(len(values), last_active + 1 + padding)))

	return regions


def prepare_samples(
	settings: PrepareSettings,
	on_progress: Callable[[int, int, str], None],
	on_log: Callable[[str], None],
	should_cancel: Callable[[], bool],
) -> Path:
	sources = [path for path in settings.source_paths if path.suffix.lower() == ".wav" and path.is_file()]
	if not sources:
		raise ValueError("Choose one or more WAV files or a folder containing WAV files.")

	settings.output_dir.mkdir(exist_ok=True)
	output_index = 1
	total_sources = len(sources)

	for source_number, source in enumerate(sources, start=1):
		if should_cancel():
			raise CancelledError()
		on_log(f"[{source_number}/{total_sources}] Scanning {source.name}")
		sound = load_mono(source, settings.output_sample_rate)
		values = np.asarray(sound.values[0], dtype=np.float64)
		regions = find_audio_regions(
			values,
			settings.output_sample_rate,
			settings.threshold_db,
			settings.min_region_ms,
			settings.min_silence_ms,
			settings.padding_ms,
		)
		if not regions:
			on_log("  no regions found")
			on_progress(source_number, total_sources, source.name)
			continue

		for start, end in regions:
			if should_cancel():
				raise CancelledError()
			part_values = values[start:end].reshape(1, -1)
			part = parselmouth.Sound(part_values, settings.output_sample_rate)
			output = settings.output_dir / f"{output_index}.wav"
			part.save(str(output), "WAV")
			on_log(f"  wrote {output.name}")
			output_index += 1

		on_progress(source_number, total_sources, source.name)

	if output_index == 1:
		raise ValueError("No non-silent sample regions were found.")

	on_log(f"Prepared {output_index - 1} sample(s) in: {settings.output_dir}")
	return settings.output_dir


class GenerationWorker(QThread):
	progress = Signal(int, int, str)
	log = Signal(str)
	done = Signal(str)
	failed = Signal(str)
	cancelled = Signal(str)

	def __init__(self, settings: GenerationSettings) -> None:
		super().__init__()
		self.settings = settings
		self._cancel = False

	def request_cancel(self) -> None:
		self._cancel = True

	def run(self) -> None:
		try:
			output = generate_chromatic(self.settings, self.progress.emit, self.log.emit, lambda: self._cancel)
		except CancelledError:
			self.cancelled.emit("Generation cancelled.")
		except Exception as error:
			self.failed.emit(str(error))
		else:
			self.done.emit(str(output))


class PrepareWorker(QThread):
	progress = Signal(int, int, str)
	log = Signal(str)
	done = Signal(str)
	failed = Signal(str)
	cancelled = Signal(str)

	def __init__(self, settings: PrepareSettings) -> None:
		super().__init__()
		self.settings = settings
		self._cancel = False

	def request_cancel(self) -> None:
		self._cancel = True

	def run(self) -> None:
		try:
			output = prepare_samples(self.settings, self.progress.emit, self.log.emit, lambda: self._cancel)
		except CancelledError:
			self.cancelled.emit("Sample preparation cancelled.")
		except Exception as error:
			self.failed.emit(str(error))
		else:
			self.done.emit(str(output))


class GeneratorWindow(QMainWindow):
	def __init__(self) -> None:
		super().__init__()
		self.setWindowTitle("ChromaKit")
		self.setMinimumSize(920, 620)
		self.setAcceptDrops(True)

		self.worker: GenerationWorker | PrepareWorker | None = None
		self.last_output_path: Path | None = None
		self.prepare_sources: tuple[Path, ...] = ()

		self.tabs = QTabWidget()
		self.setCentralWidget(self.tabs)
		self.tabs.addTab(self.build_generate_tab(), "Generate")
		self.tabs.addTab(self.build_prepare_tab(), "Prepare Samples")

		self.statusBar().showMessage("Idle")
		self.refresh_generation_validation()

	def build_generate_tab(self) -> QWidget:
		tab = QWidget()
		layout = QVBoxLayout(tab)
		layout.setContentsMargins(16, 14, 16, 14)
		layout.setSpacing(10)

		title = QLabel("ChromaKit")
		font = title.font()
		font.setPointSize(18)
		font.setBold(True)
		title.setFont(font)
		layout.addWidget(title)

		content = QHBoxLayout()
		content.setSpacing(14)
		layout.addLayout(content, 1)

		controls = QVBoxLayout()
		controls.setSpacing(10)
		content.addLayout(controls, 0)

		self.folder_input = QLineEdit()
		self.folder_input.setPlaceholderText("Choose or drop a folder with WAV samples")
		self.folder_input.textChanged.connect(self.refresh_generation_validation)
		self.browse_button = QPushButton("Browse")
		self.browse_button.clicked.connect(self.choose_folder)
		folder_row = QHBoxLayout()
		folder_row.addWidget(self.folder_input)
		folder_row.addWidget(self.browse_button)

		self.validation_label = QLabel("")
		self.validation_label.setStyleSheet("color: #a33;")

		source_group = QGroupBox("Source")
		source_layout = QVBoxLayout(source_group)
		source_layout.addLayout(folder_row)
		source_layout.addWidget(self.validation_label)
		controls.addWidget(source_group)

		self.start_note_input = QComboBox()
		self.start_note_input.addItems(NOTES)
		self.start_octave_input = QComboBox()
		self.start_octave_input.addItems(OCTAVES)
		self.start_octave_input.setCurrentText("2")
		self.range_input = QLineEdit("24")
		self.range_input.textChanged.connect(self.refresh_generation_validation)
		self.gap_input = QLineEdit("0.1")
		self.gap_input.textChanged.connect(self.refresh_generation_validation)
		self.order_input = QComboBox()
		self.order_input.addItems(ORDER_MODES)

		pitch_group = QGroupBox("Pitch and Range")
		form = QFormLayout(pitch_group)
		form.setLabelAlignment(Qt.AlignRight)
		form.setFieldGrowthPolicy(QFormLayout.AllNonFixedFieldsGrow)
		form.addRow("Starting note:", self.start_note_input)
		form.addRow("Starting octave:", self.start_octave_input)
		form.addRow("Range:", self.range_input)
		form.addRow("Sample gap:", self.gap_input)
		form.addRow("Sample order:", self.order_input)
		controls.addWidget(pitch_group)

		self.pitch_input = QCheckBox("Pitch samples")
		self.pitch_input.setChecked(True)
		self.dump_input = QCheckBox("Dump individual samples")
		self.dump_input.setChecked(True)
		self.trim_silence_input = QCheckBox("Trim silence from samples")
		self.normalize_input = QCheckBox("Peak normalize before pitch")
		self.slicex_input = QCheckBox("Embed FL Studio Slicex markers")
		self.fade_input = QLineEdit("0")
		self.fixed_length_input = QLineEdit("0")
		self.sample_rate_input = QComboBox()
		self.sample_rate_input.addItems(SAMPLE_RATES)

		options_group = QGroupBox("Processing")
		options = QGridLayout(options_group)
		options.addWidget(self.pitch_input, 0, 0)
		options.addWidget(self.dump_input, 0, 1)
		options.addWidget(self.trim_silence_input, 1, 0)
		options.addWidget(self.normalize_input, 1, 1)
		options.addWidget(self.slicex_input, 2, 0, 1, 2)
		options.addWidget(QLabel("Fade in/out (ms):"), 3, 0)
		options.addWidget(self.fade_input, 3, 1)
		options.addWidget(QLabel("Fixed note length (s):"), 4, 0)
		options.addWidget(self.fixed_length_input, 4, 1)
		options.addWidget(QLabel("Output sample rate:"), 5, 0)
		options.addWidget(self.sample_rate_input, 5, 1)
		controls.addWidget(options_group)

		self.generate_button = QPushButton("Generate Chromatic")
		self.generate_button.clicked.connect(self.generate)
		self.cancel_button = QPushButton("Cancel")
		self.cancel_button.setEnabled(False)
		self.cancel_button.clicked.connect(self.cancel_worker)
		self.open_output_button = QPushButton("Open Output")
		self.open_output_button.setEnabled(False)
		self.open_output_button.clicked.connect(self.open_last_output)

		button_row = QHBoxLayout()
		button_row.addWidget(self.generate_button)
		button_row.addWidget(self.cancel_button)
		button_row.addWidget(self.open_output_button)
		controls.addLayout(button_row)
		controls.addStretch(1)

		self.progress = QProgressBar()
		self.progress.setRange(0, 1)
		self.progress.setValue(0)
		self.log_output = QTextEdit()
		self.log_output.setReadOnly(True)
		self.log_output.setPlaceholderText("Generation logs will appear here.")

		output_group = QGroupBox("Output")
		output_layout = QVBoxLayout(output_group)
		output_layout.addWidget(self.progress)
		output_layout.addWidget(self.log_output, 1)
		content.addWidget(output_group, 1)

		return tab

	def build_prepare_tab(self) -> QWidget:
		tab = QWidget()
		layout = QVBoxLayout(tab)
		layout.setContentsMargins(16, 14, 16, 14)
		layout.setSpacing(10)

		content = QHBoxLayout()
		content.setSpacing(14)
		layout.addLayout(content, 1)

		controls = QVBoxLayout()
		controls.setSpacing(10)
		content.addLayout(controls, 0)

		self.prepare_source_input = QLineEdit()
		self.prepare_source_input.setReadOnly(True)
		self.prepare_source_input.setPlaceholderText("Choose WAV files or a folder to split by silence")
		self.prepare_files_button = QPushButton("Choose WAV Files")
		self.prepare_files_button.clicked.connect(self.choose_prepare_files)
		self.prepare_folder_button = QPushButton("Choose Folder")
		self.prepare_folder_button.clicked.connect(self.choose_prepare_folder)
		source_row = QHBoxLayout()
		source_row.addWidget(self.prepare_source_input)
		source_row.addWidget(self.prepare_files_button)
		source_row.addWidget(self.prepare_folder_button)
		source_group = QGroupBox("Source")
		source_layout = QVBoxLayout(source_group)
		source_layout.addLayout(source_row)
		controls.addWidget(source_group)

		self.threshold_input = QDoubleSpinBox()
		self.threshold_input.setRange(-90.0, -1.0)
		self.threshold_input.setValue(-40.0)
		self.threshold_input.setSuffix(" dB")
		self.min_region_input = QSpinBox()
		self.min_region_input.setRange(1, 5000)
		self.min_region_input.setValue(80)
		self.min_region_input.setSuffix(" ms")
		self.min_silence_input = QSpinBox()
		self.min_silence_input.setRange(1, 5000)
		self.min_silence_input.setValue(120)
		self.min_silence_input.setSuffix(" ms")
		self.padding_input = QSpinBox()
		self.padding_input.setRange(0, 1000)
		self.padding_input.setValue(20)
		self.padding_input.setSuffix(" ms")
		self.prepare_sample_rate_input = QComboBox()
		self.prepare_sample_rate_input.addItems(SAMPLE_RATES)

		prepare_group = QGroupBox("Silence Detection")
		prepare_form = QFormLayout(prepare_group)
		prepare_form.setFieldGrowthPolicy(QFormLayout.AllNonFixedFieldsGrow)
		prepare_form.addRow("Silence threshold:", self.threshold_input)
		prepare_form.addRow("Minimum sample length:", self.min_region_input)
		prepare_form.addRow("Minimum silence gap:", self.min_silence_input)
		prepare_form.addRow("Padding:", self.padding_input)
		prepare_form.addRow("Output sample rate:", self.prepare_sample_rate_input)
		controls.addWidget(prepare_group)

		self.prepare_button = QPushButton("Prepare Samples")
		self.prepare_button.setEnabled(False)
		self.prepare_button.clicked.connect(self.prepare)
		self.prepare_cancel_button = QPushButton("Cancel")
		self.prepare_cancel_button.setEnabled(False)
		self.prepare_cancel_button.clicked.connect(self.cancel_worker)
		self.prepare_open_button = QPushButton("Open Prepared Folder")
		self.prepare_open_button.setEnabled(False)
		self.prepare_open_button.clicked.connect(self.open_last_output)

		prepare_buttons = QHBoxLayout()
		prepare_buttons.addWidget(self.prepare_button)
		prepare_buttons.addWidget(self.prepare_cancel_button)
		prepare_buttons.addWidget(self.prepare_open_button)
		controls.addLayout(prepare_buttons)
		controls.addStretch(1)

		self.prepare_progress = QProgressBar()
		self.prepare_progress.setRange(0, 1)
		self.prepare_log_output = QTextEdit()
		self.prepare_log_output.setReadOnly(True)
		self.prepare_log_output.setPlaceholderText("Preparation logs will appear here.")
		prepare_output_group = QGroupBox("Output")
		prepare_output_layout = QVBoxLayout(prepare_output_group)
		prepare_output_layout.addWidget(self.prepare_progress)
		prepare_output_layout.addWidget(self.prepare_log_output, 1)
		content.addWidget(prepare_output_group, 1)

		return tab

	def dragEnterEvent(self, event: QDragEnterEvent) -> None:
		if event.mimeData().hasUrls():
			event.acceptProposedAction()

	def dropEvent(self, event: QDropEvent) -> None:
		urls = event.mimeData().urls()
		if not urls:
			return
		path = Path(urls[0].toLocalFile())
		if path.is_dir():
			if self.tabs.currentIndex() == 1:
				self.set_prepare_sources(tuple(sorted(path.glob("*.wav"))), path / "prepared_samples")
			else:
				self.folder_input.setText(str(path))
		elif path.suffix.lower() == ".wav" and self.tabs.currentIndex() == 1:
			self.set_prepare_sources(tuple(Path(url.toLocalFile()) for url in urls), path.parent / "prepared_samples")

	def choose_folder(self) -> None:
		folder = QFileDialog.getExistingDirectory(self, "Select sample folder")
		if folder:
			self.folder_input.setText(folder)

	def choose_prepare_files(self) -> None:
		files, _ = QFileDialog.getOpenFileNames(self, "Choose WAV files", "", "WAV files (*.wav)")
		if files:
			paths = tuple(Path(path) for path in files)
			self.set_prepare_sources(paths, paths[0].parent / "prepared_samples")

	def choose_prepare_folder(self) -> None:
		folder = QFileDialog.getExistingDirectory(self, "Choose folder with WAV files")
		if folder:
			path = Path(folder)
			self.set_prepare_sources(tuple(sorted(path.glob("*.wav"))), path / "prepared_samples")

	def set_prepare_sources(self, paths: tuple[Path, ...], output_dir: Path) -> None:
		self.prepare_sources = tuple(path for path in paths if path.suffix.lower() == ".wav")
		self.prepare_output_dir = output_dir
		if len(self.prepare_sources) == 1:
			self.prepare_source_input.setText(str(self.prepare_sources[0]))
		else:
			self.prepare_source_input.setText(f"{len(self.prepare_sources)} WAV file(s) -> {output_dir}")
		self.prepare_button.setEnabled(bool(self.prepare_sources) and self.worker is None)

	def parse_generation_settings(self) -> GenerationSettings:
		folder = Path(self.folder_input.text().strip())
		try:
			semitones = int(self.range_input.text().strip())
			gap_seconds = float(self.gap_input.text().strip())
			fade_ms = int(float(self.fade_input.text().strip()))
			fixed_length = float(self.fixed_length_input.text().strip())
		except ValueError as error:
			raise ValueError("Range, gap, fade, and fixed length must be valid numbers.") from error

		if semitones < 1 or semitones > 128:
			raise ValueError("Range must be between 1 and 128.")
		if gap_seconds < 0:
			raise ValueError("Sample gap cannot be negative.")
		if fade_ms < 0:
			raise ValueError("Fade cannot be negative.")
		if fixed_length < 0:
			raise ValueError("Fixed note length cannot be negative.")

		return GenerationSettings(
			sample_path=folder,
			start_note_index=self.start_note_input.currentIndex(),
			start_octave=int(self.start_octave_input.currentText()),
			semitones=semitones,
			gap_seconds=gap_seconds,
			pitch_samples=self.pitch_input.isChecked(),
			dump_samples=self.dump_input.isChecked(),
			order_mode=self.order_input.currentText(),
			trim_silence=self.trim_silence_input.isChecked(),
			normalize=self.normalize_input.isChecked(),
			fade_ms=fade_ms,
			fixed_note_length=fixed_length,
			output_sample_rate=int(self.sample_rate_input.currentText()),
			slicex_markers=self.slicex_input.isChecked(),
		)

	def refresh_generation_validation(self) -> None:
		message = ""
		enabled = True
		folder_text = self.folder_input.text().strip()
		if not folder_text:
			enabled = False
		else:
			folder = Path(folder_text)
			if not folder.is_dir():
				message = "Folder not found."
				enabled = False
			elif not list_source_files(folder):
				message = "No WAV files found."
				enabled = False

		try:
			if self.range_input.text().strip():
				value = int(self.range_input.text().strip())
				if value < 1 or value > 128:
					message = "Range must be between 1 and 128."
					enabled = False
			if self.gap_input.text().strip() and float(self.gap_input.text().strip()) < 0:
				message = "Sample gap cannot be negative."
				enabled = False
		except ValueError:
			message = "Range and sample gap must be valid numbers."
			enabled = False

		self.validation_label.setText(message)
		self.generate_button.setEnabled(enabled and self.worker is None)

	def generate(self) -> None:
		try:
			settings = self.parse_generation_settings()
		except Exception as error:
			QMessageBox.critical(self, "ChromaKit", str(error))
			return

		output_path = settings.sample_path / "chromatic.wav"
		if output_path.exists():
			answer = QMessageBox.question(
				self,
				"ChromaKit",
				"'chromatic.wav' already exists. Overwrite it?",
				QMessageBox.Yes | QMessageBox.No,
				QMessageBox.No,
			)
			if answer != QMessageBox.Yes:
				self.statusBar().showMessage("Generation cancelled.")
				return

		self.log_output.clear()
		self.progress.setRange(0, settings.semitones)
		self.progress.setValue(0)
		self.start_worker(GenerationWorker(settings), "Generating...")

	def prepare(self) -> None:
		if not self.prepare_sources:
			QMessageBox.critical(self, "ChromaKit", "Choose WAV files or a folder first.")
			return
		output_dir = getattr(self, "prepare_output_dir", self.prepare_sources[0].parent / "prepared_samples")
		if output_dir.exists() and any(output_dir.glob("*.wav")):
			answer = QMessageBox.question(
				self,
				"ChromaKit",
				"'prepared_samples' already contains WAV files. Overwrite matching numbered files?",
				QMessageBox.Yes | QMessageBox.No,
				QMessageBox.No,
			)
			if answer != QMessageBox.Yes:
				self.statusBar().showMessage("Preparation cancelled.")
				return

		settings = PrepareSettings(
			source_paths=self.prepare_sources,
			output_dir=output_dir,
			threshold_db=float(self.threshold_input.value()),
			min_region_ms=int(self.min_region_input.value()),
			min_silence_ms=int(self.min_silence_input.value()),
			padding_ms=int(self.padding_input.value()),
			output_sample_rate=int(self.prepare_sample_rate_input.currentText()),
		)
		self.prepare_log_output.clear()
		self.prepare_progress.setRange(0, max(1, len(self.prepare_sources)))
		self.prepare_progress.setValue(0)
		self.start_worker(PrepareWorker(settings), "Preparing samples...")

	def start_worker(self, worker: GenerationWorker | PrepareWorker, status: str) -> None:
		self.worker = worker
		worker.log.connect(self.append_log)
		worker.progress.connect(self.on_progress)
		worker.done.connect(self.on_done)
		worker.failed.connect(self.on_failed)
		worker.cancelled.connect(self.on_cancelled)
		worker.finished.connect(self.on_worker_finished)
		self.set_busy(True)
		self.statusBar().showMessage(status)
		worker.start()

	def set_busy(self, busy: bool) -> None:
		generation_inputs: Iterable[QWidget] = (
			self.folder_input,
			self.start_note_input,
			self.start_octave_input,
			self.range_input,
			self.gap_input,
			self.order_input,
			self.pitch_input,
			self.dump_input,
			self.trim_silence_input,
			self.normalize_input,
			self.slicex_input,
			self.fade_input,
			self.fixed_length_input,
			self.sample_rate_input,
			self.browse_button,
		)
		prepare_inputs: Iterable[QWidget] = (
			self.prepare_source_input,
			self.prepare_files_button,
			self.prepare_folder_button,
			self.threshold_input,
			self.min_region_input,
			self.min_silence_input,
			self.padding_input,
			self.prepare_sample_rate_input,
		)
		for widget in [*generation_inputs, *prepare_inputs]:
			widget.setEnabled(not busy)
		self.cancel_button.setEnabled(busy)
		self.prepare_cancel_button.setEnabled(busy)
		self.generate_button.setEnabled(False if busy else self.generate_button.isEnabled())
		self.prepare_button.setEnabled(False if busy else bool(self.prepare_sources))

	def cancel_worker(self) -> None:
		if self.worker:
			self.worker.request_cancel()
			self.statusBar().showMessage("Cancelling...")

	def append_log(self, message: str) -> None:
		target = self.prepare_log_output if isinstance(self.worker, PrepareWorker) else self.log_output
		target.append(message)
		target.moveCursor(QTextCursor.End)

	def on_progress(self, done: int, total: int, label: str) -> None:
		text = f"Note {done}/{total} - {label}" if isinstance(self.worker, GenerationWorker) else f"{done}/{total} - {label}"
		if isinstance(self.worker, PrepareWorker):
			self.prepare_progress.setRange(0, total)
			self.prepare_progress.setValue(done)
		else:
			self.progress.setRange(0, total)
			self.progress.setValue(done)
		self.statusBar().showMessage(text)

	def on_done(self, output: str) -> None:
		self.last_output_path = Path(output)
		self.open_output_button.setEnabled(True)
		self.prepare_open_button.setEnabled(True)
		self.statusBar().showMessage(f"Done: {output}", 8000)
		QMessageBox.information(self, "ChromaKit", f"Created {output}")

	def on_failed(self, message: str) -> None:
		self.statusBar().showMessage("Error", 8000)
		QMessageBox.critical(self, "ChromaKit", message)

	def on_cancelled(self, message: str) -> None:
		self.statusBar().showMessage(message, 8000)
		if isinstance(self.worker, PrepareWorker):
			self.prepare_progress.setValue(0)
		else:
			self.progress.setValue(0)

	def on_worker_finished(self) -> None:
		self.worker = None
		self.set_busy(False)
		self.refresh_generation_validation()
		self.prepare_button.setEnabled(bool(self.prepare_sources))

	def open_last_output(self) -> None:
		if not self.last_output_path:
			return
		path = self.last_output_path
		target = path if path.is_file() else path
		try:
			if sys.platform.startswith("win"):
				if target.is_file():
					subprocess.Popen(["explorer", f"/select,{target}"])
				else:
					subprocess.Popen(["explorer", str(target)])
			else:
				subprocess.Popen(["xdg-open", str(target if target.is_dir() else target.parent)])
		except Exception as error:
			QMessageBox.warning(self, "ChromaKit", f"Could not open output: {error}")


def main() -> None:
	app = QApplication(sys.argv)
	window = GeneratorWindow()
	window.show()
	sys.exit(app.exec())


if __name__ == "__main__":
	main()
