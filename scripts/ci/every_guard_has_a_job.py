#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every script in `scripts/ci/` and `scripts/release/` must be called by a
pipeline job -- and by one that can still block a merge.

This is the rule from CLAUDE.md, made mechanical: *nothing is done if there is
no test for it that runs in the CI pipeline.* That rule exists because the
alternative went wrong three times, each time the same way -- the test existed
and nothing ran it.

On 25-08-2026 it happened again, inside the guards themselves. A ratchet on how
many site examples fail to compile had no job at all. It existed, nothing ran
it, and it would first have meant something on the day somebody called it by
hand.

There is a second way for a guard to mean nothing, and it is harder to see.
Since 25-08-2026 GitHub is the main remote and GitLab a nightly mirror of it. A
job declared only in `.gitlab-ci.yml` runs after the night, on a copy, against a
branch nobody is merging. Nothing it finds can stop anything. Two of those had
already cost something by the time they were counted: the real PDF/A gate ran on
the mirror while a broken second gate that could not fail stood before the merge
(#286), and the advisory scan sat on a runner without docker for months (#287).

So this file asks three questions, in order:

  1. Does every script have a job somewhere?         -> orphans, below
  2. Does every mirror-only guard have a written     -> mirror_only_guards.toml
     reason for staying on the mirror?
  3. Is the number of mirror-only guards still       -> SPIEGEL_RATEL
     the number written down here?

A script that deliberately does not belong in CI -- a tool run by hand, a
generator -- is listed below with its reason.

Exit codes:
  0  every guard is accounted for
  1  a guard runs nowhere, runs only on the mirror without a reason, the count
     moved in either direction, or an exemption names the local gate as its
     place and the local gate does not call it
"""
from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
CI = REPO / ".gitlab-ci.yml"
WORKFLOWS = REPO / ".github" / "workflows"
MAP = REPO / "scripts" / "ci"
# The release lane grew its own scripts, and a guard the scan cannot see is a
# guard that can die silently: the scan covers both homes. Basenames are
# unique across the two, so the register below keeps matching on the name.
RELEASE_MAP = REPO / "scripts" / "release"
REGISTER = MAP / "mirror_only_guards.toml"
GATE = MAP / "local_ci_gate.sh"

# FLOOR: scripts found >= 400 -- scripts/ci plus scripts/release hold 462.
# If this script finds fewer, the search is broken and the directories are not
# empty; an empty scan would approve everything, which is exactly the mistake
# this file exists to catch.
MIN_SCRIPTS = 400

# RATCHET: guards that run only on the mirror -- not more than this, and not
# fewer either.
#
# The name is kept from the version of this file on `chore/test-reachability-gate`,
# where the two-way ratchet was introduced, so that the two converge rather than
# drift apart.
#
# Why equal and not less-than-or-equal: if the number goes down, room has been
# won, and a ratchet that only looks upward lets that room quietly fill again.
# Failing downward costs one line in the commit that books the win and keeps the
# number honest.
#
# 31-08-2026 (#288): 25 -> 12. Thirteen guards got a job in
# .github/workflows/ci.yml; the remaining twelve are in mirror_only_guards.toml
# with the reason they stay.
#
# 02-09-2026 (#307): 12 -> 13. Not a guard moved onto the mirror: a guard that
# had always stood before no merge was counted for the first time. The lines a
# job runs were read without asking whether the job can run on a pull request,
# so `tests_actually_ran.py`, called only from a workflow_dispatch-only
# workflow, counted as "has a job" (codex, #1633). The count now includes
# every guard whose only GitHub job is dispatch-only or parked behind
# `needs: parked`, and that one is in mirror_only_guards.toml with its reason.
#
# 02-09-2026 (#276): 13 -> 14. Not a guard at all: crash-guard.yml's verdict --
# ok, crash or hang -- moved out of the workflow into
# classify_render_outcome.sh so that a test could drive it, and a shell function
# in scripts/ci counts here like everything else. Its caller is parked behind
# `needs: parked`, so it stands before no merge; the suite that drives it does,
# on every pull request. Reason in mirror_only_guards.toml.
#
# WHAT THIS NUMBER DOES NOT SAY
# It says "also runs in GitHub Actions", not "an outside contributor sees it".
# The GitHub remote of this repository is private, so none of these guards
# touches a pull request from outside today. That is the gap in #232, and this
# number does not measure it -- it measures the step towards it.
# 06-09-2026 (#343): 14 -> 11. Three guards stopped standing before no merge:
# run_build.sh, run_clippy.sh and run_test.sh. They were excused as
# "covered-elsewhere" on ci-ephemeral.yml, whose provider was cancelled on 24-08
# and whose workflow has been disabled by hand since -- so the place that covered
# them had stopped existing while the register went on saying it did. ci.yml's
# `workspace` job runs those three scripts on every push to master now, which is
# what lets the landing lane compile only the crates a landing touches.
# 11 -> 16 on 06-09-2026 (#350). Five, in one change, and the direction this
# ratchet exists to make deliberate: the workspace build and three desktop
# preconditions left the push when the heavy work moved to a bought-and-thrown-
# away instance. Nine other guards would have joined them if `schedule` had gone
# on counting for nothing; the wave counts, and those nine did not need a row.
# 09-09-2026 (#482): 17 -> 18. One guard, and it is the one that would have
# caught the fault it was written for: a_gate_image_answers_ssh.py boots
# instances from a candidate image before it is given the name every gate
# resolves. It cannot stand before a merge -- it spends money and needs the
# Hetzner token -- and it is named in the step that runs it, in the nightly
# refresh. Reason in mirror_only_guards.toml.
# Owner disabled paid compute on 2026-09-12; eleven suites deferred explicitly.
# 27-09-2026 (#623): 29 -> 30. run_excluded_dependents.sh, the native check of
# xfa-wasm when a landing reaches it: a compiling gate, standing where run_test.sh
# stands. Reason in mirror_only_guards.toml.
# 01-10-2026 (#636): 30 -> 29. tests_actually_ran.py stands before a merge
# now: ci.yml's `clitest` job runs the CLI suite through it on every push,
# so the wrapper that only capi-ephemeral.yml (dispatch-only) called is a
# push gate. Entry removed from mirror_only_guards.toml.
# 01-10-2026 (#636 follow-up): 29 -> 30. `clitest` compiled on the persistent
# desktop, turning desktoplast, orchhost and builddir3 red at once, so it moved
# to a wave job in ci-ephemeral.yml (#350) -- which fires on workflow_dispatch
# alone. The wrapper's callers are dispatch-only again; the CLI half still runs
# on every landing in the local gate's landing lane. Entry restored.
# 01-10-2026 (#636): 30 -> 31. build_all_platforms.sh left ALLOWED for a
# dispatch-only job in build-lane.yml: it builds the release channels, so it
# stands before no merge. Reason in mirror_only_guards.toml.
# 03-10-2026 (#636): 31 -> 39. The eight consumer smokes stand only in
# dispatch-only release jobs -- the release-natives `smoke` job, the
# release-binaries and wasm release gates -- where no push has packaged
# artefacts for them to judge. Reasons in mirror_only_guards.toml.
# 08-10-2026 (#636): 39 -> 40. smoke_ort_variant.sh, the ninth consumer
# smoke: its only caller is the dispatch-only release-natives `smoke` job,
# and its toml entry already says so -- the number follows it.
# 10-10-2026 (#636): 40 -> 49. The nine compiling binding suites m722 moved
# out of ci.yml's push job into dispatch-only binding-suites.yml: the desktop
# guards refuse any compile on a push, and hosted minutes on a push are
# refused too, so no push job can carry them. The same scripts ride the
# landing lane; reasons in mirror_only_guards.toml.
# 15-10-2026: 49 -> 50. node_glibc_floor.py, the glibc floor over the two
# linux-gnu Node bindings: its only caller is the dispatch-only release-gate
# of node-bindings.yml, where the fresh bindings exist. Its test runs in
# ci.yml; reason in mirror_only_guards.toml.
# 15-10-2026: 51 -> 53. binary_host_paths.py and check_build_host_paths.sh,
# the build-host path check over fresh release natives: their only callers
# are the dispatch-only release workflows, where those natives exist. Their
# tests run in ci.yml; reasons in mirror_only_guards.toml.
SPIEGEL_RATEL = 53

# The reasons that can justify a place on the mirror. Free text would approve
# every reason, including "later".
REASONS = {"corpus", "runner", "covered-elsewhere", "credentials"}

# Scripts that deliberately have no job, with the reason. A name here without a
# reason should not exist.
ALLOWED: dict[str, str] = {
    "require_paid_compute_permission.py": (
        "denial step for paid manual workflows; its expected nonzero exit and "
        "resistance to environment overrides are tested in push CI by test_paid_compute_policy.py"
    ),
    "landing_test_scope.py": (
        "local pre-push selection helper; its decisions and the real shell skip "
        "boundary are exercised by test_landing_test_scope.py in push CI"
    ),
    "rig_data_stays_on_the_rig.py": (
        "imported by the harness, the DocLayNet dump and the proxy, which refuse through it; "
        "its decisions are exercised by test_rig_data_stays_on_the_rig.py in push CI"
    ),
    "development_gate_scope.py": (
        "local pre-push selection helper; its decisions and the real shell deferral "
        "boundary are exercised by test_development_gate_scope.py in push CI"
    ),
    "every_guard_has_a_job.py": "this file itself; its own job does name it",

    # It compiles the Node addon and the engine under it (#587), and the
    # push-to-master runner does no build work; node-bindings.yml, which would,
    # fires on a tag. It runs in the landing lane as a crate gate on pdf-node,
    # and the catalogue half of the same contract runs in ci.yml.
    "the_node_error_contract_holds.sh": (
        "builds the pdf-node addon; runs in scripts/ci/local_ci_gate.sh as a "
        "crate gate on pdf-node, and error_catalogue_sync.sh with its test runs in ci.yml"
    ),

    # ON LOAN (#367), because this guard refused a landing over something that
    # was not in the change being landed. It judges one thing: whether the
    # hourly mirror agent is loaded and last finished by pushing. Its own
    # docstring says why a pipeline job is the wrong home -- the marker is the
    # agent's launchd plist, and where there is no plist there is no claim, so a
    # job on a runner or in a contributor's clone would be red about a machine
    # it is not. It runs where the answer exists, which is the local gate, and
    # this entry names that place so the check above holds the claim to being
    # true rather than taking it on trust. Its test runs on every pull request.
    "the_mirror_agent_is_running.py":
        "machine-local: it judges the mirror agent's launchd plist, which no "
        "runner has. Runs in scripts/ci/local_ci_gate.sh, where the plist is; "
        "its test runs in ci.yml (#231, added by d43dba4ed)",

    # Deliberately by hand, and the reason is the whole design (#237). A job
    # would ask "does this already exist?" after the work was done, which is the
    # one moment at which the answer is worthless. It belongs at the start of a
    # story, in a pair of hands, and CLAUDE.md puts it there as step zero. Its
    # TEST does run, on every pull request, over a fake `gh`.
    # The measurement needs a full clone of LaurenzV/hayro -- 948 MB, with the
    # history, because the fork point is a commit and not a version number. No
    # runner carries that, and fetching it per run would be minutes of network
    # for a table that changes when the fork moves, which is a handful of times
    # a year. Run by hand with HAYRO_CLONE set; the committed table is the
    # result, and `--check` refuses a drifted one.
    #
    # What CAN go wrong in silence runs on every pull request as
    # test_fork_divergence.py: no clone reported as no divergence, a guessed
    # fork point reported as a measured one, an empty table as a clean one.
    "fork_divergence.py": (
        "needs a 948 MB clone of upstream with its history; run by hand with "
        "HAYRO_CLONE, and test_fork_divergence.py runs in CI"
    ),

    # It needs a tracker token AND this repository's history at the same time,
    # and no runner has both: `gh` is not on the self-hosted runner, and a
    # fork's pull request must never carry a token that reads a private tracker.
    # It stands in scripts/ci/local_ci_gate.sh, which is where a terminal closes
    # a story and where both halves are present, and reports SKIPPED (not a
    # pass) on a machine without access. The same reason is written down from
    # the other side in docs/GATES_REACHABLE_FROM_A_PULL_REQUEST.toml, and its
    # test runs on every pull request.
    "a_closed_story_names_a_landed_sha.py": (
        "needs a tracker token and this repository's history at once; ADVISORY "
        "in scripts/ci/local_ci_gate.sh because the tracker is not in anybody's "
        "push, and its test runs as a hard gate in CI"
    ),

    # The three halves of the PDF/A corpus (#377). None of them is a guard.
    #
    # `pdfa_corpus_rule.py` is a module -- the seed, the order it induces and
    # the classifier -- imported by the guard that DOES run on every push,
    # a_manifest_pins_every_file.py, and by its mutation test. A job that ran it
    # would run nothing: it has no main.
    "pdfa_corpus_rule.py": (
        "a module, not a script: the seed and the classifier, imported by "
        "a_manifest_pins_every_file.py and its test, both of which run in ci.yml"
    ),

    # Seventeen gigabytes of ranged requests against half a terabyte of upstream
    # archives, and hours of it. It draws the corpus once; what has to be
    # checked on every push is its OUTPUT, and that is the guard.
    "build_pdfa_corpus_manifest.py": (
        "draws the 20,000-document corpus by hand: hours and ~17 GB of network "
        "against digitalcorpora.org. Its output is checked on every push by "
        "a_manifest_pins_every_file.py, whose own test also runs there"
    ),

    # The layer above it (#461): a thousand-document tuning set and a
    # three-hundred-document test set per capability. Same shape, same reason --
    # a rule that is imported and a builder that writes, with the guard on the
    # output being the thing that runs on every push.
    "tuning_and_test_rule.py": (
        "a module, not a script: the seed and the draw of #461, imported by "
        "the_tuning_and_test_sets_are_the_drawn_sets.py and its mutation test, "
        "both of which run in ci.yml"
    ),
    # The same shape once more, for the thousand #365 publishes off. The rule
    # is imported; the builder's `hash` stage moves half a gigabyte of ranged
    # requests against digitalcorpora.org to give a list of names its digests,
    # and is run once per draw.
    "holdout_1000_rule.py": (
        "a module, not a script: the seed and the exclusion of #365, imported "
        "by the_holdout_1000_is_the_drawn_set.py and its mutation test, both "
        "of which run in ci.yml"
    ),
    # And once more for the structured-extraction split of #566. The shape is
    # the same and the reason is: the rule is imported by the guard on its
    # output and by the mutation test that keeps that guard honest, and both of
    # those run on every push.
    "structured_holdout_rule.py": (
        "a module, not a script: the seed and the by-document split of #566, "
        "imported by the_structured_holdout_is_the_drawn_set.py and its "
        "mutation test, both of which run in ci.yml"
    ),
    "build_holdout_1000_manifest.py": (
        "its `hash` stage fetches and hashes a thousand GovDocs1 members over "
        "the network, once per draw; its `write` stage only rewrites what "
        "the_holdout_1000_is_the_drawn_set.py redraws and compares on every "
        "push, which is where the verdict belongs"
    ),
    "build_tuning_and_test_manifests.py": (
        "writes the four manifests of #461. Running it in a job would only "
        "rewrite what the guard already redraws and compares on every push: "
        "the_tuning_and_test_sets_are_the_drawn_sets.py, which is where the "
        "verdict belongs"
    ),

    # Thirty gigabytes and forty minutes, run by the machine that is about to
    # measure. A pull-request gate that fetched the corpus would be a gate that
    # spends an hour to check what the manifest already states.
    "fetch_pdfa_corpus.py": (
        "materialises 10,000 documents on a measuring machine: ~30 GB and "
        "tens of minutes. The manifest it reads is guarded on every push by "
        "a_manifest_pins_every_file.py"
    ),

    "does_this_already_exist.py": (
        "asked by a person before starting a story, not by a job afterwards; "
        "step zero in CLAUDE.md, and test_does_this_already_exist.py runs in CI"
    ),

    # A cargo build of five hundred small programs -- every Rust block on
    # pdfluent.com, compiled against the real facade. It belongs in the full
    # local lane, which runs on a push to master, and not in a hosted job whose
    # minutes #333 is trying to get to zero while the repository is private.
    #
    # It is not unguarded for that. The half of it that needs no compiler --
    # the exception register read in both directions, and the hand-over digest
    # the website refuses its own build against -- runs on every pull request as
    # test_site_blocks_register.py, in seconds. docs/GATES_REACHABLE_FROM_A_PULL_REQUEST.toml
    # carries the same reason from the other side (#247).
    "site_blocks_compile.py": (
        "a cargo build of every Rust block on the site; runs in the full lane "
        "of scripts/ci/local_ci_gate.sh, on a push to master. Its register and "
        "hand-over half runs on every pull request as test_site_blocks_register.py"
    ),

    # ON LOAN (#247, from #260/#331). Not this file's owner's change: the entry
    # was simply never mirrored here, so this guard has stood red on master for
    # everyone, on a reason that is already written down one register over.
    #
    # It reads the withdrawn-object list from ~/.config/pdfluent, deliberately
    # outside the tree -- publishing the blocklist would publish where the
    # withdrawn thing can still be fetched. A hosted runner has no such file, so
    # in a workflow it can only skip, and a green tick for a check that asked
    # nothing is worse than the red. Its TEST does run, in publication-guards.yml.
    # The same reason stands in docs/GATES_REACHABLE_FROM_A_PULL_REQUEST.toml.
    "no_withdrawn_object.py": (
        "reads ~/.config/pdfluent, outside the tree by decision (#260), so a "
        "hosted runner can only skip it. Runs in scripts/ci/local_ci_gate.sh; "
        "test_no_withdrawn_object.py runs in publication-guards.yml"
    ),
    "the_repositories_stand_as_intended.py": (
        "lists the organisation's private repositories, which needs gh signed in "
        "as a member; GITHUB_TOKEN sees only its own repository (#637). Runs in "
        "scripts/ci/local_ci_gate.sh; test_the_repositories_stand_as_intended.py "
        "runs in publication-guards.yml"
    ),

    # Cannot work in a workflow, by its own measurement: /actions/runners needs
    # `administration: read`, which GITHUB_TOKEN cannot be granted, and there is
    # no `gh` on the runner. Wired into a job it printed SKIPPED and returned 0
    # on every run -- a green tick for a check that never asked anything, while
    # this file counted the filename in a `run:` and called it enforced.
    #
    # It runs in the local gate, where `gh` is authenticated, and it now exits 1
    # rather than 0 when CI is set, so wiring it again fails loudly instead of
    # quietly. A token for it is an owner decision (#290). (codex, #1639)
    "every_label_has_a_runner.py": (
        "needs `gh` with a keyring; GITHUB_TOKEN cannot be granted the scope "
        "/actions/runners requires, so in a workflow it can only skip. Runs in "
        "scripts/ci/local_ci_gate.sh"
    ),

    # Not a guard. A shared helper the fixtures import; a job for it would run
    # a module with no verdict and report success for having imported it. It
    # arrived with #1647 and was counted here as an orphan on every run, which
    # is the register asking a fair question about a file that does not belong
    # in it.
    # Not in CI, and by design: a release recheck a person runs before shipping,
    # from scripts/quality/run_quality_release_rechecks.sh. It builds a wasm
    # package and drives Chromium through Playwright, which is minutes of work
    # for a question nobody asks per pull request. It became visible to this
    # register when the scan started reading subdirectories (#366); it had been
    # running nowhere and reported by nothing before that, which is the older
    # of the two problems.
    "run_qr10_wasm_browser.sh": (
        "a release recheck run by hand from "
        "scripts/quality/run_quality_release_rechecks.sh; it builds a wasm "
        "package and drives Chromium, and exits 2 (SKIP) where neither is there"
    ),

    # Not a guard either, and for the same reason: the capability ratchets of
    # the improvement loop (#366) import it, and a job for it would run a module
    # with no verdict and report success for having imported it. What it holds
    # is the comparison every ratchet shares -- so it is covered by their tests,
    # not by a job of its own: test_pdfa_ratchet.py drives every branch of it on
    # every pull request, including the sentinel rule that has no PDF/A caller.
    "ratchet.py": (
        "the shared comparison the capability ratchets import, not a check: it "
        "has no verdict to report. scripts/ci/loop/test_pdfa_ratchet.py drives "
        "it in CI"
    ),

    "fixture_env.py": (
        "the sealed-environment helper the fixtures import, not a check: it "
        "has no verdict of its own, and what it does is proven by "
        "test_a_fixture_cannot_touch_a_real_repo.py"
    ),

    # Same shape again (#443): a declaration, not a check. It says which PDF/A
    # levels the converter targets and which baseline file each one owns, and
    # three callers read it -- the conformance gate, the capability register and
    # a_pdfa_flavour_has_a_baseline.py. A job of its own would import a module
    # with no verdict and report a pass for having imported it.
    "pdfa_flavours.py": (
        "the flavour declaration the conformance gate, the capability register "
        "and a_pdfa_flavour_has_a_baseline.py read, not a check: it has no "
        "verdict to report. scripts/ci/test_pdfa_conformance_gate.py and "
        "scripts/ci/test_a_pdfa_flavour_has_a_baseline.py drive it in CI"
    ),

    # ---- publication guards, split onto master ahead of their wiring (#222) ----
    # These arrived from t3/1543-resolve so they can be read and reviewed on
    # master. every_document_is_registered.py is no longer among them: the seven
    # documents its PUBLIC_TREE entries name -- which until 02-09-2026 existed
    # only on #1543, so the register stood red on master by decision rather than
    # delete the protective lines -- landed byte-identical from that branch, and
    # the guard runs in .github/workflows/document-register.yml.
    "corpus_herkomst.py": (
        "the provenance register, imported by every_document_is_registered.py "
        "(document-register.yml), which asks herkomst() for every tracked PDF; "
        "its own main() regenerates docs/CORPUS_HERKOMST.md and is run by hand"
    ),
    # The other three measure the tree as it stands and could be wired today; they
    # are held back only so they land and are reviewed as one set.
    # Pre-push only, and that is where it decides: master takes fast-forwards
    # only, so the local gate IS the merge point. Its CI counterpart is the
    # inline "Every commit written since the DCO carries a matching sign-off"
    # step in the same workflow, which asks the same question of a pull
    # request's own commits; a second job running this script would ask it
    # twice. Its test IS wired, in orchestration-guard. (#316)
    "every_commit_since_the_cutoff_is_signed.py": (
        "the pre-push half of the #316 sign-off gate; CI asks the same question "
        "in orchestration-guard's inline step, and this script's test runs there"
    ),
    "geen_interne_zaken.py": "wired with the other publication guards after #1543",
    "simulate_public_tree.py": "wired with the other publication guards after #1543",
    "herkomsttabel.py": (
        "generates docs/HERKOMST.md and is imported by header_sweep for the list "
        "of forked crates; wired with the set after #1543"
    ),
    "header_sweep.py": (
        "red on master on purpose: 18 of 630 own source files carry the old "
        "proprietary header and the rest carry none, which the flip (#257) and "
        "the header commit on #1543 fix together"
    ),
    # Diagnostics, not gates. Both answer a question a human is already asking
    # when something has gone wrong; neither has a verdict a pipeline could act
    # on. A job for either would report success for having run.
    "bounded_probe.sh": (
        "touches a filesystem that may be dead, on purpose and with a bound. "
        "It is the tool you reach for when a mount hangs, and it has no pass "
        "or fail of its own"
    ),
    "why_not_writable.sh": (
        "tells a human which of three causes made a path unwritable. It "
        "diagnoses an already-failed gate rather than being one"
    ),

    # Written 02-09-2026 (#307) while ci.yml was frozen for #1666. It belongs in
    # orchestration-guard next to test_gate_corpus_is_pinned.py, and that is a
    # one-line change to make the moment ci.yml is open again; until then it
    # runs by hand: python3 scripts/ci/test_gate_corpus_no_crash.py. Listed
    # here so the debt is visible, not so it is excused -- remove this entry in
    # the change that adds the step.
    "test_gate_corpus_no_crash.py": (
        "holds the no-crash gate to stopping at its budget while running; "
        "awaiting its run: line in ci.yml, which #1666 has frozen (#307)"
    ),

    # Deliberately local: they exist to do something *before* the pipeline.
    "local_ci_gate.sh": (
        "runs the pipeline gates locally before a push; a job for this would "
        "have the pipeline check itself"
    ),
    # The requirements preflight (m482): it runs first inside every lane of
    # scripts/ci/local_ci_gate.sh -- which is where a missing binary would
    # otherwise surface an hour in -- and as the first step of each
    # self-hosted release job, after setup-selfhosted. A push job for the
    # lane flavour would have the pipeline check itself; the release flavour
    # runs in dispatch-only release jobs. Held by
    # test_gate_preflight.py, which runs in ci.yml.
    "gate_preflight.py": (
        "the requirements preflight: runs in scripts/ci/local_ci_gate.sh as "
        "the first step of every lane, and in the dispatch-only release jobs "
        "after setup-selfhosted. Held by test_gate_preflight.py in ci.yml"
    ),
    # A wrapper, not a guard: it takes the machine-wide gate lock, runs whatever
    # command it was handed, and passes that command's exit status back. It has
    # no verdict of its own, so a job for it would assert nothing. What it does
    # IS tested -- test_one_gate_at_a_time.sh drives it over a private lock
    # directory (cases 10 to 13) and runs in the fast lane and in ci.yml.
    "with_gate_lock.sh": (
        "a wrapper with no verdict of its own: it runs someone else's command "
        "under the gate lock of #359 and returns that command's status. Tested "
        "by test_one_gate_at_a_time.sh, which has a job"
    ),
    # Reads a table that only exists on a machine where the local gate has run.
    # A hosted runner has never written one, so in a workflow this guard can
    # only skip -- and a green tick for a check that looked at nothing is the
    # failure this file was written to prevent. It runs in
    # scripts/ci/local_ci_gate.sh, where the table is; its test runs in ci.yml.
    # Machine-local for the same reason, and a sharper one: what it judges is
    # the per-worktree git config of the clone it runs in. A CI checkout has one
    # worktree, made seconds earlier by actions/checkout, so in a workflow this
    # guard can only report "not judged" -- and a green tick for a check that
    # looked at nothing is what this file exists to prevent. It runs in
    # scripts/ci/local_ci_gate.sh, on the machine where the worktrees are; its
    # test builds its own clone and runs in ci.yml.
    "every_worktree_runs_the_tracked_hooks.py": (
        "machine-local: it judges the per-worktree core.hooksPath of this clone, "
        "and a CI checkout has one worktree that never has it set. Runs in "
        "scripts/ci/local_ci_gate.sh, and test_every_worktree_runs_the_tracked_"
        "hooks.py runs in ci.yml (#454)"
    ),

    "the_fast_lane_stays_under_its_budget.py": (
        "reads the local gate's own timing table, which exists only on a machine "
        "that has run the gate; in a workflow it could only skip. Runs in "
        "scripts/ci/local_ci_gate.sh, and test_the_fast_lane_stays_under_its_"
        "budget.py runs in ci.yml"
    ),

    # Tools a person runs to know something, not a gate that fails.
    "runner_busy_check.sh": (
        "run by hand before a heavy run -- the runner shares one machine with "
        "the corpus measurements"
    ),
    "check_runner_contention.sh": "likewise, hand diagnosis of runner contention",
    "indirect_font_keys_prevalence.py": (
        "a one-off corpus measurement that supported a design decision, not a gate"
    ),
    "qr11_binding_runtime_mapping.sh": "a one-off quality review (QR-11)",

    # Says so itself, in its own first line: "Run by hand, not in CI." It talks
    # to github.com and raw.githubusercontent, reads each source's file list at a
    # pinned commit and downloads every pick. A job for it would put a network
    # fetch of somebody else's repository on the critical path of every build,
    # and the thing it produces -- corpus/CI_CORPUS_MANIFEST.json -- is committed,
    # so the pipeline reads the result rather than re-earning it.
    "build_gate_corpus_manifest.py": (
        "builds the gate-corpus manifest by downloading from upstream suites; run "
        "by hand when the manifest changes, and its output is committed"
    ),

    # Release moment, not every commit.
    # Maintenance, run when the disk-headroom floor is reached from below --
    # exactly the moment a job must not start. A job for it would delete build
    # caches on a schedule nobody chose. (#298)
    "sweep_merged_build_caches.py": (
        "reclaims target/ from worktrees whose work is already on master; a "
        "maintenance action, run when the floor says there is no room"
    ),
    "run_release_drift_check.sh": (
        "belongs on a pipeline schedule, not on a push"
    ),
    "qr9_sanitizers.sh": (
        "ASAN/LSAN plus Miri over the memory-safety selection; a release gate, "
        "needs nightly plus rust-src and runs for tens of minutes"
    ),
    "check_large_blobs.sh": (
        "runs as a git hook before a commit, not in the pipeline -- that is the "
        "moment the blob can still be stopped"
    ),

    # Needs something a pipeline checkout does not have.
    "verapdf_cached.py": (
        "a cache layer around veraPDF, not a verdict of its own: it is handed "
        "to the corpus gates with --verapdf-path. Its behaviour is checked by "
        "test_verapdf_cached.py, which runs in promise-guard on GitHub"
    ),
    "infra_health.py": (
        "reads live systems -- the runner list, the Actions budget -- and needs "
        "gh plus a host token for it. The answer is about today's machines and "
        "not about the change under review, so failing a merge on it stops the "
        "wrong person. test_infra_health.py runs in orchestration-guard and "
        "holds it to telling a working machine from a leaked one. OPEN: this "
        "belongs on a schedule (#288)"
    ),
    # CORRECTED 05-09-2026 (#231). What stood here said "It runs on the machine
    # that mirrors, where both remotes exist" -- and no job, schedule or hook
    # called it anywhere. The backup then fell 348 commits behind and 8 ahead at
    # the same time and stayed that way for three days, behind a guard this file
    # was booking as placed. A reason that names a place is now checked against
    # that place, below, so this cannot be written again without being true.
    "mirror_has_not_drifted.py": (
        "compares origin/master with gitlab/master and therefore needs both "
        "remotes in one checkout; an Actions checkout knows only its own, and "
        "then it announces SKIPPED instead of comparing anything -- permanently "
        "skipped is not a gate. It runs in scripts/ci/local_ci_gate.sh, in the "
        "full lane, which is the landing on master and the one checkout that has "
        "both remotes. test_mirror_has_not_drifted.py runs in "
        "orchestration-guard and holds it to still biting; "
        "the_topology_agrees_with_the_mirror_gate.py holds the direction it "
        "enforces to the roles written in CLAUDE.md"
    ),
    "origin_is_the_source_in_every_checkout.py": (
        "its subject IS this machine: which repository `origin` resolves to here, "
        "and how many checkouts of this repository sit on the disk. An Actions "
        "workspace has one remote and one checkout by construction, so there the "
        "guard can only agree with itself -- the drift it looks for is not "
        "expressible there. It runs in scripts/ci/local_ci_gate.sh, in the full "
        "lane, which is the landing and the machine that has the copies; "
        "test_origin_is_the_source_in_every_checkout.py runs in the guards job on "
        "GitHub and holds it to refusing each of the wrong shapes on fixtures, "
        "and to passing over a retired copy only when the surviving checkout "
        "holds every branch of it (#291, #340)"
    ),
    "branches_have_a_merge_request.py": (
        "asks about the checked-out branch, and an Actions checkout is a "
        "detached HEAD -- the script then announces SKIPPED and returns 0, every "
        "run. Permanently skipped reports the same green as a pass. It runs in "
        "the local gate, where there is a branch to ask about; "
        "test_branches_have_a_merge_request.py runs in promise-guard and holds "
        "it to counting"
    ),
    "generate_cli_docs.sh": (
        "generates docs/cli and man pages from clap --help; it is not a gate "
        "itself, and its --check runs in CI inside the workspace test suite "
        "via pdfluent-cli tests/cli_docs.rs, so a dedicated job would run the "
        "same diff twice"
    ),

    # The release-time pair (#574): the semver gate and the new-enum check it
    # calls. Both need rustdoc JSON from the pinned nightly plus the v1.0.0
    # baseline, and both run before publish by design -- "Not in per-PR CI"
    # is written in the gate's own header. Their mechanisms are covered on
    # every push by the three fixture tests in ci.yml's guards job instead.
    "check_new_public_enums.py": (
        "release-time gate: needs the pinned nightly and the v1.0.0 "
        "baseline; runs in publish-crates.yml before any publish"
    ),
    "semver_gate.sh": (
        "release-time gate: needs the pinned nightly and the v1.0.0 "
        "baseline; runs in publish-crates.yml before any publish"
    ),
    # The per-crate pin on the enums sealed on 14-10-2026: the same check,
    # so the same needs (pinned nightly rustdoc, a v1.0.0 worktree).
    "test_new_enums_are_sealed.sh": (
        "needs the pinned nightly and the v1.0.0 baseline, like the gate it "
        "pins; runs in scripts/ci/local_ci_gate.sh in the landing lane next "
        "to semver_gate.sh"
    ),
    # -- scripts/release: the release lane. These run at a release moment --
    # from the runbook, on dispatch, or on the mirror -- never on a push, so a
    # push job for them would judge nothing (there are no release artefacts on
    # a push). The lane's TESTS run in ci.yml; each entry below names the one
    # that holds it, where there is one.
    "artifacts_cover_every_platform.py": (
        "runs in the release lane over release artefacts (build_all_platforms.sh, "
        "the node release gate, the maven channel guard); on a push there is "
        "nothing for it to judge. test_artifacts_cover_every_platform.py runs "
        "in ci.yml over fixtures and holds every check"
    ),
    "license_audit.py": (
        "runs in the release lane, called by the artefact gate over "
        "jar/wheels/nupkg/CLI archives. test_license_audit.py runs in ci.yml"
    ),
    "maven_channel_guard.sh": (
        "stands in front of `mvn deploy`, which happens at a release, not on a "
        "push; on the mirror and in the build lane. test_maven_channel_guard.py "
        "runs in ci.yml over fixture poms and jars"
    ),
    "maven_canonical_version_pair.py": (
        "imported by the maven channel guard, which is where it runs; held by "
        "test_maven_channel_guard.py in ci.yml"
    ),
    "musl_cdylib_rustflags.sh": (
        "sourced by the musl cdylib builds in the release lane and the node "
        "workflow's musl legs; held by test_musl_crt_split.py in ci.yml"
    ),
    "release_rustflags.sh": (
        "sourced by every release artefact build in the four release "
        "workflows, and by musl_cdylib_rustflags.sh for the musl cdylibs; "
        "held by test_release_rustflags.py in ci.yml"
    ),
    "selfhosted_setup.sh": (
        "runs on self-hosted release runners through "
        ".github/actions/setup-selfhosted, which only the dispatch-only "
        "release workflows call; held by "
        "test_runner_scripts_pass_shellcheck.py in ci.yml"
    ),
    "rosetta_x64.sh": (
        "runs the osx-x64 smokes under Rosetta on the self-hosted Mac, from "
        "release-natives.yml's smoke steps on the self pool only; held by "
        "test_rosetta_x64.py and test_runner_scripts_pass_shellcheck.py "
        "in ci.yml"
    ),
    "stage_java_natives.sh": (
        "stages release natives by hand or from the build lane; held by "
        "test_stage_natives_have_no_host_fallback.py in ci.yml"
    ),
    "stage_dotnet_natives.sh": (
        "stages release natives by hand or from the build lane; held by "
        "test_stage_natives_have_no_host_fallback.py in ci.yml"
    ),
    "post_publish_smoke_runner.sh": (
        "run by hand from the publish runbook after a release; its "
        "missing-script and artefact rules are held by "
        "test_smoke_runner_requires_artifact.py in ci.yml"
    ),
    "post_publish_contents_check.sh": (
        "run by hand from the publish runbook against a published release"
    ),
    "cli_build_archives.sh": (
        "builds the CLI release archives by hand in the release lane"
    ),
    "zip_cli_stage.py": (
        "cuts the Windows CLI zips in the release-binaries matrix; held by "
        "test_cli_build_archives.py in ci.yml"
    ),
    "check_cli_staged_package.sh": (
        "checks each staged CLI package in the release lane, from the "
        "binaries matrix and the CLI release gate; held by "
        "test_cli_takeover_wiring.py in ci.yml"
    ),
    "cli_cratesio_gate.py": (
        "judges the CLI crate at publish time; held by "
        "test_cli_cratesio_gate.py in ci.yml"
    ),
    "cli_docker_build.sh": (
        "builds the release Docker image by hand in the release lane; the "
        "Dockerfile it builds is held by test_cli_dockerfile.py in ci.yml"
    ),
    "render_homebrew_formula.py": (
        "renders the Homebrew formula at a release; held by "
        "test_render_homebrew_formula.py in ci.yml"
    ),
    "pdfluent_cli_release_gate.sh": (
        "the CLI release gate, run by hand in the release lane; the CLI smoke "
        "it fronts is held by test_smoke_cli.py in ci.yml"
    ),
    "pdfluent_cli_package.sh": (
        "packages the CLI release artefacts by hand in the release lane"
    ),
    "package_cabi.sh": (
        "packages the C API release artefacts by hand in the release lane"
    ),
    "package_artifacts_still_match.sh": (
        "compares release artefacts by hand in the release lane"
    ),
    "audit-all-packages.sh": (
        "audits the published packages by hand after a release"
    ),
    "audit_package_tree.py": (
        "called by the build lane per wheel and on the mirror; a push has no "
        "release tree for it to audit"
    ),
    "licence_representation.py": (
        "called by audit-all-packages.sh on a built package; a push has no "
        "package for it to judge, and its cases run through "
        "test_audit_all_packages.py in ci.yml"
    ),
    "fix_python_wheel_paths.py": (
        "called by the build lane per wheel it just built"
    ),
    "build_pdfluent_sdist.sh": (
        "builds the pdfluent sdist for the build lane and the release "
        "package job; the maturin floor is held by "
        "test_build_pdfluent_sdist.py in ci.yml"
    ),
    "collect_capi_release.sh": (
        "assembles the judged capi dir and the seven C API tarballs for "
        "the release-natives package job; held by "
        "test_collect_capi_release.py in ci.yml"
    ),
    "check_release_artifact_contents.py": (
        "checks release artefacts by hand in the release lane"
    ),
    "check_website_wasm_lag.sh": (
        "compares the website against the released wasm by hand"
    ),
    "crate_version_is_on_crates_io.py": (
        "asks the registry about a release version, by hand in the release lane"
    ),
    "wheel_platform_tag_is_publishable.py": (
        "judges wheel tags at publish time in the release lane"
    ),
    "the_structured_extraction_is_at_parity.sh": (
        "holds the structured extraction to the last master round by hand "
        "(preview since 06-10-2026; parity itself is --stable)"
    ),
    "public_snapshot_check.sh": (
        "checks the public snapshot by hand at a release"
    ),
    "release_train.py": (
        "drives the release train by hand from the runbook"
    ),
    "fetch_ort_libs.py": (
        "stages the pinned ORT libraries by hand in the release lane; "
        "held by test_fetch_ort_libs.py in ci.yml"
    ),
    "stage_models.py": (
        "fetches the pinned model weights from GCS by hand in the release "
        "lane; held by test_stage_models.py in ci.yml"
    ),
    "fetch_models_release.py": (
        "downloads the models-v1 assets from the public release in the "
        "dispatch-only release-natives jobs; held by "
        "test_release_ort_variant.py in ci.yml"
    ),
    "release_train_guard.sh": (
        "stands in front of the release train, which runs by hand"
    ),
    "promote_release.sh": (
        "promotes a release by hand from the runbook"
    ),
    "registry_dry_run.sh": (
        "dry-runs the registry steps by hand before a release"
    ),
    "prepublish_crate_audit.sh": (
        "audits the crates by hand before a publish"
    ),
    "topological_publish_order.py": (
        "orders the crates for a publish, by hand in the release lane"
    ),
    "package_with_workspace_deps.sh": (
        "packages the release-smoke crate together with its transitive "
        "workspace dependencies from the tag-triggered release-smoke lane; "
        "held by test_package_with_workspace_deps.py in ci.yml"
    ),
    "topological_cratesio_dry_run.py": (
        "dry-runs the crates.io publish order by hand"
    ),
    "crates_topo_dry_run.sh": (
        "dry-runs the crates.io publish order on the mirror"
    ),
    "publish_ordered.sh": (
        "publishes the ordered crates from a dispatch-only job"
    ),
    "publish_wasm.sh": (
        "publishes the wasm package from the release lane"
    ),
    "wasm_dry_run.sh": (
        "dry-runs the wasm publish on the mirror"
    ),
    "transform-wasm-pkg.sh": (
        "rewrites the wasm package in the dispatch-only wasm publish"
    ),
    "crate_version_from_manifest.py": (
        "prints the wasm crate's version for the dispatch-only wasm publish, "
        "matched on the manifest path; held by "
        "test_release_package_names_exist.py in ci.yml"
    ),
    "sbom-generate.sh": (
        "generates the release SBOM on the mirror"
    ),
    "ledger_add_entry.py": (
        "appends to the release ledger by hand at a release"
    ),
    "ledger_mark_yanked.py": (
        "marks a yank in the release ledger by hand"
    ),
    "ledger_verify.py": (
        "verifies the release ledger by hand"
    ),
    "scrub-wheel.py": (
        "scrubs a wheel by hand in the release lane"
    ),
    "seed_public_repo.sh": (
        "seeds the public repository by hand"
    ),
    "seed_history_filter.py": (
        "called by the public-repo seeding, by hand"
    ),
    "tree_dirt.sh": (
        "lists worktree dirt by hand around a release"
    ),
    "homebrew_cask_clean_prefix_install.sh": (
        "test-installs the Homebrew cask into a clean prefix by hand"
    ),
    "audit_dotnet_packageid.test.sh": (
        "a by-hand regression test beside audit-all-packages.sh"
    ),
    "gate_e_version_aware.test.sh": (
        "a by-hand unit test of the release-train Gate E comparison"
    ),
    "smoke_common.sh": (
        "sourced by every consumer smoke, which is where it runs; a breakage "
        "fails the smoke test files in ci.yml"
    ),
    "smoke_rust.sh": (
        "no release job wires the cargo channel yet; invoked by hand through "
        "the post-publish smoke runner. Its argument and artefact handling "
        "is held by the smoke test files in ci.yml"
    ),
    "verify_packaged_workspace.sh": (
        "called by the by-hand registry dry run and prepublish audit; its "
        "test stands beside it, by hand too"
    ),
    "test_verify_packaged_workspace.py": (
        "a by-hand test of the by-hand packaged-workspace check: it packs "
        "and builds crates, so it stands where its subject stands"
    ),
}


def _gitlab_commands(conf) -> list[str]:
    out: list[str] = []
    for _, job in (conf or {}).items():
        if not isinstance(job, dict):
            continue
        for key in ("script", "before_script", "after_script"):
            lines = job.get(key) or []
            if isinstance(lines, str):
                lines = [lines]
            out.extend(line for line in lines if isinstance(line, str))
    return out


# The events on which a workflow's verdict can still stop a merge. A workflow
# that fires on none of these -- `workflow_dispatch` only, or a rare `schedule`
# -- runs when somebody remembers to ask, on whatever ref they name, and its red
# reaches no pull request. A `push` that names only tags is a release moment,
# not a merge, and is treated the same way.
BLOCKING_EVENTS = {"pull_request", "pull_request_target", "push", "merge_group"}

# A FREQUENT SCHEDULE COUNTS, and this is where a rule was narrowed rather than
# dropped (#350).
#
# On 06-09-2026 the workspace build, the corpus gate, the PDF/A gate and four
# heavy suites moved out of a push trigger and into a two-hourly wave of
# ci-ephemeral.yml, on an instance bought per wave -- because the one
# self-hosted runner had twenty-nine runs queued on four cores. Read with
# `schedule` excluded outright, eight guards became "stands before no merge" in
# one commit, and the register would have grown by eight rows describing a lane
# that in fact runs oftener than most of what is in it.
#
# The line is twelve fires a day, the same number and the same argument as
# SCHEDULE_PER_DAG_MINIMUM in test_feature_gated_tests_run.py: against master's
# head, within one interval, without anybody typing anything. A nightly cannot
# say that -- a day of landings sit behind it -- and stays out.
#
# What is given up is real and is not hidden: the verdict arrives after the
# merge rather than before it. That is a weaker place for a guard to stand than
# a pull request, and it is where the whole repository stands since #333.
SCHEDULE_PER_DAY_MINIMUM = 12


def _schedule_fires_per_day(spec) -> int:
    """How often these crons come round in a day, at most.

    Crude in the safe direction: only minute and hour are expanded, and any
    restriction on day-of-month, month or day-of-week makes the answer zero.
    `0 */2 * * *` is twelve; `17 4 * * *` is one; `0 4 * * 1` is none of this
    rule's business.
    """
    if not isinstance(spec, list):
        return 0
    totaal = 0
    for item in spec:
        if not isinstance(item, dict) or not item.get("cron"):
            continue
        velden = str(item["cron"]).split()
        if len(velden) != 5:
            continue
        minuut, uur, dag, maand, weekdag = velden
        if (dag, maand, weekdag) != ("*", "*", "*"):
            continue

        def aantal(veld: str, bereik: int) -> int:
            if veld == "*":
                return bereik
            if veld.startswith("*/") and veld[2:].isdigit() and int(veld[2:]) > 0:
                return bereik // int(veld[2:])
            return len([x for x in veld.split(",") if x])

        totaal += aantal(minuut, 60) * aantal(uur, 24)
    return totaal

# A job whose `needs` chain passes through a job by this name never runs: the
# parked job exists to fail loudly on a dispatch nobody should be making (#276),
# and everything behind it is a queue entry pretending to be a gate.
PARKED_JOB = "parked"


def _home(scripts: list, name: str) -> str:
    """The scanned path of `name`, so the report names a file that is there."""
    for p in scripts:
        if p.name == name:
            return str(p.relative_to(REPO))
    return f"scripts/ci/{name}"


def _events(conf) -> dict:
    """The `on:` block, keyed by event name. PyYAML reads a bare `on` as the
    boolean True (YAML 1.1), so both spellings are looked up."""
    on = (conf or {}).get("on", (conf or {}).get(True))
    if isinstance(on, str):
        return {on: None}
    if isinstance(on, list):
        return {str(x): None for x in on}
    if isinstance(on, dict):
        return {str(k): v for k, v in on.items()}
    return {}


def _called_by(conf) -> set[str]:
    """The local workflow files this one calls, as bare file names.

    `uses: ./.github/workflows/security-audit.yml` on a job is a reusable-
    workflow call: the callee's jobs run as part of THIS run and its red is
    this run's red.
    """
    uit: set[str] = set()
    for job in ((conf or {}).get("jobs") or {}).values():
        if not isinstance(job, dict):
            continue
        roept = job.get("uses")
        if isinstance(roept, str) and roept.startswith("./.github/workflows/"):
            uit.add(roept.rsplit("/", 1)[-1].split("@", 1)[0])
    return uit


def _can_block(conf, *, called_from_blocking: bool = False) -> bool:
    """Can a red from this workflow stand in front of a merge at all?

    `called_from_blocking` is the reusable-workflow case, and leaving it out
    was a hole with the same shape as the mirror. Since #350 the heavy suites
    have no trigger of their own: `security-audit.yml` and the three beside it
    are `workflow_call` only, and the two-hourly wave in `ci-ephemeral.yml`
    calls them. Judged on their own events they look like a workflow nothing
    can start, so every script they run was reported as standing before no
    merge -- while in fact their red is the wave's red, on a cadence, on a
    machine that is bought for it. (#361)
    """
    if called_from_blocking:
        return True
    for event, spec in _events(conf).items():
        if event == "schedule" and _schedule_fires_per_day(spec) >= SCHEDULE_PER_DAY_MINIMUM:
            return True
        if event not in BLOCKING_EVENTS:
            continue
        if event == "push" and isinstance(spec, dict) and "tags" in spec and "branches" not in spec:
            continue  # a tag push is a release, not a merge
        return True
    return False


def _needs(job) -> list[str]:
    needs = job.get("needs") or []
    return [str(needs)] if isinstance(needs, str) else [str(n) for n in needs]


def _behind_parked(name: str, jobs: dict, seen: frozenset = frozenset()) -> bool:
    """Does this job's `needs` chain pass through the parked job?"""
    if name == PARKED_JOB:
        return True
    job = jobs.get(name)
    if not isinstance(job, dict) or name in seen:
        return False
    return any(_behind_parked(n, jobs, seen | {name}) for n in _needs(job))


def _actions_commands(conf, *, called_from_blocking: bool = False) -> tuple[list[str], list[str]]:
    """The `run:` lines of every step of every job in a workflow, split into
    (can stop a merge, cannot).

    Only `run:` counts, for the same reason as on GitLab: every step carries a
    `name:` that mentions its script, so searching the whole YAML for a filename
    approves a job that is nothing but a heading.

    The split is the finding from #1633: a guard called only from a job in a
    dispatch-only workflow, or from a job parked behind `needs: parked`, was
    counted as "has a job" though nothing it finds can reach a pull request.
    That is the mirror's shape again -- a check by that name exists, and it
    stops nothing -- so those lines are handed back separately and judged with
    the mirror-only guards rather than with the blocking ones.
    """
    blocking: list[str] = []
    parked: list[str] = []
    workflow_blocks = _can_block(conf, called_from_blocking=called_from_blocking)
    jobs = (conf or {}).get("jobs") or {}
    for name, job in jobs.items():
        if not isinstance(job, dict):
            continue
        target = blocking if workflow_blocks and not _behind_parked(name, jobs) else parked
        for step in job.get("steps") or []:
            if isinstance(step, dict) and isinstance(step.get("run"), str):
                target.append(step["run"])
    return blocking, parked


def _runs(name: str, blob: str) -> bool:
    """Does `name` appear in `blob` as a whole path, and not as a tail?

    A plain `name in blob` approves guards that run nowhere: `test_infra_health.py`
    contains `infra_health.py`, so the guard counted as covered the moment its own
    test got a job. The test ran, the guard did not, and the difference was
    invisible. The same shape would undermine every measurement here -- moving a
    guard to GitHub could drag a namesake along that does not run there.

    Hence: no word character, dot or dash on the left (except a `/` with a path
    prefix), and nothing on the right that extends the filename.
    """
    pattern = r"(?<![\w.\-])(?:[\w./\-]*/)?" + re.escape(name) + r"(?![\w.\-])"
    return re.search(pattern, blob) is not None


def _register() -> tuple[dict[str, dict], list[str]]:
    """The written reasons, or the errors explaining why there are none to read."""
    if not REGISTER.exists():
        return {}, [f"{REGISTER.relative_to(REPO)} does not exist"]
    try:
        import tomllib
    except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
        print(
            "SKIPPED (not a pass): tomllib is missing (Python < 3.11), so "
            f"{REGISTER.relative_to(REPO)} was not read and the mirror list was "
            "not checked.",
            file=sys.stderr,
        )
        return {}, ["tomllib is missing"]
    try:
        data = tomllib.loads(REGISTER.read_text())
    except Exception as e:  # TOMLDecodeError, and anything else that fails to read
        return {}, [f"{REGISTER.relative_to(REPO)} is not valid TOML: {e}"]

    out: dict[str, dict] = {}
    errors: list[str] = []
    entries = data.get("guard") or []
    if not isinstance(entries, list) or not entries:
        errors.append(f"{REGISTER.relative_to(REPO)} contains no [[guard]] entry")
        return out, errors
    for entry in entries:
        name = entry.get("script")
        if not name:
            errors.append("a [[guard]] without `script`")
            continue
        for field in ("reason", "why", "issue"):
            if not entry.get(field):
                errors.append(f"{name}: field `{field}` is missing or empty")
        if entry.get("reason") not in REASONS:
            errors.append(
                f"{name}: reason={entry.get('reason')!r} is none of {sorted(REASONS)}"
            )
        if name in out:
            errors.append(f"{name}: appears twice in the register")
        out[name] = entry
    return out, errors


def main() -> int:
    if not CI.exists():
        print(f"SKIPPED (not a pass): {CI} is missing", file=sys.stderr)
        return 0

    # Only what a job actually executes counts.
    #
    # The first version searched the whole YAML for the filename. That is too
    # lenient: every job here carries a comment naming its script, so removing a
    # job left the name in place and the check stayed green.
    import yaml  # PyYAML is in the CI image

    # Both pipelines count, and since 25-08-2026 that is no longer a detail:
    # GitHub is the main remote and GitLab the nightly mirror. Reading only
    # .gitlab-ci.yml reported guards as orphans while GitHub Actions had a job on
    # them -- and, the other way round and worse, approved a guard that runs only
    # on the mirror and therefore stops nothing.
    gitlab = _gitlab_commands(yaml.safe_load(CI.read_text(errors="replace")))
    actions: list[str] = []
    parked: list[str] = []
    workflows = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    geladen: dict[pathlib.Path, object] = {}
    for wf in workflows:
        try:
            geladen[wf] = yaml.safe_load(wf.read_text(errors="replace"))
        except yaml.YAMLError as e:
            print(f"FAIL: {wf.relative_to(REPO)} is not valid YAML: {e}", file=sys.stderr)
            return 1

    # A reusable workflow called by one that can block a merge inherits that:
    # its jobs run inside the caller's run and its red is the caller's red.
    # One level, deliberately -- nothing here nests deeper, and a transitive
    # walk would have to answer for cycles it has never had to meet.
    geroepen: set[str] = set()
    for wf, conf in geladen.items():
        if _can_block(conf):
            geroepen |= _called_by(conf)

    for wf, conf in geladen.items():
        blocking, unblockable = _actions_commands(
            conf, called_from_blocking=wf.name in geroepen
        )
        actions.extend(blocking)
        parked.extend(unblockable)

    # FLOOR: workflows >= 1 -- without workflows the scan is broken, not the
    # directory empty. Enforced on the shape and not on a number, hence the
    # marker: an empty list is the only state that is wrong here.
    if not workflows:  # FLOOR
        print(
            f"FLOOR: no workflows found in {WORKFLOWS.relative_to(REPO)}. "
            "The search is broken -- this is not a green.",
            file=sys.stderr,
        )
        return 1

    gh = "\n".join(actions)
    gh_parked = "\n".join(parked)
    gl = "\n".join(gitlab)
    ci = gh + "\n" + gh_parked + "\n" + gl

    # rglob, not iterdir. A guard in a subdirectory used to be invisible here,
    # and the loop scaffolding of #366 lands in exactly one: the improvement
    # protocol puts a capability's ratchet at
    # `scripts/ci/loop/<capability>_ratchet.py`. A register that stops at the
    # top level would have approved a ratchet that ran nowhere -- which is the
    # sentence at the top of this file, arriving through a directory rather than
    # through a missing job.
    scripts = sorted(
        p
        for base in (MAP, RELEASE_MAP)
        for p in base.rglob("*")
        if p.is_file() and p.suffix in {".py", ".sh"} and not p.name.startswith("_")
    )
    if len(scripts) < MIN_SCRIPTS:
        print(
            f"FLOOR: {len(scripts)} scripts found in scripts/ci + scripts/release, "
            f"expected >= {MIN_SCRIPTS}. The search is broken -- this is not a green.",
            file=sys.stderr,
        )
        return 1

    problems = 0

    # A REASON THAT NAMES A PLACE IS CHECKED AGAINST THAT PLACE.
    #
    # An ALLOWED entry is prose, and prose cannot be wrong out loud. The entry
    # for mirror_has_not_drifted.py said it ran "on the machine that mirrors" and
    # nothing ran it at all -- for months, until the backup drifted 348 commits
    # and somebody went looking (#231). The reason was plausible, booked, and
    # false, which is worse than an orphan: an orphan is reported every run.
    #
    # So: an entry that claims the local gate runs it has to be true. This cannot
    # judge a reason that names a machine or a person, and does not try; it makes
    # the one claim that IS mechanical stop being free.
    claiming = [n for n, why in ALLOWED.items() if "local_ci_gate.sh" in why]
    gate_text = GATE.read_text(errors="replace") if GATE.is_file() else ""
    if claiming and not gate_text:
        problems += 1
        print(f"{GATE.relative_to(REPO)} could not be read, and {len(claiming)} "
              "exemption(s) name it as the place they run. SKIPPED (not a pass).",
              file=sys.stderr)
    elif claiming:
        untrue = [n for n in claiming if not _runs(n, gate_text)]
        if untrue:
            problems += 1
            print("\nALLOWED entries that say the local gate runs them, and it "
                  "does not:\n", file=sys.stderr)
            for n in untrue:
                print(f"  scripts/ci/{n}", file=sys.stderr)
            print("\nEither wire it into scripts/ci/local_ci_gate.sh or say where it "
                  "\nreally runs. A reason nobody checks is how a guard goes three "
                  "days\nwithout running while a register calls it placed (#231).",
                  file=sys.stderr)

    # The path is reported, not the basename. Since the scan reads
    # subdirectories (#366), "scripts/ci/pdfa_ratchet.py" would name a file that
    # is not there and send the reader looking in the wrong place. Matching
    # stays on the basename: a `run:` line names the file by whatever path it
    # uses, and _runs() allows a prefix.
    orphans = [
        p for p in scripts
        if p.name not in ALLOWED and not _runs(p.name, ci)
    ]
    if orphans:
        problems += 1
        print(
            "Guards without a pipeline job. They exist, and nothing runs them:\n",
            file=sys.stderr,
        )
        for path in orphans:
            print(f"  {path.relative_to(REPO)}", file=sys.stderr)
        print(
            "\nPut a job on it in .github/workflows/ or .gitlab-ci.yml, or add the\n"
            "script to ALLOWED with the reason it belongs in a pair of hands.\n\n"
            "The rule from CLAUDE.md: nothing is done if there is no test for it\n"
            "that runs in the CI pipeline. That holds for the checks themselves.",
            file=sys.stderr,
        )

    # A guard that runs only on the mirror stops nothing. Neither does one whose
    # only GitHub job is in a dispatch-only workflow or parked behind a job that
    # always fails: the name is in a `run:`, and no pull request ever sees the
    # result. Both shapes are judged here as one list -- "stands before no
    # merge" -- against the same register and the same ratchet, because the
    # question a reader has to answer is the same: why is this not in front of
    # the merge, and where is that written down?
    mirror_only = [
        p.name for p in scripts
        if p.name not in ALLOWED and not _runs(p.name, gh)
        and (_runs(p.name, gl) or _runs(p.name, gh_parked))
    ]
    shape = {
        n: ("only on GitLab" if _runs(n, gl) and not _runs(n, gh_parked)
            else "only from a dispatch-only or parked job" if not _runs(n, gl)
            else "on GitLab and from a dispatch-only or parked job")
        for n in mirror_only
    }
    print(
        f"[guard-jobs] {len(scripts)} script(s), {len(ALLOWED)} deliberately by "
        f"hand, {len(mirror_only)} standing before no merge "
        f"({sum(1 for v in shape.values() if v == 'only on GitLab')} only on GitLab, "
        f"{sum(1 for v in shape.values() if v != 'only on GitLab')} only from a "
        "dispatch-only or parked job)"
    )

    # The number on its own is bookkeeping. What it is worth stands per guard in
    # mirror_only_guards.toml, and that register is read in both directions: a
    # guard on the mirror without an entry is a decision that was never taken,
    # and an entry without a guard on the mirror is a reason that outlived its
    # situation and now reads as current.
    register, register_errors = _register()
    if register_errors:
        problems += 1
        print(f"\n{REGISTER.relative_to(REPO)} cannot be used:", file=sys.stderr)
        for e in register_errors:
            print(f"  - {e}", file=sys.stderr)

    undeclared = [n for n in mirror_only if n not in register]
    if undeclared:
        problems += 1
        print(
            "\nThese guards stand before no merge -- only on the mirror, or only in\n"
            "a job that fires on workflow_dispatch or waits behind `needs: parked`\n"
            "-- so a failure stops nothing, and no reason for that is written down:\n",
            file=sys.stderr,
        )
        for name in sorted(undeclared):
            print(f"  {_home(scripts, name)}  ({shape[name]})", file=sys.stderr)
        print(
            f"\nEither give it a job in .github/workflows/ that runs on pull_request\n"
            f"or push, or add it to {REGISTER.relative_to(REPO)} with reason and why.",
            file=sys.stderr,
        )

    stale = sorted(
        n for n in register
        if n not in mirror_only
    )
    if stale:
        problems += 1
        print(
            f"\nEntries in {REGISTER.relative_to(REPO)} that no longer describe "
            "anything:\n",
            file=sys.stderr,
        )
        for name in stale:
            if not ((MAP / name).exists() or (RELEASE_MAP / name).exists()):
                print(f"  {name}: the script no longer exists", file=sys.stderr)
            elif name in ALLOWED:
                print(f"  {name}: is in ALLOWED, so it is not a mirror job", file=sys.stderr)
            elif _runs(name, gh):
                print(f"  {name}: now runs in GitHub Actions before a merge -- remove the entry",
                      file=sys.stderr)
            else:
                print(f"  {name}: runs on neither pipeline", file=sys.stderr)
        print(
            "\nA reason that stays behind reads as current. Remove it in the same\n"
            "change that made it untrue.",
            file=sys.stderr,
        )

    if mirror_only:
        print(
            "[guard-jobs] standing before no merge, so stopping nothing:",
            file=sys.stderr,
        )
        for name in sorted(mirror_only):
            reason = register.get(name, {}).get("reason", "NO REASON WRITTEN DOWN")
            print(f"[guard-jobs]   {_home(scripts, name)}  ({reason}; {shape[name]})",
                  file=sys.stderr)

    if len(mirror_only) != SPIEGEL_RATEL:  # RATCHET
        problems += 1
        direction = "more" if len(mirror_only) > SPIEGEL_RATEL else "fewer"
        print(
            f"\n[guard-jobs] {len(mirror_only)} guards stand before no merge, "
            f"{direction} than the {SPIEGEL_RATEL} written down here.",
            file=sys.stderr,
        )
        if len(mirror_only) > SPIEGEL_RATEL:
            print(
                "One has been added. Put a job on it in .github/workflows/ that runs\n"
                "on pull_request or push, or raise SPIEGEL_RATEL and add the entry to\n"
                f"{REGISTER.relative_to(REPO)} saying why this guard stays out of\n"
                "the way of a merge.",
                file=sys.stderr,
            )
        else:
            print(
                "One has gone, and that is good news that ought to be pinned down:\n"
                f"set SPIEGEL_RATEL to {len(mirror_only)} and remove its entry from\n"
                f"{REGISTER.relative_to(REPO)}, or the room just won can be filled\n"
                "again tomorrow without anything noticing.",
                file=sys.stderr,
            )

    # And the other way round: an exception that no longer exists has to go.
    # Against the scan and not against `MAP / name`: since the scan reads
    # subdirectories (#366), a guard can be present and still not sit at the top
    # level, and asking the filesystem the narrower question reports it as
    # deleted.
    present = {p.name for p in scripts}
    gone = sorted(n for n in ALLOWED if n not in present)
    if gone:
        problems += 1
        print(
            f"\nThese names are in ALLOWED but no longer exist: {', '.join(gone)}.\n"
            "Remove them, or the list describes a problem that has been solved.",
            file=sys.stderr,
        )

    if problems:
        return 1

    print(
        f"[guard-jobs] every guard has a job; the {len(mirror_only)} standing before "
        "no merge each carry a written reason"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
