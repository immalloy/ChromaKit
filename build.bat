@echo off
setlocal
cd /d "%~dp0"

python scripts\build.py

exit /b %errorlevel%
