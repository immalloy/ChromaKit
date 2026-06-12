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
	--name "chromatic_generator" ^
	--hidden-import numpy ^
	"src\chromatic_generator\__main__.py"

exit /b %errorlevel%

