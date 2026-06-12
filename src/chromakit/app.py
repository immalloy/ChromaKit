#This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

#This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

#You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

import os
from os.path import exists

import parselmouth
import wx

from chromakit import app_ui


class GeneratorGUI(app_ui.AppFrame):
	def generate_chromatic(self, event):
		sample_path = self.dirPicker.GetPath()
		sample_gap = parselmouth.praat.call("Create Sound from formula", "Gap", 1, 0, float(self.gapInput.GetValue()), 48000, "0")
		file_index = 0
	
		while exists(sample_path + os.sep + str(file_index + 1) + ".wav"):
			file_index += 1
		
		semitones = int(self.rangeInput.GetValue())
		pitched_sounds = []
		spaced_pitched_sounds = []
		
		for i in range(semitones):
			starting_key = self.startNoteChoice.GetSelection() + 12 * self.startOctaveChoice.GetSelection()
			
			current_sound = parselmouth.praat.call(parselmouth.praat.call(parselmouth.Sound(sample_path + os.sep + str(i % (file_index) + 1) + ".wav"), "Resample", 48000, 1), "Convert to mono")

			if self.pitchedCheck.IsChecked():
				manipulation = parselmouth.praat.call(current_sound, "To Manipulation", 0.05, 60, 600)
				pitch_tier = parselmouth.praat.call(manipulation, "Extract pitch tier")

				parselmouth.praat.call(pitch_tier, "Formula", f"32.703 * (2 ^ ({i + starting_key + 12}/12))")
				parselmouth.praat.call([pitch_tier, manipulation], "Replace pitch tier")
			
				pitched_sounds.append(parselmouth.praat.call(manipulation, "Get resynthesis (overlap-add)"))
				spaced_pitched_sounds.append(parselmouth.praat.call(manipulation, "Get resynthesis (overlap-add)"))
			else:
				pitched_sounds.append(current_sound)
				spaced_pitched_sounds.append(current_sound)
			
			spaced_pitched_sounds.append(sample_gap)
			
			chromatic = parselmouth.Sound.concatenate(spaced_pitched_sounds)
			chromatic.save(sample_path + os.sep + "chromatic.wav", "WAV")
			
			if self.samplesCheck.IsChecked() and self.pitchedCheck.IsChecked():
				if not os.path.exists(sample_path + os.sep + "pitched_samples"):
					os.makedirs(sample_path + os.sep + "pitched_samples")
				for pitched_sound in pitched_sounds:
					pitched_sound.save(sample_path + os.sep + "pitched_samples" + os.sep + ""f"pitched_{1 + pitched_sounds.index(pitched_sound)}.wav", "WAV")

def main():
	app = wx.App(False)
	frame = GeneratorGUI(None)
	frame.Show(True)
	app.MainLoop()


if __name__ == "__main__":
	main()

