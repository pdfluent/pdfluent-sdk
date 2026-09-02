#!/bin/sh
# How a single render attempt is judged: ok, crash, or hang.
#
# This lived inline in .github/workflows/crash-guard.yml, where nothing could
# run it. Two consequences, and the second is why it moved out here:
#
#   * `timeout 30 ... || true` threw the exit code away and judged the run from
#     stderr alone. A process the timeout killed writes nothing to stderr, so a
#     PDF that hung the renderer for thirty seconds counted as clean -- the one
#     outcome a crash guard exists to catch, scored as a pass.
#   * a classifier embedded in a workflow is a classifier no test can drive, so
#     that gap survived every green run of the gate.
#
# Called as: classify_render_outcome <exit-code> <stderr-file>
# Prints exactly one of: ok | crash | hang
classify_render_outcome() {
    _rc="$1"
    _err="$2"

    # `timeout` exits 124 when it fires, and 137 when the process had to be
    # SIGKILLed (its own -k, or the kernel's OOM killer). Neither leaves a
    # panic message behind, so this has to be judged on the code, not the text.
    if [ "$_rc" -eq 124 ] || [ "$_rc" -eq 137 ]; then
        echo hang
        return 0
    fi

    # A real crash announces itself. An ordinary non-zero exit does not:
    # PasswordProtected, an unsupported format and a malformed file are the
    # binary behaving correctly, and this gate is not a quality gate.
    if [ -f "$_err" ] && grep -qE "panicked at|SIGSEGV|SIGABRT|Aborted|Bus error|Killed|stack backtrace|core dumped|signal: [0-9]" "$_err"; then
        echo crash
        return 0
    fi

    echo ok
}
