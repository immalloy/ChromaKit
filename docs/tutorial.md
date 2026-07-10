# Beginner Tutorial

This is the simple version. You only need WAV files and ChromaKit.

## Make a Chromatic

1. Create a new folder.
2. Put your voice samples in that folder.
3. If you can, name them `1.wav`, `2.wav`, `3.wav`, and so on.
4. Open ChromaKit.
5. Stay on the **Generate** tab.
6. Click **Browse** and choose your sample folder.
7. Leave the default settings alone for your first try.
8. Click **Generate Chromatic**.

When it finishes, look inside your sample folder. You should see `chromatic.wav`.

## Prepare Raw Vocals First

Use this if you have a long recording instead of clean short samples.

1. Open ChromaKit.
2. Go to **Prepare Samples**.
3. Click **Choose WAV Files** for one or more files, or **Choose Folder** for a folder.
4. Leave the silence settings alone for your first try.
5. Click **Prepare Samples**.
6. Open the new `prepared_samples/` folder.
7. Go back to **Generate**.
8. Choose the `prepared_samples/` folder.
9. Click **Generate Chromatic**.

## What the Main Generate Settings Mean

- **Starting note**: the first note in the scale.
- **Starting octave**: how low or high the first note starts.
- **Range**: how many notes ChromaKit makes.
- **Sample gap**: silence between notes in `chromatic.wav`.
- **Sample order**: how samples are picked.
- **Pitch samples**: changes each sample to match the notes.
- **Audio style → Formant corrected**: shifts pitch while keeping vocal formants in place, reducing the chipmunk effect. This is the default style.
- **Dump individual samples**: saves separate WAV files too.
- **Trim silence from samples**: cuts quiet edges from samples.
- **Peak normalize before pitch**: makes sample volume more even.
- **Embed FL Studio Slicex markers**: adds markers for FL Studio Slicex.

## If It Does Not Work

- Make sure your files are `.wav`.
- Make sure the folder is not empty.
- Try shorter, cleaner samples.
- If **Prepare Samples** finds nothing, lower the silence threshold, for example from `-40 dB` to `-50 dB`.
- If the output sounds messy, try fewer samples first.
