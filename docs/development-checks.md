# Development checks

Effective 2026-09-12. The owner has disabled paid cloud compute indefinitely.
This supersedes the change-triggered paid wave introduced on 2026-09-11.

- Ordinary pushes use the local gate and existing compiler cache. A landing
  checks changed crates and reverse dependencies; shared dependency/toolchain
  changes broaden the selection. Unknown scope retains broader checks.
- The paid workflows have no automatic triggers. They are disabled in GitHub,
  require manual dispatch and a false-by-default repository variable, and refuse
  provisioning in the tracked permission check. Re-enabling a workflow or
  setting an old environment flag is insufficient.
- GitHub's Hetzner credential, the local keychain credential and both local
  hcloud contexts were removed. The local hcloud command also blocks paid
  mutations before it invokes the provider client. These are independent layers.
- The existing free desktop runner now uses `xfa-local-checks`. Its old label
  `xfa-fast` is permanently retired: old queued workflows cannot match it. Never
  register a runner with the retired label. The public SDK mirror has Actions
  disabled repository-wide; development CI belongs to the private engine repo.
- There is no scheduled expiry or automatic re-enable. Never restore a credential
  or bypass the restriction from an old worktree or secret store. A new explicit
  owner instruction is needed to consider paid work again.
- Eleven infrastructure suites and nine additional feature configurations lost
  automatic paid execution. Their deferral is recorded in the coverage registers.
  Regression tests of the spending lock still run in ordinary push CI.
- Normal local crate checks remain available. For a deliberate full local run,
  use `LANE=full bash scripts/ci/local_ci_gate.sh --full`. A local result is not
  evidence that a paid corpus or integration suite passed. Release decisions need
  fresh evidence for the exact release commit and the suites that were required.

Existing worktrees should incorporate current master through their normal
landing procedure. Keep uncommitted work. The pre-push gate no longer picks a
place by itself (#591): `XFA_GATE_REMOTE=1` runs it on an instance, and a run on
this machine needs `GATE_LOCAL_OK=1`; with neither, the push stops and says so. A missing cloud token now reflects the owner's
spending restriction and must not be treated as broken setup to auto-repair.
