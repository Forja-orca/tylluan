#!/usr/bin/env python3
"""Fail if a tracked Markdown file links to a repo path that does not exist.

Why this exists (2026-10-14 documentation audit): 20 relative links were broken
or pointed at absolute paths of one developer's machine (file:///E:/...,
E:\\TylluanMCPo3). check_docs_reality.sh only judges backtick-cited paths, so
nothing gated real Markdown links.

Checked: [text](target), ![alt](target), <img src>, <a href> with a relative
target. Skipped: http(s)/mailto/anchor-only links and anything inside fenced
code blocks. A target passes if it is a tracked file or a directory that
contains tracked files. `file:///` links are always errors: they only resolve
on one machine.

Usage: python3 scripts/check_doc_links.py      (exit 0 clean, 1 on findings)
"""
import os
import re
import subprocess
import sys
import urllib.parse

# Windows consoles default to cp1252 and cannot print the status symbols below.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")

ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
).stdout.strip()


def git_ls(pattern=None):
    cmd = ["git", "-C", ROOT, "ls-files"] + ([pattern] if pattern else [])
    return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.splitlines()


tracked = set(git_ls())
md_files = [f for f in git_ls("*.md")]

LINK = re.compile(r"(?<!\!)\[[^\]\n]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
EXTRA = re.compile(r"!\[[^\]\n]*\]\(([^)\s]+)\)|<img[^>]+src=\"([^\"]+)\"|<a[^>]+href=\"([^\"]+)\"")
EXTERNAL = re.compile(r"^(https?:|mailto:|#|tel:|data:)")

problems = []
checked = 0
for f in md_files:
    try:
        text = open(os.path.join(ROOT, f), encoding="utf-8").read()
    except (OSError, UnicodeDecodeError) as exc:
        problems.append((f, 0, f"cannot read file: {exc}"))
        continue
    in_code = False
    for lineno, line in enumerate(text.splitlines(), 1):
        if line.strip().startswith("```"):
            in_code = not in_code
            continue
        if in_code:
            continue
        targets = [m.group(1) for m in LINK.finditer(line)]
        for m in EXTRA.finditer(line):
            targets += [g for g in m.groups() if g]
        for target in targets:
            if EXTERNAL.match(target):
                continue
            checked += 1
            if target.lower().startswith("file:"):
                problems.append((f, lineno, f"machine-specific file:// link: {target}"))
                continue
            path = urllib.parse.unquote(target.split("#")[0].split("?")[0])
            if not path:
                continue
            if path.startswith("/"):
                rel = path.lstrip("/")
            else:
                rel = os.path.normpath(os.path.join(os.path.dirname(f), path)).replace("\\", "/")
            if rel in tracked or any(t.startswith(rel.rstrip("/") + "/") for t in tracked):
                continue
            problems.append((f, lineno, f"target not tracked in the repo: {target}"))

if problems:
    for f, lineno, why in problems:
        print(f"❌ {f}:{lineno}: {why}")
    print(f"\n{len(problems)} broken doc link(s) across {len(md_files)} Markdown files ({checked} relative links checked).")
    sys.exit(1)
print(f"✅ {checked} relative doc links across {len(md_files)} Markdown files all resolve to tracked paths.")
