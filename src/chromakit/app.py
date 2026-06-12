from pathlib import Path
import sys

import parselmouth
from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
	QApplication,
	QCheckBox,
	QComboBox,
	QFileDialog,
	QFormLayout,
	QHBoxLayout,
	QLabel,
	QLineEdit,
	QMainWindow,
	QMessageBox,
	QPushButton,
	QSpinBox,
	QVBoxLayout,
	QWidget,
)


NOTES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
OCTAVES = ["2", "3", "4"]


def generate_chromatic(
	sample_path,
	start_note_index,
	start_octave_index,
	semitones,
	gap_seconds,
	pitch_samples=True,
	dump_samples=True,
):
	sample_dir = Path(sample_path)
	if not sample_dir.is_dir():
		raise ValueError("Choose a valid sample folder.")

	sample_count = 0
	while (sample_dir / f"{sample_count + 1}.wav").exists():
		sample_count += 1

	if sample_count == 0:
		raise ValueError("The sample folder needs numbered WAV files like 1.wav, 2.wav, 3.wav.")

	sample_gap = parselmouth.praat.call(
		"Create Sound from formula",
		"Gap",
		1,
		0,
		float(gap_seconds),
		48000,
		"0",
	)
	starting_key = int(start_note_index) + 12 * int(start_octave_index)
	pitched_sounds = []
	spaced_pitched_sounds = []

	for i in range(int(semitones)):
		sample_file = sample_dir / f"{i % sample_count + 1}.wav"
		current_sound = parselmouth.praat.call(
			parselmouth.praat.call(
				parselmouth.Sound(str(sample_file)),
				"Resample",
				48000,
				1,
			),
			"Convert to mono",
		)

		if pitch_samples:
			manipulation = parselmouth.praat.call(current_sound, "To Manipulation", 0.05, 60, 600)
			pitch_tier = parselmouth.praat.call(manipulation, "Extract pitch tier")
			parselmouth.praat.call(pitch_tier, "Formula", f"32.703 * (2 ^ ({i + starting_key + 12}/12))")
			parselmouth.praat.call([pitch_tier, manipulation], "Replace pitch tier")
			pitched_sound = parselmouth.praat.call(manipulation, "Get resynthesis (overlap-add)")
		else:
			pitched_sound = current_sound

		pitched_sounds.append(pitched_sound)
		spaced_pitched_sounds.append(pitched_sound)
		spaced_pitched_sounds.append(sample_gap)

	chromatic = parselmouth.Sound.concatenate(spaced_pitched_sounds)
	chromatic_path = sample_dir / "chromatic.wav"
	chromatic.save(str(chromatic_path), "WAV")

	if dump_samples and pitch_samples:
		pitched_dir = sample_dir / "pitched_samples"
		pitched_dir.mkdir(exist_ok=True)
		for index, pitched_sound in enumerate(pitched_sounds, start=1):
			pitched_sound.save(str(pitched_dir / f"pitched_{index}.wav"), "WAV")

	return chromatic_path


class GeneratorWindow(QMainWindow):
	def __init__(self):
		super().__init__()
		self.setWindowTitle("ChromaKit")
		self.setMinimumSize(520, 360)

		root = QWidget()
		layout = QVBoxLayout(root)
		layout.setContentsMargins(18, 16, 18, 16)
		layout.setSpacing(12)

		title = QLabel("ChromaKit")
		title_font = title.font()
		title_font.setPointSize(20)
		title_font.setBold(True)
		title.setFont(title_font)
		title.setAlignment(Qt.AlignCenter)
		layout.addWidget(title)

		self.folder_input = QLineEdit()
		self.folder_input.setPlaceholderText("Choose a folder with 1.wav, 2.wav, 3.wav...")
		browse_button = QPushButton("Browse")
		browse_button.clicked.connect(self.choose_folder)

		folder_row = QHBoxLayout()
		folder_row.addWidget(self.folder_input)
		folder_row.addWidget(browse_button)

		self.start_note_input = QComboBox()
		self.start_note_input.addItems(NOTES)

		self.start_octave_input = QComboBox()
		self.start_octave_input.addItems(OCTAVES)

		self.range_input = QSpinBox()
		self.range_input.setRange(1, 120)
		self.range_input.setValue(24)

		self.gap_input = QLineEdit("0.1")

		form = QFormLayout()
		form.setLabelAlignment(Qt.AlignRight)
		form.addRow("Sample folder:", folder_row)
		form.addRow("Starting note:", self.start_note_input)
		form.addRow("Starting octave:", self.start_octave_input)
		form.addRow("Range:", self.range_input)
		form.addRow("Sample gap:", self.gap_input)
		layout.addLayout(form)

		self.pitch_input = QCheckBox("Pitch samples")
		self.pitch_input.setChecked(True)
		self.dump_input = QCheckBox("Dump individual pitched samples")
		self.dump_input.setChecked(True)
		layout.addWidget(self.pitch_input)
		layout.addWidget(self.dump_input)

		self.generate_button = QPushButton("Generate Chromatic")
		self.generate_button.clicked.connect(self.generate)
		layout.addWidget(self.generate_button)
		layout.addStretch(1)

		self.setCentralWidget(root)

	def choose_folder(self):
		folder = QFileDialog.getExistingDirectory(self, "Select sample folder")
		if folder:
			self.folder_input.setText(folder)

	def generate(self):
		self.generate_button.setEnabled(False)
		try:
			output_path = generate_chromatic(
				self.folder_input.text(),
				self.start_note_input.currentIndex(),
				self.start_octave_input.currentIndex(),
				self.range_input.value(),
				float(self.gap_input.text()),
				self.pitch_input.isChecked(),
				self.dump_input.isChecked(),
			)
		except Exception as error:
			QMessageBox.critical(self, "ChromaKit", str(error))
		else:
			QMessageBox.information(self, "ChromaKit", f"Created {output_path}")
		finally:
			self.generate_button.setEnabled(True)


def main():
	app = QApplication(sys.argv)
	window = GeneratorWindow()
	window.show()
	sys.exit(app.exec())


if __name__ == "__main__":
	main()

