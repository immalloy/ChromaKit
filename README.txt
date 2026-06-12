Chromatic Scale Generator
=========================

A small wxPython tool for generating chromatic WAV samples with praat-parselmouth.

Project layout
--------------

- src\chromatic_generator\ - Python application code
- assets\ - packaged icons and other runtime/build assets
- ui\ - wxFormBuilder project files
- scripts\ - helper wrappers
- requirements.txt - Python dependencies
- requirements.bat - installs Python dependencies
- run.bat - runs the app from source
- build.bat - builds a Windows executable with PyInstaller

Setup
-----

Run this from the repo root:

    requirements.bat

Run from source
---------------

    run.bat

Build
-----

    build.bat

The executable is written to dist\chromatic_generator.exe.

Notes
-----

The Python module praat-parselmouth is used for pitch processing:
https://github.com/YannickJadoul/Parselmouth

The user interface uses wxPython. The wxFormBuilder project is kept at ui\form.fbp.
