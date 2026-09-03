---
name: pdfluent-way
description: How work is done in the PDFluent/XFA repositories, from the first claim to the merged change. Use before starting any task larger than a one-line fix, before claiming what the codebase can do, before writing production code, before committing, pushing, or reporting anything as done. Replaces codebase-questions, git-hygiene, quality-guardrails, push-and-verify and issue-to-prevention.
---

# The PDFluent way

Six sections in the order the work happens. Most rules exist because their
absence cost real time this quarter, and then the incident is named. Rules
that are a decision rather than a measurement are marked **[agreed]** with
who decided; treat those as policy you can argue, not as evidence you can
check.

```
0 KNOW  →  1 SPEC  →  2 TEST FIRST  →  3 BRANCH  →  4 DONE  →  5 PUSH & VERIFY  →  6 PREVENT
```

## 0. Know before you claim

Answer structural questions from the authoritative source, never from a
bounded search.

- **The register is the source.** `docs/CAPABILITY_REGISTER.md` (generated,
  CI-checked) says what exists; `cargo metadata`/`cargo tree` say what is
  wired. `grep -A12`, `head`, a single glob: none of these can prove absence.
  Nine wrong answers in one week came from bounded greps.
- **A name is not a capability.** An empty feature flag, a doc that cites a
  guard, a comment that says "closed on 02-09": each was found this week
  describing something that did not exist (`notice_matches_the_registry.py`,
  `HERKOMST.md`, `KWALITEITSSPOOR.md` lived only on dead branches).
- **When a number surprises you, suspect the measurement first.** Four times
  in one day the instrument decided the answer: an empty `$G` variable that
  made a guard read stdin, a fixture one byte short, a blobless clone that
  compared against an empty file, a heredoc that escaped its own test.
- **State the method with the finding.** "Per `cargo tree`, …", not "it
  seems".
- **List your assumptions before you act**, in the message, numbered, with
  "correct me now or I proceed". Every wrong diagnosis of the WSL compaction
  (three windows lost) was an unstated assumption; the fourth attempt started
  by measuring who held the file handle and worked.

## 1. Spec before code, sized to the task

**Read the constitution first.** The project keeps its non-negotiables in
versioned files, not in chat: `CLAUDE.md` (rules and product boundaries),
`ROADMAP.md` (decisions taken, open tracks), `docs/CAPABILITY_REGISTER.md`
(what exists, generated), `PROMPTS_HERSTART.md` (current state and measured
traps). An agent that starts from the chat instead of these re-discovers
solved problems and reverts decisions; that is the memory loss the spec
layer exists to prevent. The register was itself built by reverse-specifying
a brownfield codebase; do the same for any area the register does not cover
before you plan work there.

Skip for a one-line fix. For anything else, write the spec first and keep it
short: a screen, not a document [agreed: longer specs stopped being read].

1. **Objective** — what, why, for whom, and what "done" looks like in one
   observable sentence.
2. **Validation first** — *how will we prove it works?* Name the test, the CI
   job that runs it, and the mutation that turns it red. This is written
   **before** the design, because the validation is the CI extension we are
   building; the feature is what makes it pass.
3. **Assumptions** — numbered, as in section 0.
4. **Boundaries** — three tiers, from the owner's standing decisions:
   - *always*: tests before commit, `-s` sign-off, English commits, one
     worktree per push, registers regenerated;
   - *ask the coordinator first*: new dependency, CI config, territory
     transfer, anything that costs money or downtime;
   - *never*: `PRE_PUSH_SKIP=1`, `git add -A`, `git stash`, secrets or
     internal hosts/names in commits, `Co-Authored-By`, deleting page content
     to reach conformance.
5. **Capability map** — only when one request bundles several independently
   testable capabilities (the public-repo track, the SDK release): a module
   table with stable ids, one-way dependencies, build order. Ten lines the
   coordinator reviews before any module spec is written.

Write requirements as Given/When/Then where behaviour is involved [agreed:
course practice, owner 03-09]; edge cases and failure cases are requirements,
not afterthoughts. The validation
section is the acceptance test list and nothing else passes for it.

The spec lives in the tracker issue, not in a chat message [agreed: a chat
is gone at the next restart; three sessions restarted from a stale block this
week]. The implementer
gets the spec, the constitution and the files the task touches, in one fresh
worktree: not the chat history, not the whole tree. If the implementation
fails against a clear spec, fix the code; if it fails because the spec was
ambiguous, fix the spec first and say so on the issue. A spec that keeps
needing rescue in chat is the defect.

## 2. Test first, watch it fail

**No production code without a failing test first** [agreed: owner, 03-09,
after several deliveries that turned out not to do what was reported]. If you
wrote code before the test, move it out of the tree and rebuild from the
test; code kept in view gets the test fitted to it instead of the other way
round.

RED → verify it fails **for the right reason** → GREEN with the minimal code
→ verify all green → REFACTOR staying green → next.

- **Verify RED is mandatory.** A test that never failed proves nothing. This
  week: a test whose result was discarded (`let _ =`) could only fail by
  aborting the process; a floor assertion fired before the named failure it
  was supposed to protect; a guard read only `beginbfchar` and never saw the
  `bfrange` case.
- **One behaviour, real code, clear name.** No mocks unless unavoidable
  [agreed: a mock tests the mock].
- **Both directions.** Prove the test fails when the feature is removed *and*
  that it accepts what it must accept. A stricter clip-path test that returns
  `None` unconditionally passes the first half and breaks every rectangle.
- **The mutation goes in the commit message**: what you broke, which
  assertion went red by name, restored from a copy (`cmp` identical), green
  again. Never restore with `git checkout --` after a mutation; it moves HEAD.
- **Fixtures that read the real repository inherit whatever it happens to
  be.** Baselines sampled from the success path only, `HEAD~3..HEAD` term
  picks, tests that respect `GIT_DIR` from the pre-push hook: each went stale
  or lied. Seal the environment (`sealed_env`), sample both paths.
- **A check that reads text as code also reads the explanation.** Guards that
  grep raw YAML counted comments and job names as coverage; parse the
  structure and read only what runs (`run:`, `script:`).

## 3. Branch, territory, commit

- **Check where you are before you write**: `git status --short` (deal with
  dirt first), `git checkout <branch>`, `git branch --show-current`, then
  write. A failed checkout that goes unnoticed put a doc on a CI branch.
- **The branch prefix names the territory** (`.claude/territories.toml`).
  Fixing another terminal's PR: work on a local branch with the *owner's*
  prefix and push by refspec to the PR branch; the guard reads the local name
  and only after a twelve-minute build. Claims move, they do not duplicate;
  temporary moves carry the reason and the return condition.
- **One worktree per push until the remote sha matches.** On 03-09 a
  background push ran the gate over a worktree whose branch had been switched
  for a review in the meantime; the gate judged a different tree than the
  commit being pushed, and the push was reported as failed for a reason that
  did not exist.
- **Commits**: English, conventional prefix, `-s` sign-off (owner decision
  03-09), no `Co-Authored-By`, no internal hosts, machine names, partners,
  customers or prices. `git add <explicit paths>`, never `-A`. Verify what the
  commit *contains*, not that the command exited.
- **Never `git stash`**: the shared checkout carries 79 stashes from other
  sessions, and on 18-08 a stash to unblock a checkout picked up someone
  else's conflicted `pom.xml` and carried it into a commit. **Never `git
  reset --hard`** without a recoverable ref [agreed: the mutation-restore
  incident of 25-08 lost work the same way].
- **Generated files are regenerated, not merged**: `capability_register.py`,
  `test_reachability.py`, `header_sweep.py --write`. An auto-merge produced a
  TOML with duplicate keys and a crate missing a function; "no conflict" says
  nothing about correctness.

## 4. Definition of done

Not done until all five ship **in the same change**:

1. **A test that fails when the feature is removed** (section 2).
2. **A CI job that executes it** on pull requests and on the default branch.
   GitHub Actions is the pipeline; the GitLab mirror does not count. A guard
   wired only in `local_ci_gate.sh` is not in CI.
3. **Registers updated**: capability register, reachability list,
   `mirror_only_guards.toml` when a guard has no job and why, PROMISES when we
   tell customers we do it.
4. **Bindings exposure or a written reason.**
5. **Nothing fetches, writes, spends or executes without being asked.**

Rules around the five:

- **Do not soften a gate to make it pass.** A ratchet with a floor at today's
  number, or a stated exception in the register with a reason. Never
  `allow_failure`, never `|| true`, never `--threshold 0.0`.
- **Skipping is allowed; silent skipping is not.** `SKIPPED (not a pass):
  <reason>` on stderr and a non-zero exit. Four corpus gates were "green" for
  months because they compared nothing.
- **Slow is not an argument** [agreed: owner, Definition of Done]. The WASM
  smoke test takes 10 to 14 minutes on the runner; `convertToPdfa` was broken
  for three months while that test existed and nothing ran it.
- **Report outcomes faithfully**: what ran, what it printed, what was
  skipped. "Committed and pushed" without having read the push output is a
  claim, not a report.

## 5. Push and verify

- **Before pushing**: `df -h` (the gate needs 35 GB; every build leaves a
  12 GB `target/`; a merged worktree loses its `target/` in the same turn),
  runner online (`gh api repos/<r>/actions/runners`), no heavy measurement
  running on the shared machine.
- **Push through the pre-push gate, always.** `PRE_PUSH_SKIP=1` is never
  allowed, not with a table, not when the red is "pre-existing". If the red is
  not yours, fix it or wait for its owner.
- **Read the log, not the exit code you happen to see.** A redirection keeps
  `$?`; a pipe replaces it with the status of its last stage and `||` binds to
  that stage (`false | tee log; echo $?` prints 0); a background task's
  reported exit is that of its last command, often an `echo`. Three pushes
  were reported as done this way on 03-09 while the gate had refused them.
  Capture the status before any pipe, anchor on `^PUSH_EXIT=` in the log, and
  confirm the remote sha: `git rev-parse github/<branch>` is the truth.
- **Then verify the pipeline yourself**: `gh run list --commit <full sha>`
  (short shas return nothing), wait for `completed`, read every non-green
  job. `canceled` is not `failed`. Known reds are listed in
  PROMPTS_HERSTART.md; anything else is yours.
- **Merging** is a local fast-forward of a head reviewed by a non-author at
  zero open findings, after a completed PR run: rebase in a fresh worktree,
  check the patch-id over the whole branch range, push `HEAD:refs/heads/master`,
  then close the PR and the tracker issue by hand. GitHub's merge and rebase
  buttons write the personal address into the history.

## 6. A found problem is not done when it is fixed

Every problem found while doing something else gets three lines, in the
tracker or `docs/KWALITEITSSPOOR.md`:

1. **What was wrong** — the observable fact.
2. **The repair** — or the roadmap line if it widened the change.
3. **The measure** — what now fails without a human when this class recurs.

Fix it now when it is inside what you are touching; roadmap it when it is
not; never leave it as an observation in a reply. Ask the second question:
where else does this shape exist? (The "comment counted as wiring" shape was
found five times in two days once someone asked.) Beware the test that pins
the bug: a test written after the fix that passes with the bug present is
decoration.

## 7. Replan after each landing

When a change lands, spend two minutes on the layer above it before taking
the next task: does `ROADMAP.md` or `CLAUDE.md` need a line for a decision
this work forced, does the register need regenerating, did the work reveal
that a planned item is already done or no longer needed (this week: two of
five points on #262 were done, the FormCalc item in a start block was
merged twice over). Small corrections go in now; larger ones become a
roadmap line. Institutional memory lives in these files or it does not live.

## Before you report anything as done

Read this list once more and answer each line with evidence you can paste:
test name and its red run, CI job and its run id, registers regenerated,
push log line, remote sha, PR run conclusion. If one is missing, the work is
not done; say which.
