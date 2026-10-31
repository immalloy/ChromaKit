# Chromakit release log

This is the short version of what changed for people using the app.

## v1.2.1

Fixed the release workflow.

## v1.2.0

ChromaKit rebuilt has been finished in Rust and the ui in egui (Thank u Dave for the ref).
Implemented Volume sustaining and improvements to rendering speed

## v1.1.1

This release fixes the build and release workflows.

## v1.1.0

The starting octave is now a dropdown from 0 to 9, covering C0 through C9.
On Windows, Chromakit follows the system light or dark theme
Individual exports are cleared when a new render starts and written as each note finishes. Progress now names the file and note together, such as `pitched_087.wav (note 122, 87/90)`.

## v1.0.0

Chromakit's first public alpha release.

The app can detect the pitch of numbered WAV samples, cycle through them across a chromatic range, and export the result as a combined WAV or separate files.

It also includes native folder picking on Windows, macOS, and Linux, plus a simple FLTK window that stays out of the way.

This is the starting point. Chromakit is still barebones, and future releases will build on this base.
