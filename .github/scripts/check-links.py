#!/usr/bin/env python3
"""List broken relative links in the repository's tracked Markdown files.

usage: check-links.py [REPO_ROOT]  (default: the current directory)
Checks `[text](target)` and `[text](target#anchor)` links whose target is not a
URL: the file must exist, and an anchor must match a heading slug (GitHub's
rules: lowercase, punctuation dropped, spaces to hyphens). Prints one line per
broken link and exits 1 if any.
"""
import re
import subprocess
import sys
from pathlib import Path

GITHUB = "GITHUB_ACTIONS" in __import__("os").environ
root = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
files = subprocess.run(["git", "ls-files", "*.md"], cwd=root, capture_output=True, text=True).stdout.split()
files += [f for f in subprocess.run(["git", "ls-files", "--others", "--exclude-standard", "*.md"],
                                     cwd=root, capture_output=True, text=True).stdout.split()]
LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)|!\[[^\]]*\]\(([^)\s]+)\)")


def slug(heading):
    text = re.sub(r"[`*_]", "", heading.strip().lower())
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def anchors(path):
    found, counts = set(), {}
    in_code = False
    for line in path.read_text(errors="replace").splitlines():
        if line.startswith("```"):
            in_code = not in_code
        if in_code:
            continue
        m = re.match(r"^#{1,6}\s+(.*?)\s*#*$", line)
        if m:
            s = slug(m.group(1))
            n = counts.get(s, 0)
            found.add(s if n == 0 else f"{s}-{n}")
            counts[s] = n + 1
    return found


broken = 0
for f in sorted(set(files)):
    path = root / f
    if not path.exists() or "node_modules" in f:
        continue
    text = path.read_text(errors="replace")
    # Ignore links inside fenced code.
    text = re.sub(r"```.*?```", "", text, flags=re.S)
    for m in LINK.finditer(text):
        target = m.group(1) or m.group(2)
        if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("//"):
            continue
        file_part, _, anchor = target.partition("#")
        dest = path if not file_part else (path.parent / file_part).resolve()
        if not dest.exists():
            print(f"::error file={f}::missing file {target}" if GITHUB else f"{f}: missing file {target}")
            broken += 1
        elif anchor and dest.suffix == ".md" and anchor.lower() not in anchors(dest):
            print(f"::error file={f}::missing anchor {target}" if GITHUB else f"{f}: missing anchor {target}")
            broken += 1
print(f"{broken} broken links in {len(set(files))} Markdown files")
sys.exit(1 if broken else 0)
