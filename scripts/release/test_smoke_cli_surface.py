#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every CLI flag/value a consumer smoke passes exists in the clap definition.

m490: smoke_cli.sh ran `pdfluent extract --format markdown`, but the CLI's
`Format` enum offers `md` — so the markdown step failed and would have turned
every release smoke red. The fix renamed the smoke to the CLI's value; this
test keeps smoke and CLI from drifting apart again in either direction: a flag
or enum value used by a smoke but missing from clap fails here, and a CLI
rename fails here until the smoke follows.

The CLI surface is parsed from the clap source (cheap; no build), the way
test_napi_build_flags.py judges `napi build` lines against the pinned help.
The smokes invoke the binary through `$BIN` / `${RUN[@]}` arrays, so the scan
judges the tokens after those markers (plus a bare `pdfluent` for smokes that
call it directly), not the word `pdfluent` alone.
"""
from __future__ import annotations

import pathlib
import re
import shlex
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SMOKES = REPO / "scripts" / "release" / "consumer_smokes"
CLI_SRC = REPO / "crates" / "pdfluent-cli" / "src"

# FLOOR: CLI invocations judged. smoke_cli.sh holds 5 (`--version`, `--help`,
# two `extract` runs, `extract --help`) and smoke_ort_variant.sh 5 more
# (`--version`, `models install --help`, `models install`, `models status`,
# `extract`); fewer means the scanner broke, and a scan that matches nothing
# approves everything.
MINIMUM_INVOCATIONS = 8

# FLOOR: clap surface parsed. Fewer means the parser broke, not that the CLI
# shrank: a shrunken CLI that the smoke still uses fails in the offender
# lists below instead.
MINIMUM_SUBCOMMANDS = 15
MINIMUM_FORMATS = 4
MINIMUM_QUALITIES = 3

# Invocation markers: the argv arrays the smokes resolve the binary into, plus
# a bare `pdfluent` in command position (not `pdfluent-extract`: that is the
# JS wrapper's own CLI, and `cargo --format-version` in smoke_rust.sh is
# cargo's own flag — neither is judged against this clap definition).
MARKERS = {"${RUN[@]}", "$BIN", "$BIN_PATH"}

# Clap built-ins: always allowed, never declared in an Args struct.
BUILTIN_LONGS = {"--help", "--version"}
BUILTIN_SHORTS = {"-h"}


def _kebab(name: str) -> str:
    """Clap's default rename: `ExtractText` -> `extract-text`."""
    return re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()


def _enum_values(source: str, enum: str) -> list[str]:
    """The CLI spellings of a `clap::ValueEnum` enum's variants."""
    body = source.split(f"pub enum {enum}", 1)[1].split("\n}\n", 1)[0]
    variants = re.findall(r"^    (\w+)(?:,| \{|\s*$)", body, re.M)
    return [_kebab(v) for v in variants]


def _value_enums() -> dict[str, list[str]]:
    """Every ValueEnum in the CLI: type name -> its CLI spellings."""
    enums: dict[str, list[str]] = {}
    for src in sorted(CLI_SRC.glob("cmd_*.rs")):
        text = src.read_text()
        for m in re.finditer(
            r"#\[derive\([^\]]*clap::ValueEnum[^\]]*\)\]\s*pub enum (\w+)",
            text,
        ):
            enums[m.group(1)] = _enum_values(text, m.group(1))
    return enums


def _args_structs() -> dict[str, dict]:
    """Every `*Args` struct: long flags, short flags, valued flags, and the
    positional's ValueEnum type where the struct takes a bare enum value."""
    structs: dict[str, dict] = {}
    for src in [CLI_SRC / "lib.rs", *sorted(CLI_SRC.glob("cmd_*.rs"))]:
        text = src.read_text()
        for m in re.finditer(r"(?:pub )?struct (\w+)\s*\{(.*?)\}", text, re.S):
            name, body = m.group(1), m.group(2)
            longs: dict[str, str | None] = {}
            shorts: set[str] = set()
            positional_enum: str | None = None
            for fm in re.finditer(
                r'(?:#\[([^\]]*)\]\s+)?pub (\w+): ([\w:<>]+),', body
            ):
                attr, field, type_ = fm.group(1) or "", fm.group(2), fm.group(3)
                if attr.startswith("command("):
                    continue
                base = type_.split("::")[-1].strip("<> ")
                if "long" not in attr and "short" not in attr:
                    if fm.group(1) is None:
                        positional_enum = base
                    continue
                long = "--" + field.replace("_", "-")
                lit = re.search(r'long\s*=\s*"([^"]+)"', attr)
                if lit:
                    long = "--" + lit.group(1)
                short = re.search(r"short\s*=\s*'(\w)'", attr)
                if short:
                    shorts.add("-" + short.group(1))
                takes_value = not (
                    base == "bool" or "ArgAction::Count" in attr
                )
                longs[long] = base if ("value_enum" in attr) else (
                    "" if takes_value else None
                )
            structs[name] = {
                "longs": longs,
                "shorts": shorts,
                "positional_enum": positional_enum,
            }
    return structs


def _subcommands() -> dict[str, str]:
    """Top-level subcommand name -> its Args struct (lib.rs `Commands`)."""
    text = (CLI_SRC / "lib.rs").read_text()
    body = text.split("enum Commands", 1)[1].split("\n}\n", 1)[0]
    subs = {}
    for m in re.finditer(r"^    (\w+)\(([\w:]+)\)[, ]*$", body, re.M):
        subs[_kebab(m.group(1))] = m.group(2).split("::")[-1]
    for m in re.finditer(r"^    (\w+) \{$", body, re.M):
        subs.setdefault(_kebab(m.group(1)), "")
    return subs


def _models_subcommands() -> dict[str, str]:
    """`models` subcommand name -> its Args struct (`ModelsCommands`)."""
    text = (CLI_SRC / "cmd_models.rs").read_text()
    body = text.split("enum ModelsCommands", 1)[1].split("\n}\n", 1)[0]
    return {
        _kebab(m.group(1)): m.group(2)
        for m in re.finditer(r"^    (\w+)\((\w+)\)[, ]*$", body, re.M)
    }


def _without_heredocs(text: str) -> list[str]:
    """Raw lines with heredoc bodies dropped: Cargo.toml and python/js
    snippets are not shell (`pdfluent = { ... }` names a dependency, it
    does not run the CLI)."""
    out = []
    end: str | None = None
    for line in text.splitlines():
        if end is not None:
            if line.strip() == end:
                end = None
            continue
        m = re.search(r"<<-?\s*['\"]?(\w+)['\"]?", line)
        if m:
            end = m.group(1)
        out.append(line)
    return out


def _logical_lines(text: str) -> list[str]:
    """Shell lines with backslash continuations joined."""
    out = []
    buf = ""
    for line in _without_heredocs(text):
        stripped = line.rstrip()
        if stripped.endswith("\\"):
            buf += stripped[:-1]
        else:
            out.append(buf + line)
            buf = ""
    if buf:
        out.append(buf)
    return out


# A marker only counts in command position: `[[ -x "$BIN" ]]` tests a
# variable and `find -name pdfluent` names a file — neither runs the CLI.
_COMMAND_SEPS = {
    "|", "||", "&&", ";", "{", "(", "$(", "!", "if", "while", "until",
    "then", "do", "else", "elif",
}


def _in_command_position(prev: str | None) -> bool:
    return (
        prev is None or prev in _COMMAND_SEPS or prev.endswith(("=", "$("))
    )


def _is_bare(tok: str) -> bool:
    return (
        tok == "pdfluent"
        or tok.endswith("/pdfluent")
        or tok.endswith("$(pdfluent")
    )


def _command_substitutions(line: str) -> list[str]:
    """The inner texts of the line's outermost `$(...)` spans (`$((...))`
    arithmetic is not a command and is skipped). The smokes wrap most
    invocations as `out="$("$BIN" ...)"`, which lexes as one token — the
    argv hides inside the span."""
    spans = []
    i, n = 0, len(line)
    while i < n:
        # Only single quotes suppress `$(...)`; inside double quotes it
        # still runs, so `"` is an ordinary character out here.
        if line[i] == "'":
            i += 1
            while i < n and line[i] != "'":
                i += 1
            i += 1
        elif line.startswith("$(", i) and not line.startswith("$((", i):
            depth = 1
            i += 2
            start = i
            quote: str | None = None
            while i < n and depth:
                ch = line[i]
                if quote is not None:
                    if ch == "\\":
                        i += 1
                    elif ch == quote:
                        quote = None
                elif ch in "'\"":
                    quote = ch
                elif ch == "(":
                    depth += 1
                elif ch == ")":
                    depth -= 1
                i += 1
            spans.append(line[start:i - 1])
        else:
            i += 1
    return spans


def _invocations() -> list[tuple[str, list[str]]]:
    """Every CLI argv in the smokes: (file, tokens after the marker)."""
    found = []
    for src in sorted(SMOKES.glob("smoke_*.sh")):
        for line in _logical_lines(src.read_text()):
            try:
                candidates = [shlex.split(line, comments=True)]
            except ValueError:
                candidates = []
            for span in _command_substitutions(line):
                try:
                    candidates.append(shlex.split(span, comments=True))
                except ValueError:
                    pass
            for tokens in candidates:
                for i, tok in enumerate(tokens):
                    prev = tokens[i - 1] if i else None
                    if (tok in MARKERS or _is_bare(tok)) and (
                        _in_command_position(prev)
                    ):
                        found.append((src.name, tokens[i + 1:]))
                        break
                else:
                    continue
                break
    return found


def _check(argv: list[str], surface: dict) -> list[str]:
    """The ways one argv disagrees with the clap surface, as sentences."""
    problems = []
    sub = ""
    rest = list(argv)
    if rest and not rest[0].startswith("-"):
        sub, rest = rest[0], rest[1:]
        if sub not in surface["subs"]:
            return [f"unknown subcommand `{sub}`"]
    args = surface["structs"].get(surface["subs"].get(sub, ""), {})
    longs: dict = dict(surface["globals"])
    shorts: set = set(surface["global_shorts"])
    positional_enum = surface["structs"].get("Cli", {}).get("positional_enum")
    if isinstance(args, dict) and args:
        longs = {**longs, **args["longs"]}
        shorts |= args["shorts"]
        positional_enum = args["positional_enum"]
    nested = ""
    if sub == "models" and rest and not rest[0].startswith("-"):
        nested, rest = rest[0], rest[1:]
        if nested not in surface["models"]:
            return [f"unknown `models` subcommand `{nested}`"]
        nargs = surface["structs"].get(surface["models"][nested], {})
        longs = {**longs, **nargs["longs"]}
        shorts |= nargs["shorts"]
        positional_enum = nargs["positional_enum"]
    where = " ".join(p for p in [sub, nested] if p) or "top level"
    i = 0
    while i < len(rest):
        tok = rest[i]
        i += 1
        if tok in ("--",) or ">" in tok or "<" in tok or tok in {
            "||", "&&", ";", "|", "{", "}", "(", ")",
        }:
            break
        if tok.startswith("--"):
            flag, eq, value = tok.partition("=")
            if flag in BUILTIN_LONGS:
                continue
            if flag not in longs:
                problems.append(f"`{flag}` is no flag of `{where}`")
                continue
            if eq:
                values = [value]
            elif longs[flag] is None:
                continue
            elif i < len(rest) and not rest[i].startswith("-"):
                values = [rest[i]]
                i += 1
            else:
                continue
            enum = longs[flag]
            for value in values:
                if not value or value.startswith("$"):
                    continue
                if enum and value not in surface["enums"].get(enum, []):
                    problems.append(
                        f"`{flag} {value}`: `{value}` is no `{enum}` value "
                        f"({', '.join(surface['enums'].get(enum, []))})"
                    )
        elif tok.startswith("-") and len(tok) > 1:
            for ch in tok[1:]:
                if f"-{ch}" not in shorts and f"-{ch}" not in BUILTIN_SHORTS:
                    problems.append(f"`-{ch}` is no flag of `{where}`")
        elif tok.startswith("$"):
            continue
        elif positional_enum and positional_enum in surface["enums"]:
            if tok not in surface["enums"][positional_enum]:
                problems.append(
                    f"`{tok}` is no `{positional_enum}` value "
                    f"({', '.join(surface['enums'][positional_enum])})"
                )
    return problems


def _surface() -> dict:
    structs = _args_structs()
    return {
        "enums": _value_enums(),
        "structs": structs,
        "subs": _subcommands(),
        "models": _models_subcommands(),
        "globals": structs.get("Cli", {}).get("longs", {}),
        "global_shorts": structs.get("Cli", {}).get("shorts", set()),
    }


class SmokeCliSurfaceTests(unittest.TestCase):
    def test_parsers_see_the_cli_surface(self):
        # The contract's two ends: the clap parse must find the
        # subcommands and enums the smokes use, and the smoke scan must
        # find the invocations. Either side going blind fails here rather
        # than approving a drift it cannot see.
        surface = _surface()
        self.assertGreaterEqual(len(surface["subs"]), MINIMUM_SUBCOMMANDS)
        self.assertIn("extract", surface["subs"])
        self.assertIn("models", surface["subs"])
        self.assertGreaterEqual(
            len(surface["enums"].get("Format", [])), MINIMUM_FORMATS
        )
        self.assertGreaterEqual(
            len(surface["enums"].get("Quality", [])), MINIMUM_QUALITIES
        )
        self.assertIn("md", surface["enums"].get("Format", []))
        self.assertNotIn("markdown", surface["enums"].get("Format", []))
        self.assertGreaterEqual(len(_invocations()), MINIMUM_INVOCATIONS)

    def test_every_smoke_flag_and_value_exists_in_clap(self):
        # `--format markdown` beside `extract` is what m490 found: the
        # CLI offers `md`, so the smoke's markdown step died before it
        # extracted anything. Every literal flag and enum value on every
        # smoke argv must exist in the clap surface (variable values like
        # the advertised-format loop's `$fmt` are skipped: the smoke feeds
        # them from `--help` at run time).
        surface = _surface()
        offenders = []
        for name, argv in _invocations():
            for problem in _check(argv, surface):
                offenders.append(f"{name}: {' '.join(argv)}: {problem}")
        self.assertEqual(offenders, [])


if __name__ == "__main__":
    unittest.main()
