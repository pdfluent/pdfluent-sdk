#!/usr/bin/env python3
"""D13A — Runner integration guards for the `pdfluent measure` subcommand.

Verifies that run_fresh_merge_policy_comparison.py targets the D13A `measure`
contract: `measure` subcommand, `--output-json` (not `--metrics-out`), and the
canonical short policy tokens.

Stdlib only. Run in gates:
  python3 scripts/xfa_fidelity/test_measure_runner_integration.py
Exit 0 = all guards pass.
"""
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RUNNER = "run_fresh_merge_policy_comparison"

FAILS = []


def check(cond, msg):
    if not cond:
        FAILS.append(msg)


def _load(name):
    spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, name + ".py"))
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def guard_policy_cli_token_mapping():
    """Internal policy enum names map to canonical measure CLI tokens."""
    mod = _load(RUNNER)
    check(
        mod._policy_cli_token(mod.POLICY_SSF) == "saved-state",
        "SavedStateFaithful must map to 'saved-state'",
    )
    check(
        mod._policy_cli_token(mod.POLICY_FM) == "fresh-merge",
        "FreshMergeExperimental must map to 'fresh-merge'",
    )
    # Unknown policy passes through unchanged (defensive).
    check(
        mod._policy_cli_token("something-else") == "something-else",
        "unknown policy should pass through unchanged",
    )


def _measure_cmd_block(src):
    """Return the text of the `cmd = [ ... ]` list in run_measurement."""
    start = src.index('cmd = [', src.index('def run_measurement'))
    end = src.index(']', start)
    return src[start:end]


def guard_runner_targets_measure_contract():
    """The measure invocation uses `--output-json`, not `--metrics-out`/`--timeout`."""
    with open(os.path.join(HERE, RUNNER + ".py"), encoding="utf-8") as fh:
        src = fh.read()
    cmd = _measure_cmd_block(src)
    check('"measure"' in cmd, "runner must invoke the `measure` subcommand")
    check('"--output-json"' in cmd, "measure cmd must pass --output-json")
    check('"--metrics-out"' not in cmd, "measure cmd must NOT use the old --metrics-out flag")
    # The runner keeps its own --timeout argparse option (subprocess bound), but
    # must NOT forward --timeout to the measure binary (which has no such flag).
    check('"--timeout"' not in cmd, "measure cmd must NOT forward --timeout to the binary")


def guard_binary_autodetect_includes_pdfluent():
    """Binary auto-detection should find the `pdfluent` binary name."""
    mod = _load(RUNNER)
    # _resolve_binary returns None when no binary exists locally; we only assert
    # that the candidate list (as reflected in source) includes pdfluent.
    with open(os.path.join(HERE, RUNNER + ".py"), encoding="utf-8") as fh:
        src = fh.read()
    check("pdfluent" in src, "runner binary auto-detect must include 'pdfluent'")


def main():
    guard_policy_cli_token_mapping()
    guard_runner_targets_measure_contract()
    guard_binary_autodetect_includes_pdfluent()

    if FAILS:
        print("FAIL — measure runner integration guards:")
        for f in FAILS:
            print(f"  - {f}")
        sys.exit(1)
    print("OK — measure runner integration guards pass (3 guards)")


if __name__ == "__main__":
    main()
