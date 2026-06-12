@echo off
setlocal
cd /d "%~dp0"

set "PYTHONPATH=%CD%\src;%PYTHONPATH%"
python -m chromakit
