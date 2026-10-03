"""Render actual offline UI fixtures. Requires Pillow; no sound device or personal data."""
from pathlib import Path
import subprocess
import sys
from PIL import Image

workspace = Path(__file__).resolve().parent.parent
modes = sys.argv[1:] or ["projects-small", "performance-small-rose", "sequence", "shortcuts-small"]
for mode in modes:
    if not mode.replace("-", "").isalnum():
        raise ValueError("Preview modes use letters, digits and hyphens")
    result = subprocess.run(
        [str(workspace / "target/debug/rc505_rs.exe"), "--offline", "--data-dir=var/ui-current", f"--ui-preview={mode}"],
        cwd=workspace, capture_output=True, timeout=30,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    if result.returncode:
        raise RuntimeError(result.stderr.decode("utf-8", errors="replace"))
    ppm = workspace / "var/ui-verification" / f"{mode}.ppm"
    with Image.open(ppm) as rendered:
        rendered.save(ppm.with_suffix(".png"))
    ppm.unlink()  # PNG keeps the exact captured pixels without the large raw copy.
    print(mode, flush=True)
