"""Reconstruct a recorded renderer from its public upstream commit and verified patches."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = json.loads((ROOT / "patches/profiles/manifest.json").read_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", choices=MANIFEST["profiles"])
    parser.add_argument("destination", type=Path, help="new directory for the patched source")
    parser.add_argument("--source", default=MANIFEST["upstream"],
                        help="upstream Git URL or a local mirror with the pinned commit")
    args = parser.parse_args()
    dest = args.destination.absolute()
    if dest.exists() or dest.is_symlink():
        parser.error("Choose a new directory for the renderer.")
    dest.parent.mkdir(parents=True, exist_ok=True)
    sequence = ["baseline"]
    if args.profile != "baseline":
        sequence.append(args.profile)
    with tempfile.TemporaryDirectory(dir=dest.parent) as scratch:
        tree = Path(scratch) / "renderer"
        tree.mkdir()

        def git(*arguments):
            subprocess.run(["git", "-C", str(tree), *arguments], check=True)

        git("init", "--quiet")
        git("fetch", "--quiet", "--depth=1", args.source, MANIFEST["upstream_commit"])
        git("checkout", "--quiet", "--detach", "FETCH_HEAD")
        for name in sequence:
            item = MANIFEST["profiles"][name]
            patch = ROOT / item["patch"]
            if hashlib.sha256(patch.read_bytes()).hexdigest() != item["sha256"]:
                raise SystemExit(f"Patch checksum failed for {name}.")
            git("apply", "--index", "--whitespace=nowarn", str(patch))
        tree.rename(dest)
    print(f"Prepared {args.profile} in {dest}")


if __name__ == "__main__":
    main()
