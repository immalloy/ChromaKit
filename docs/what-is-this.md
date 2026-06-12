# What is ChromaKit?

ChromaKit is a desktop app for making Friday Night Funkin' chromatic scales from WAV samples.

## What is a chromatic scale?

In FNF modding, a chromatic scale, often called a chrom, is an audio file with a character voice spread across musical notes. Instead of only having one sound, you get that same voice pitched up and down so it can sing a melody.

Think of it like a small custom voicebank. You load it into a DAW or sampler, then use it to make a character sing notes.

## What does ChromaKit make?

ChromaKit can create:

- `chromatic.wav` - one combined chromatic scale
- `pitched_samples/` - separate pitched WAV files
- `samples/` - separate unpitched WAV files, if pitch is turned off
- `prepared_samples/` - numbered samples made from raw vocals

## When should I use Prepare Samples?

Use **Prepare Samples** when you have one long vocal recording or a messy folder of raw vocals. ChromaKit scans for sound, cuts out silent gaps, and writes clean numbered samples.

After that, use those numbered samples in the **Generate** tab.

## Do my files need special names?

Numbered files work best:

- `1.wav`
- `2.wav`
- `3.wav`

If ChromaKit does not find numbered files, it will still use the other WAV files in the folder.

## What is the output for?

The output is for music tools like FL Studio, Ableton, Reaper, or any sampler that can use WAV files.
