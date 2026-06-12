from pathlib import Path
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
RELEASE_DIST = DIST / "release"


def platform_asset_name():
	if sys.platform.startswith("win"):
		return "ChromaKit-windows"
	if sys.platform == "darwin":
		return "ChromaKit-macos"
	return "ChromaKit-linux"


def main():
	command = [
		sys.executable,
		"-m",
		"PyInstaller",
		"--noconfirm",
		"--paths",
		str(ROOT / "src"),
		"--name",
		"ChromaKit",
		"--hidden-import",
		"numpy",
	]

	icon = ROOT / "assets" / "icon.ico"
	if sys.platform.startswith("win") and icon.exists():
		command.append("--windowed")
		command.extend(["--icon", str(icon)])

	command.append(str(ROOT / "src" / "chromakit" / "__main__.py"))
	subprocess.run(command, cwd=ROOT, check=True)

	if RELEASE_DIST.exists():
		shutil.rmtree(RELEASE_DIST)
	RELEASE_DIST.mkdir(parents=True, exist_ok=True)

	built_app = DIST / "ChromaKit"
	if not built_app.exists():
		raise FileNotFoundError(f"Expected build output was not created: {built_app}")

	archive_base = RELEASE_DIST / platform_asset_name()
	shutil.make_archive(str(archive_base), "zip", DIST, "ChromaKit")


if __name__ == "__main__":
	main()
