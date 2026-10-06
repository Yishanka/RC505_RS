"""One-time release-page cleanup; git tags and commits are never removed."""
from pathlib import Path
import argparse
import json
import subprocess
from release_notes import render

REPOSITORY = "Yishanka/RC505_RS"
NEW_TAG = "v0.4.8"
RETIRE = frozenset(("v0.2.1", "v0.2.3", "v0.2.4", "v0.2.5", "v0.2.6", "v0.2.7", "v0.4.1", "v0.4.3", "v0.4.6"))
CONDENSE = ("v0.2.8", "v0.3.0", "v0.4.0", "v0.4.2", "v0.4.4", "v0.4.5", "v0.4.7")
PROTECTED = frozenset(("v0.1.0-alpha", "v0.1.1-alpha", NEW_TAG, *CONDENSE))


def plan(releases: list[dict]) -> list[str]:
    tags = {r["tagName"] for r in releases}
    if not PROTECTED <= tags:
        raise ValueError(f"Protected releases missing: {sorted(PROTECTED - tags)}")
    latest = [r for r in releases if r["isLatest"]]
    if len(latest) != 1 or latest[0]["tagName"] != NEW_TAG or latest[0]["isDraft"] or latest[0]["isPrerelease"]:
        raise ValueError("The verified replacement must be the latest published release")
    return sorted(RETIRE & tags)


def gh(*args: str) -> str:
    return subprocess.check_output(["gh", *args, "--repo", REPOSITORY], text=True, encoding="utf-8")


def run(execute: bool) -> None:
    releases = json.loads(gh("release", "list", "--limit", "100", "--json", "tagName,isLatest,isDraft,isPrerelease"))
    retired = plan(releases)
    assets = json.loads(gh("release", "view", NEW_TAG, "--json", "assets"))["assets"]
    required = {"RC505-RS-0.4.8-windows-x64-setup.exe", "RC505-RS-0.4.8-windows-x64-portable.zip", "update.json", "SHA256SUMS.txt"}
    if not required <= {a["name"] for a in assets}:
        raise ValueError("Replacement downloads are incomplete")
    root = Path(__file__).resolve().parent.parent
    history = (root / "docs/RELEASE_NOTES_CN.md").read_text(encoding="utf-8-sig")
    notes_root = root / "var/release-curation-0.4.8"
    notes_root.mkdir(parents=True, exist_ok=True)
    # Prepare every replacement page before making any remote change.
    pages = []
    for tag in CONDENSE:
        path = notes_root / f"{tag}.md"
        path.write_text(render(tag[1:], history, archived=True), encoding="utf-8")
        pages.append((tag, path))
    report = {"retire": retired, "preserve": sorted(PROTECTED), "condense": list(CONDENSE), "delete_git_tags": False}
    (notes_root / "plan.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report), flush=True)
    if not execute:
        return
    for tag, path in pages:
        gh("release", "edit", tag, "--notes-file", str(path))
    for tag in retired:
        # No --cleanup-tag: users can still check out every historical revision.
        gh("release", "delete", tag, "--yes")
        print(f"Removed release page and assets: {tag}", flush=True)
    after = json.loads(gh("release", "list", "--limit", "100", "--json", "tagName,isLatest,isDraft,isPrerelease"))
    if plan(after):
        raise RuntimeError("Some selected releases remain")
    print("Release cleanup verified; historical source tags retained.", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--execute", action="store_true")
    run(parser.parse_args().execute)
