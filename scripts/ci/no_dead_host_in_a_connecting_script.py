#!/usr/bin/env python3
"""A script that connects somewhere may not carry a literal address.

WHY (#261 family)

The Hetzner VPS at 46.225.223.175 was cancelled on 24-08-2026. Addresses get
reassigned, and that one now answers for somebody else. Seven scripts in this
repository still named it -- six with the address hardcoded and no way to
override it, one as a default behind an unset variable.

The failure mode is the part worth stating. A dead address in a script does not
break when the host goes away; it keeps working, against whoever holds the
address next. `scripts/orchestrator-cycle.sh` carried an `scp` line, so the
first person to run it without thinking would have pushed our data to a stranger
and seen nothing unusual.

So: in any file that invokes ssh, scp or rsync, a literal IPv4 address is a
failure, and so is the retired hostname used as a host. A missing host must be
loud -- `VPS_HOST` unset should exit 2 with a sentence, not fall back to
something that once worked.

WHAT IS DELIBERATELY NOT FLAGGED

  * `known_hosts` entries. They are the opposite of the danger: a stale key
    makes the next connection to a reassigned address REFUSE loudly. Deleting
    them would let the stranger be trusted silently.
  * loopback and unspecified addresses -- 127.0.0.1, 0.0.0.0, ::1 -- which name
    this machine and cannot be reassigned to anyone.
  * comment lines, which do not connect to anything. History is worth keeping;
    it is the executable line that must not carry an address.
"""
from __future__ import annotations
import argparse, pathlib, re, sys

CONNECTS = re.compile(r'(?:^|[^\w-])(?:ssh|scp|rsync)\b')
IPV4 = re.compile(r'\b(\d{1,3}(?:\.\d{1,3}){3})\b')
RETIRED_HOSTS = ("xfa-test-runner",)
SAFE_IPS = {"127.0.0.1", "0.0.0.0", "255.255.255.255"}
SKIP_DIRS = {".git", "target", "node_modules", ".worktrees"}
SKIP_NAMES = ("known_hosts",)
SUFFIXES = (".sh", ".py", ".mjs", ".js", ".bash", ".zsh")


def is_comment(line: str) -> bool:
    return line.lstrip().startswith(("#", "//", "*", "/*"))


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".")
    args = ap.parse_args(argv[1:])
    root = pathlib.Path(args.root).resolve()

    scanned, problems = 0, []
    for path in root.rglob("*"):
        if not path.is_file() or path.suffix not in SUFFIXES:
            continue
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if any(n in path.name for n in SKIP_NAMES):
            continue
        if path.resolve() == pathlib.Path(__file__).resolve():
            # This file has to name the address to explain why no other file may.
            # The alternative is a guard that cannot describe its own reason.
            continue
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        if not CONNECTS.search(text):
            continue
        scanned += 1
        rel = path.relative_to(root)
        for n, line in enumerate(text.splitlines(), 1):
            if is_comment(line):
                continue
            for ip in IPV4.findall(line):
                if ip in SAFE_IPS or ip.startswith("0."):
                    continue
                problems.append(f"{rel}:{n}: literal address {ip} in a script that "
                                "connects. Take it from the environment and fail "
                                "loudly when it is unset.")
            for host in RETIRED_HOSTS:
                if re.search(rf'@{re.escape(host)}\b|\b(?:ssh|scp|rsync)\s+[^\s]*{re.escape(host)}\b', line):
                    problems.append(f"{rel}:{n}: connects to the retired host "
                                    f"{host!r}; that machine is gone.")

    if scanned == 0:
        # A guard that finds no files cannot tell you the tree is clean.
        print("[dead-host] FATAL: no connecting scripts found under "
              f"{root}. Refusing to report a clean result for a scan that read "
              "nothing.", file=sys.stderr)
        return 2
    if problems:
        print(f"[dead-host] FAIL: {len(problems)} literal address(es) in scripts "
              "that connect:", file=sys.stderr)
        for p in problems:
            print(f"    {p}", file=sys.stderr)
        print("\n  An address that dies does not stop the script -- it points it at "
              "whoever\n  holds that address next. 46.225.223.175 was ours until "
              "24-08-2026.", file=sys.stderr)
        return 1
    print(f"[dead-host] OK: {scanned} connecting script(s), none carries a literal "
          "address.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
