@echo off
setlocal
cd /d "%~dp0"

if not exist "assets\icon.ico" (
	echo Missing assets\icon.ico
	exit /b 1
)

python -m PyInstaller ^
	--onefile ^
	--noconsole ^
	--icon "assets\icon.ico" ^
	--paths "src" ^
	--name "ChromaKit" ^
	--hidden-import numpy ^
	"src\chromakit\__main__.py"

exit /b %errorlevel%
