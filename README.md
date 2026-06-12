# ChromaKit

ChromaKit is a simple cross-platform app for making Friday Night Funkin' chromatic scales.

In FNF, a chromatic scale, often called a chrom, is an audio file that contains a character's voice singing vowels or other sounds across a musical range. It works like a small custom voicebank, similar in idea to a Vocaloid voicebank, so composers and cover creators can make a character sing any melody in a DAW like FL Studio, Ableton, or Reaper.

ChromaKit takes a folder of WAV samples and generates:

- `chromatic.wav` - one combined chromatic scale
- `pitched_samples/` - optional individual pitched WAV files
- `samples/` - optional individual unpitched WAV files when pitch is disabled

It can also prepare raw vocals by splitting one or more WAV files on silence and writing numbered samples into `prepared_samples/`.

## How to Use

1. Put your source samples in one folder.
2. Name the samples `1.wav`, `2.wav`, `3.wav`, and so on. If numbered samples are not found, ChromaKit uses the other WAV files in the folder.
3. Open ChromaKit.
4. Choose or drag in the sample folder.
5. Pick the starting note, octave, range, gap, sample order, and processing options. You can also trim leading/trailing silence from each source sample during generation.
6. Click **Generate Chromatic**.

Use **Prepare Samples** to choose raw WAV files or a folder, tune the silence detection settings if needed, and click **Prepare Samples**. Generated numbered samples are written to `prepared_samples/`.

## License

ChromaKit is licensed under the GPL-3.0-or-later license.

## Credits

ChromaKit is built from [Chromatic Scale Generator](https://gamebanana.com/tools/8906) by ChillSpace. It is also inspired by [Chromatic Scale Generator PLUS! (REVIVED)](https://gamebanana.com/tools/20901) and [(CANCELLED) Chromatic Scale Generator DELUXE](https://gamebanana.com/tools/20598), created by me.
