---
name: parallel-terminals
description: How to work in this repository when several terminals are running at once, without colliding. Use at the START of any session, before the first edit, and whenever you pick up new work or create a branch.
---

# Several terminals, one repository

Six territories are declared in `.claude/territories.toml`. Each terminal owns one.
`scripts/ci/territories_do_not_overlap.py` enforces it, so this is not an agreement
you can quietly drift from.

## Before you touch anything

1. **Find out which territory you are.** The user says so, or the issue carries a
   `t1-forks` … `t6-visibility` label. If neither, ask. Do not guess.
2. **Read your territory's paths** in `.claude/territories.toml`.
3. **Name your branch after it**: `t2/ci-guards-on-github`. The guard reads the prefix.
4. **Work in your own worktree**, under `.worktrees/`, off `github/master`.
   Never in the main checkout — it carries another session's branch.

## The rules that exist because they were broken

- **Never `git stash`.** This tree carries stashes that are not yours.
- **Never bare `git add -A`.** On 27-08 two files belonging to another session
  landed in someone else's commit, under their message. Add by path.
- **Use `/usr/bin/git`.** The Homebrew git at `/usr/local/bin/git` is wedged.
- **`github` is the primary remote, `origin` (GitLab) is a nightly mirror.**
  Diff and compare against `github/master`. The mirror runs behind, and comparing
  against it reports other people's commits as yours.
- **`cargo check` does not build `#[cfg(test)]`.** Use `--tests`, or a test module
  that does not compile passes your gate.

## Reaching outside your territory

Sometimes you must. Then do it visibly: change `.claude/territories.toml` in a
commit, so it comes past review. The guard permits editing that file from any
branch precisely so the claim is possible, and refuses the silent version.

Paths nobody claims are free. If you find yourself contending over one, add it to
the map rather than racing for it.

## The CI queue is the scarce resource, not your speed

One self-hosted runner, and a hard limit of one Hetzner instance at a time. Roughly
three jobs an hour for everybody. Consequences:

- **Verify locally first.** The pre-push hook runs the full local gate. Do not
  push to "see if CI likes it".
- **`.github/workflows/ci.yml` only triggers on pull requests to master and pushes
  to master.** A branch push runs nothing. Anything reported as "pushed and green"
  without a pull request has not been tested.
- **Open one pull request when the work is done**, not one per commit. Seven at
  once on 31-08 saturated the queue for hours.
- **No GitHub-hosted runners.** The owner wants zero Actions cost. Three workflows
  on `ubuntu-latest` have never once succeeded because of it. Self-hosted, or not
  at all.

## What finished means here

Not "there is a test". A test that runs in the pipeline, on every merge request,
and that **fails when the thing it protects is broken**. Prove it: break the code
deliberately, watch the test go red, restore from a **copy** of the file.

Never `git checkout --` after a mutation. It discards your own work along with
the mutation, while the harness cheerfully reports the test as caught.

A test that stays green against broken code is evidence that nothing broke, not
evidence that it catches anything. Say which one you have.

Anything that cannot run announces itself: `SKIPPED (not a pass): <reason>` on
stderr. A silent skip is indistinguishable from a pass, and that is how a
`/ToUnicode` bug survived a fully green suite.

## Reporting back

State what you verified rather than what you did. Include every mutation and its
result. If you could not finish, say precisely where you stopped — a half-finished
job honestly labelled is worth more than a complete-looking one that is not.
