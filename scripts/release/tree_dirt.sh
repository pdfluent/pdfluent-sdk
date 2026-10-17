#!/usr/bin/env bash
# tree_dirt.sh — what counts as a dirty tree on the publish path, in one place.
#
# Source it and call `pdfluent_tree_dirt <repo> [crate] [version]`. It prints the
# `git status --porcelain` lines that should stop the run, and nothing when the
# tree is clean enough to publish from.
#
# WHY IT IS A FILE
#
# The rule lived twice, in prepublish_crate_audit.sh and in
# publish_ordered.sh, and on 16-09-2026 only one of the two copies learned it
# (#358). The audit had been taught that its own reports are not dirt -- it
# writes three of them per crate into a directory that is not gitignored, so the
# audit of crate N was leaving exactly the dirt that made the audit of crate N+1
# refuse. publish_ordered.sh's own check at the top of the run had the same
# defect and kept it: it only bites on a RESUME, because the first run of a
# publish starts from a clean tree, so three runs went past it and the fourth
# did not.
#
# THE RULE
#
# 1. Untracked output under benchmarks/runs/prepublish_audits/ is not dirt. It
#    is what this run has already written.
# 2. With a crate and a version, the three report files THAT run is about to
#    overwrite are not dirt either, whatever their git status: once a release's
#    reports are committed -- which PUBLISH_PROTOCOL.md 4 asks for -- auditing
#    that version again rewrites them, because they carry the temporary tree
#    path the run used.
# 3. Everything else is dirt, including a TRACKED report of another crate or
#    another version that was edited, staged or deleted. That is a change to the
#    evidence of an earlier release, and PUBLISH_PROTOCOL.md 7 says the audit
#    must refuse it.
#
# Without a crate name, only rule 1 applies -- which is right for
# publish_ordered.sh, whose check runs once, before the first audit, when
# nothing has been rewritten yet.
#
# TRAIN MODE. Within one audit-all-packages.sh run, every report the train
# already wrote is this run's own output -- including TRACKED reports it
# rewrote (once a release's reports are committed, re-auditing that version
# rewrites them with a fresh temporary path inside, which reads as ` M`).
# Excusing only the current crate's three files stopped the train at the
# second crate. So when PDFLUENT_AUDIT_TRAIN=1, the whole reports directory
# is excused, whatever the git status of the files under it. Anything
# outside that directory still fails. audit-all-packages.sh sets this for
# its per-crate calls only; a standalone audit keeps the strict rules.

PDFLUENT_AUDIT_REPORTS="benchmarks/runs/prepublish_audits"

pdfluent_tree_dirt() {
    local repo="$1" crate="${2:-}" version="${3:-}"
    if [ "${PDFLUENT_AUDIT_TRAIN:-}" = "1" ]; then
        git -C "${repo}" status --porcelain \
            | grep -Ev "^.. \"?${PDFLUENT_AUDIT_REPORTS}/" \
            || true
        return 0
    fi
    local own_filter='^$a^'          # matches nothing when there is no crate
    if [ -n "${crate}" ] && [ -n "${version}" ]; then
        local own
        own=$(printf '%s' "${PDFLUENT_AUDIT_REPORTS}/${crate}-${version}" \
            | sed -e 's/[][\.^$*+?(){}|\/]/\\&/g')
        own="^.. \"?${own}(\.md|\.audit\.md|\.audit\.json)\"?\$"
        own_filter="${own}"
    fi
    git -C "${repo}" status --porcelain \
        | grep -Ev "^\?\? \"?${PDFLUENT_AUDIT_REPORTS}/" \
        | grep -Ev "${own_filter}" \
        || true
}
