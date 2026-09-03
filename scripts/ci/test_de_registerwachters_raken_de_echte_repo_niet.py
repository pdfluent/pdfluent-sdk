#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""The register guards must not be able to touch the repository they run in.

They fetch with `+refs/heads/*:refs/heads/*` -- a force-update of every branch.
Aimed at their own cache that is correct; aimed at the real repository it resets
local branches to whatever the remote has, and unpushed commits are gone.

Under a git hook `GIT_DIR` and `GIT_WORK_TREE` are exported, and git then obeys
them and ignores `cwd=`. So the aim is decided by the environment, which is why
`schone_omgeving()` exists.

Both directions are checked here, because a test that cannot fail measures
nothing: the first attempt at this reproduction used a branch that did not exist
on the remote, the refspec never touched it, and the commit survived for reasons
that had nothing to do with the fix.

Exit codes:
  0  the guards cannot reach the real repository
  1  they can
  3  cannot check (announced, never silent)
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GIT = "/usr/bin/git"


import importlib.util as _ilu
_spec = _ilu.spec_from_file_location(
    "fixture_env", Path(__file__).resolve().parent / "fixture_env.py")
_fx = _ilu.module_from_spec(_spec)
_spec.loader.exec_module(_fx)


def run(*args: str, cwd: Path, env: dict[str, str] | None = None):
    """Run git for the fixtures, with `GIT_*` stripped unless asked otherwise.

    This defaulted to inheriting the environment, and that was a real hazard
    rather than a tidiness point. Wired into the local gate, this test runs from
    the pre-push hook, where `GIT_DIR` points at the repository being pushed --
    so every `init` and `commit` meant for a throwaway fixture operated on the
    real repository instead. It rewrote this branch to a fixture commit and left
    the working tree carrying a file called `f`.

    The test that exists to prove the guards cannot reach the real repository
    reached it, and by exactly the mechanism it documents. Only the deliberate
    control below gets a dirty environment, and it names its target explicitly.
    """
    return subprocess.run(
        [GIT, *args], cwd=cwd, capture_output=True, text=True, check=False,
        # `cwd=` is not decoration: it is what makes sealed_env() run the
        # sandbox check, and every call here works inside a temporary
        # directory. A fixture pointed anywhere else should fail loudly.
        env=env if env is not None else _fx.sealed_env(cwd=cwd),
    )


def schone_omgeving() -> dict[str, str]:
    """The fixture environment, from `fixture_env.sealed_env()`.

    It used to be `{k: v for k, v in os.environ.items() if not k.startswith("GIT_")}`
    inline, and #1647's lint is right that this is not enough: dropping `GIT_*`
    stops a fixture READING the real repository, and does nothing about it
    WRITING to the real config. Two incidents in one night came from that
    distinction. `sealed_env()` seals the config surface as well, and it is the
    same helper the register guards themselves now use -- one sealing, not two.

    The name is kept because callers here mean "the environment a fixture runs
    in", and one call site deliberately does NOT want it: the dirty control in
    the seal test, which is passed an environment explicitly.
    """
    return _fx.sealed_env()


def onverzegelde_omgeving() -> dict[str, str]:
    """`GIT_*` stripped and nothing else -- the environment a fixture used to get.

    Exactly one caller wants this, and it is the control: the test that proves a
    redirecting config REACHES an unsealed clone. If the control were sealed too
    it would prove nothing, and the seal test would pass by testing the seal
    against the seal. Everything else goes through `schone_omgeving()`.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def een_repo_met_ongepusht_werk(tmp: Path) -> tuple[Path, dict[str, str]]:
    """A repository whose side branch exists on the remote *and* has a commit on top.

    Both halves matter. Without the branch existing upstream the refspec has
    nothing to force onto it; without the local commit there is nothing to lose.
    """
    tmp.mkdir(parents=True, exist_ok=True)
    upstream = tmp / "upstream.git"
    work = tmp / "work"
    run("init", "-q", "--bare", str(upstream), cwd=tmp)
    run("clone", "-q", str(upstream), str(work), cwd=tmp)
    run("config", "user.email", "t@t", cwd=work)
    run("config", "user.name", "t", cwd=work)
    (work / "f").write_text("one\n")
    run("add", "f", cwd=work)
    run("commit", "-qm", "one", cwd=work)
    run("push", "-q", "origin", "HEAD:master", cwd=work)
    run("checkout", "-q", "-b", "feature", cwd=work)
    (work / "f").write_text("shared\n")
    run("add", "f", cwd=work)
    run("commit", "-qm", "shared, pushed", cwd=work)
    run("push", "-q", "origin", "feature", cwd=work)
    (work / "f").write_text("local\n")
    run("add", "f", cwd=work)
    run("commit", "-qm", "UNPUSHED WORK", cwd=work)
    # Detached, which is what `actions/checkout` leaves behind. With a branch
    # checked out git refuses the fetch and the damage never lands -- which is
    # exactly the accident that hid this.
    run("checkout", "-q", "--detach", cwd=work)
    hook_env = dict(os.environ, GIT_DIR=str(work / ".git"), GIT_WORK_TREE=str(work))
    return work, hook_env


def tip(work: Path) -> str:
    return run("log", "-1", "--format=%s", "feature", cwd=work, env=schone_omgeving()).stdout.strip()


def een_geplant_object_wordt_niet_geaccepteerd() -> str | None:
    """have_commit must judge the object, not the name at its path.

    CACHE_ROOT is chosen by XDG_CACHE_HOME, so the cache is a location an
    environment can point anywhere. Forging a commit at a register SHA needs a
    SHA-1 collision, but planting an object at that SHA's path needs nothing --
    and `git cat-file -e <sha>` (no peel) accepts the plant while
    `git cat-file -e <sha>^{commit}` refuses it with "hash mismatch". Drop the
    peel from have_commit and this check fails, which is the point of it.
    """
    import importlib.util

    with tempfile.TemporaryDirectory() as raw:
        return _geplant_object(Path(raw))


def _geplant_object(tmp: Path) -> str | None:
    import importlib.util

    real = tmp / "planted-real"
    run("init", "-q", str(real), cwd=tmp)
    run("-c", "user.email=a@b", "-c", "user.name=a",
        "commit", "-q", "--allow-empty", "-m", "genuine", cwd=real)
    genuine = run("rev-parse", "HEAD", cwd=real).stdout.strip()
    run("-c", "user.email=x@y", "-c", "user.name=x",
        "commit", "-q", "--allow-empty", "-m", "PLANTED", cwd=real)
    forged = run("rev-parse", "HEAD", cwd=real).stdout.strip()
    body = subprocess.run(
        ["/usr/bin/git", "cat-file", "commit", forged], cwd=real,
        capture_output=True, env=schone_omgeving(), check=True,
    ).stdout

    cache = tmp / "planted-cache.git"
    run("init", "-q", "--bare", str(cache), cwd=tmp)
    import zlib
    obj = b"commit %d\x00" % len(body) + body
    d = cache / "objects" / genuine[:2]
    d.mkdir(parents=True, exist_ok=True)
    (d / genuine[2:]).write_bytes(zlib.compress(obj))

    spec = importlib.util.spec_from_file_location(
        "_register_guard", ROOT / "scripts" / "ci" / "the_fork_register_is_verifiable.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)

    if mod.have_commit(cache, genuine):
        return ("have_commit accepted an object planted at a genuine SHA's path; "
                "version_at would then read the planted content, because the read "
                "path does not rehash either")
    return None


def een_volledige_cache_wordt_nog_steeds_ondervraagd() -> str | None:
    """A cache holding every register commit is still checked for identity.

    CACHE_ROOT follows XDG_CACHE_HOME, so the cache is a location the
    environment picks. If the identity check only guarded the fetch, a cache
    that already holds every commit -- including one that is really this
    repository -- would be read without ever being questioned, and the register
    would be verified against ourselves. Not damage, a false green.
    """
    import importlib.util

    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        cache = tmp / "volledig.git"
        run("init", "-q", str(cache), cwd=tmp)
        run("-c", "user.email=a@b", "-c", "user.name=a",
            "commit", "-q", "--allow-empty", "-m", "in de cache", cwd=cache)
        sha = run("rev-parse", "HEAD", cwd=cache).stdout.strip()

        spec = importlib.util.spec_from_file_location(
            "_register_guard_2",
            ROOT / "scripts" / "ci" / "the_fork_register_is_verifiable.py")
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)

        # Every commit present, and no remote naming the upstream: the identity
        # check must refuse before the completeness of the cache excuses it.
        waarom = mod.refresh_for(cache, [sha], "https://github.com/LaurenzV/hayro")
        if waarom is None:
            return ("refresh_for read a cache holding every register commit without "
                    "checking what that cache is; a cache location chosen by "
                    "XDG_CACHE_HOME is then never questioned")
    return None


def vendor_de_wacht(doel: Path) -> Path:
    """Copy the register guard into `doel/scripts/ci`, with what it imports.

    The guard seals its environment through `fixture_env.sealed_env()` and loads
    it as a sibling file, so a copy of the guard alone is not the guard: it dies
    on the import before it can refuse anything, and a fixture that dies is not
    a fixture that passed. One place vendors both, because the first time this
    was written twice one of the two was already wrong. (#292)
    """
    (doel / "scripts" / "ci").mkdir(parents=True, exist_ok=True)
    for naam in ("the_fork_register_is_verifiable.py", "fixture_env.py"):
        bron = ROOT / "scripts" / "ci" / naam
        (doel / "scripts" / "ci" / naam).write_bytes(bron.read_bytes())
    return doel / "scripts" / "ci" / "the_fork_register_is_verifiable.py"


def main() -> int:
    if not (ROOT / "scripts/ci").is_dir():
        print(f"SKIPPED (not a pass): {ROOT}/scripts/ci is missing", file=sys.stderr)
        return 3

    failures: list[str] = []
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)

        # --- the control: without the fix, the commit is destroyed ----------
        work, hook_env = een_repo_met_ongepusht_werk(tmp / "a")
        run("fetch", "--quiet", "--filter=blob:none", "origin", "+refs/heads/*:refs/heads/*",
            cwd=tmp / "a" / "upstream.git", env=hook_env)
        if tip(work) == "UNPUSHED WORK":
            failures.append(
                "the control did not reproduce the damage, so this test proves nothing: "
                "the unpushed commit survived a fetch that should have destroyed it"
            )

        # --- and with a clean environment it survives -----------------------
        work, hook_env = een_repo_met_ongepusht_werk(tmp / "b")
        # `hook_env` is handed over explicitly rather than pushed into
        # `os.environ`: mutating this process's environment leaks into every
        # later fixture call, and `schone_omgeving()` would then be scrubbing a
        # variable this test had set itself.
        run("fetch", "--quiet", "--filter=blob:none", "origin", "+refs/heads/*:refs/heads/*",
            cwd=tmp / "b" / "upstream.git", env=schone_omgeving())
        if tip(work) != "UNPUSHED WORK":
            failures.append("a clean environment did not protect the unpushed commit")

    # --- the second lock must be stricter than the environment, not the user --
    #
    # Both clone shapes are legitimate: the cache this fetches for itself is
    # bare, and a clone a developer points `HAYRO_CLONE` at is an ordinary
    # checkout whose git-dir is `<clone>/.git`. The first version of the check
    # knew only the bare shape and refused the other, failing the run before it
    # scored anything.
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "reg", ROOT / "scripts/ci/the_fork_register_is_verifiable.py"
    )
    reg = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(reg)
    HAYRO = reg.UPSTREAMS["hayro"][0]

    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        bare = tmp / "cache.git"
        nonbare = tmp / "hayro"
        run("init", "-q", "--bare", str(bare), cwd=tmp)
        run("init", "-q", str(nonbare), cwd=tmp)

        # Both need a hayro origin: the lock requires the target to be the
        # upstream, not merely somewhere that is not us.
        run("remote", "add", "origin", "https://github.com/LaurenzV/hayro.git", cwd=bare)
        run("remote", "add", "origin", "https://github.com/LaurenzV/hayro.git", cwd=nonbare)

        if reg.de_fetch_gaat_naar_de_cache(bare, HAYRO) is not None:
            failures.append("the bare cache was refused as a fetch target")
        if reg.de_fetch_gaat_naar_de_cache(nonbare, HAYRO) is not None:
            failures.append(
                "a non-bare HAYRO_CLONE checkout was refused, though the script "
                "documents it as supported"
            )
        if reg.de_fetch_gaat_naar_de_cache(ROOT, HAYRO) is None:
            failures.append(
                "the repository root was accepted as a fetch target -- the second "
                "lock is not locking"
            )

        # A plain checkout, which is what ROOT is outside a worktree. The
        # previous version of this test passed on ROOT only because it ran in a
        # worktree, whose git-dir is `<main>/.git/worktrees/<name>` and so did
        # not match the shape being accepted. In a plain clone the repository
        # was accepted outright, and this test said nothing.
        plain = tmp / "plain"
        run("init", "-q", str(plain), cwd=tmp)
        if reg.de_fetch_gaat_naar_de_cache(plain, HAYRO) is None:
            failures.append(
                "a plain checkout with no hayro origin was accepted as a fetch target"
            )

        # The case the origin check alone cannot see: a repository that *does*
        # have hayro as its origin and is also the one the script is running
        # inside. That is what these scripts vendored into a hayro fork would
        # look like, and there the origin test passes while a force-fetch would
        # rewrite the developer's own branches. Only "the target is not us"
        # refuses it.
        vendored = tmp / "hayro-fork"
        (vendored / "scripts" / "ci").mkdir(parents=True)
        run("init", "-q", str(vendored), cwd=tmp)
        run("remote", "add", "origin", "https://github.com/LaurenzV/hayro.git", cwd=vendored)
        script = vendor_de_wacht(vendored)
        spec_v = importlib.util.spec_from_file_location(
            "reg_vendored", vendored / "scripts/ci" / script.name
        )
        reg_v = importlib.util.module_from_spec(spec_v)
        spec_v.loader.exec_module(reg_v)
        if reg_v.de_fetch_gaat_naar_de_cache(vendored, HAYRO) is None:
            failures.append(
                "a hayro-origin repository that is the script's own checkout was "
                "accepted -- the origin check passes there, so only the "
                "'not this repository' clause can refuse it"
            )

        # A linked worktree handed over as HAYRO_CLONE is a legitimate checkout.
        # Its git-dir is `<main>/.git/worktrees/<name>`, which matches neither
        # `<clone>` nor `<clone>/.git`, so comparing git-dirs refused it before
        # anything was scored. Common-dirs are the same for main and linked
        # worktrees, which is why the identity question is asked that way.
        wt = tmp / "linked"
        run("worktree", "add", "-q", str(wt), cwd=nonbare)
        if reg.de_fetch_gaat_naar_de_cache(wt, HAYRO) is not None:
            failures.append(
                "a linked worktree of a hayro clone was refused as a fetch target"
            )

        # The case common-dirs exist for: the guard running inside a worktree
        # while the *main* checkout is handed to it as the target. Their
        # git-dirs differ (`<main>/.git` against
        # `<main>/.git/worktrees/<name>`), so a git-dir comparison calls them
        # different repositories and lets the main checkout through. Their
        # common-dirs are equal, which is the whole point.
        hoofd = tmp / "guarded"
        (hoofd / "scripts" / "ci").mkdir(parents=True)
        run("init", "-q", str(hoofd), cwd=tmp)
        run("remote", "add", "origin", "https://github.com/LaurenzV/hayro.git", cwd=hoofd)
        (hoofd / "f").write_text("x")
        run("add", "f", cwd=hoofd)
        run("-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "x", cwd=hoofd)
        zijtak = tmp / "guarded-wt"
        run("worktree", "add", "-q", str(zijtak), cwd=hoofd)
        # the guard lives in the worktree, and is asked about the main checkout
        (zijtak / "scripts" / "ci").mkdir(parents=True, exist_ok=True)
        script = vendor_de_wacht(zijtak)
        spec_w = importlib.util.spec_from_file_location(
            "reg_worktree", zijtak / "scripts/ci" / script.name
        )
        reg_w = importlib.util.module_from_spec(spec_w)
        spec_w.loader.exec_module(reg_w)
        if reg_w.de_fetch_gaat_naar_de_cache(hoofd, HAYRO) is None:
            failures.append(
                "running from a worktree, the main checkout was accepted as a fetch "
                "target -- git-dirs differ between the two, only common-dirs match"
            )

        # Per registered upstream, not a hardcoded name. The same loop refreshes
        # the lopdf cache, whose legitimate origin is `J-F-Liu/lopdf.git`; a
        # check for "hayro" refused it and left the next lopdf fork-point update
        # unverifiable.
        for naam, (upstream_url, _) in reg.UPSTREAMS.items():
            cache = tmp / f"cache-{naam}"
            run("init", "-q", "--bare", str(cache), cwd=tmp)
            run("remote", "add", "origin", upstream_url, cwd=cache)
            if reg.de_fetch_gaat_naar_de_cache(cache, upstream_url) is not None:
                failures.append(
                    f"the {naam} cache was refused although its origin is the one "
                    f"the register names ({upstream_url})"
                )
        # and a cache whose origin is a *different* registered upstream
        kruis = tmp / "cache-crossed"
        run("init", "-q", "--bare", str(kruis), cwd=tmp)
        run("remote", "add", "origin", reg.UPSTREAMS["hayro"][0], cwd=kruis)
        if reg.de_fetch_gaat_naar_de_cache(kruis, reg.UPSTREAMS["lopdf"][0]) is None:
            failures.append(
                "a cache with hayro as origin was accepted as the lopdf cache"
            )
        # URL spellings that name the same repository must not be refused
        for spelling in (
            "git@github.com:LaurenzV/hayro.git",
            "https://github.com/LaurenzV/hayro",
            "https://github.com/LaurenzV/hayro.git/",
        ):
            alt = tmp / f"cache-alt-{abs(hash(spelling))}"
            run("init", "-q", "--bare", str(alt), cwd=tmp)
            run("remote", "add", "origin", spelling, cwd=alt)
            if reg.de_fetch_gaat_naar_de_cache(alt, reg.UPSTREAMS["hayro"][0]) is not None:
                failures.append(f"origin spelled {spelling} was refused")

        # The ordinary fork layout: `origin` is the developer's fork and
        # `upstream` is the real project. A remote's name is a local preference,
        # so insisting on `origin` refused a checkout holding exactly the
        # history this guard needs.
        fork = tmp / "fork-layout"
        run("init", "-q", str(fork), cwd=tmp)
        run("remote", "add", "origin", "https://github.com/someone/hayro-fork.git", cwd=fork)
        run("remote", "add", "upstream", HAYRO, cwd=fork)
        if reg.de_fetch_gaat_naar_de_cache(fork, HAYRO) is not None:
            failures.append(
                "a fork layout (origin=fork, upstream=hayro) was refused, though a "
                "remote does name the registered upstream"
            )

        # A checkout with only a fork remote is still refused, deliberately, and
        # the refusal has to say what it saw rather than fail blankly.
        alleen_fork = tmp / "fork-only"
        run("init", "-q", str(alleen_fork), cwd=tmp)
        run("remote", "add", "origin", "https://github.com/someone/hayro-fork.git", cwd=alleen_fork)
        verdict = reg.de_fetch_gaat_naar_de_cache(alleen_fork, HAYRO)
        if verdict is None:
            failures.append("a checkout with no remote naming the upstream was accepted")
        elif "hayro-fork" not in verdict:
            failures.append(
                "the refusal does not name the remotes it saw, so the reader cannot "
                f"tell why: {verdict}"
            )

        # URL rewriting in the target's *own* config. The seal switches off the
        # system and global files; it cannot switch off the repository's, and an
        # `insteadOf` there rewrites the fetch URL just the same while
        # `remote.origin.url` still reads correctly. Measured: such a cache
        # fetched a decoy commit straight through the seal.
        omgeleid = tmp / "locally-redirecting.git"
        run("init", "-q", "--bare", str(omgeleid), cwd=tmp)
        run("remote", "add", "origin", HAYRO, cwd=omgeleid)
        if reg.de_fetch_gaat_naar_de_cache(omgeleid, HAYRO) is not None:
            failures.append("a clean cache was refused before any rewrite was set")
        run("config", f"url.file://{tmp}/decoy.insteadOf", HAYRO, cwd=omgeleid)
        oordeel = reg.de_fetch_gaat_naar_de_cache(omgeleid, HAYRO)
        if oordeel is None:
            failures.append(
                "a cache that rewrites URLs in its own config was accepted -- the "
                "seal does not reach local config, so this has to be refused"
            )
        elif "decoy" not in oordeel:
            failures.append(
                "the refusal does not name what the rewrite pointed at, so the "
                f"reader cannot see why: {oordeel}"
            )

        # The mirror image: a *global* rewrite that the probe would see and the
        # fetch would not. The probe must run in the same environment as the
        # fetch, or it reports on a repository the fetch never contacts.
        spiegel = tmp / "mirror-cache.git"
        run("init", "-q", "--bare", str(spiegel), cwd=tmp)
        run("remote", "add", "origin", "https://mirror.invalid/hayro.git", cwd=spiegel)
        globaal = tmp / "home-global"
        globaal.mkdir()
        (globaal / ".gitconfig").write_text(
            f'[url "{HAYRO}"]\n\tinsteadOf = https://mirror.invalid/hayro.git\n'
        )
        vorige_home = os.environ.get("HOME")
        os.environ["HOME"] = str(globaal)
        try:
            oordeel_spiegel = reg.de_fetch_gaat_naar_de_cache(spiegel, HAYRO)
        finally:
            if vorige_home is None:
                os.environ.pop("HOME", None)
            else:
                os.environ["HOME"] = vorige_home
        if oordeel_spiegel is None:
            failures.append(
                "a cache whose origin is a mirror was accepted because a global "
                "insteadOf made the probe report the canonical URL -- the fetch "
                "seals that config away and would have taken the mirror"
            )

        # And somewhere that *is* a hayro clone by name but not by origin.
        impostor = tmp / "hayro-lookalike"
        run("init", "-q", str(impostor), cwd=tmp)
        run("remote", "add", "origin", "https://example.invalid/other.git", cwd=impostor)
        if reg.de_fetch_gaat_naar_de_cache(impostor, HAYRO) is None:
            failures.append(
                "a repository named hayro but with a different origin was accepted"
            )

    # --- config cannot redirect the clone -------------------------------
    #
    # `url.<base>.insteadOf` rewrites clone and fetch URLs. Stripping `GIT_*`
    # does not touch it: that stops git reading the caller's *repository*, not
    # the caller's *config*. Unsealed, a clone of the register's own upstream
    # URL yields whatever the redirect points at, while `remote.origin.url`
    # still records the URL that was asked for -- so the origin check approves
    # substituted content and the fork points are verified against a decoy.
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        echt, lok = tmp / "real.git", tmp / "decoy.git"
        for pad, tekst in ((echt, "REAL"), (lok, "DECOY")):
            run("init", "-q", "--bare", str(pad), cwd=tmp)
            seed = tmp / f"seed-{pad.stem}"
            run("clone", "-q", str(pad), str(seed), cwd=tmp)
            (seed / "README").write_text(tekst)
            run("add", "README", cwd=seed)
            run("-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", tekst, cwd=seed)
            run("push", "-q", "origin", "HEAD:master", cwd=seed)
        # The redirect goes where a developer's config actually lives, so the
        # seal has to *ignore* it rather than be handed something else. Writing
        # it into `GIT_CONFIG_GLOBAL` and then also sealing that variable tests
        # nothing: the first version of this case did exactly that, overriding
        # the seal with the redirect and then reporting the seal broken.
        thuis = tmp / "home"
        thuis.mkdir()
        (thuis / ".gitconfig").write_text(
            f'[url "file://{lok}"]\n\tinsteadOf = file://{echt}\n'
        )

        def kloon(env: dict[str, str], naar: Path) -> str:
            subprocess.run(
                [GIT, "clone", "-q", "--bare", f"file://{echt}", str(naar)],
                cwd=tmp, capture_output=True, text=True, check=False, env=env,
            )
            got = subprocess.run(
                [GIT, "log", "-1", "--format=%s", "--all"], cwd=naar,
                capture_output=True, text=True, check=False, env=schone_omgeving(),
            )
            return got.stdout.strip()

        vuil = dict(onverzegelde_omgeving(), HOME=str(thuis))
        if kloon(vuil, tmp / "unsealed.git") != "DECOY":
            failures.append(
                "the control did not reproduce the redirect, so this proves nothing"
            )
        verzegeld = dict(reg.verzegelde_omgeving(), HOME=str(thuis))
        if kloon(verzegeld, tmp / "sealed.git") != "REAL":
            failures.append(
                "a redirecting config still reached the clone -- the seal is not sealing"
            )

        # The two above test the *helper*. They say nothing about whether the
        # clone and fetch actually use it: unsealing a call site left them
        # green, because they never run one. So the call sites are checked too.
        for naam in ("the_fork_register_is_verifiable.py",
                     "een_forkpunt_wordt_op_inhoud_gecontroleerd.py"):
            bron = (ROOT / "scripts/ci" / naam).read_text()
            for handeling in ('git("clone"', 'git("fetch"'):
                if handeling in bron.replace("git_verzegeld(", "SEALED("):
                    failures.append(
                        f"{naam} reaches the network with the unsealed helper "
                        f"({handeling}...), so a redirecting config applies to it"
                    )

    # --- a change that does not touch the register needs no clone ---------
    #
    # This guard blocks every push. Requiring an upstream clone before asking
    # whether the register changed put a network fetch, and a possible exit 3,
    # on the critical path of work that has nothing to do with fork points.
    with tempfile.TemporaryDirectory() as raw:
        leeg = Path(raw) / "cache-home"
        omgeving = dict(schone_omgeving(), XDG_CACHE_HOME=str(leeg))
        omgeving.pop("HAYRO_CLONE", None)
        uitkomst = subprocess.run(
            [sys.executable, str(ROOT / "scripts/ci/een_forkpunt_wordt_op_inhoud_gecontroleerd.py")],
            cwd=ROOT, capture_output=True, text=True, check=False, env=omgeving,
        )
        if uitkomst.returncode != 0:
            failures.append(
                "a branch that does not touch the register did not pass cleanly: "
                f"exit {uitkomst.returncode}, {uitkomst.stderr.strip()[:160]}"
            )
        # Which assertion applies depends on the branch this runs on, so it is
        # decided rather than assumed. The previous version asserted "no cache"
        # unconditionally and therefore only held while nobody edited the
        # register -- it failed on the first branch that did (#262's decision
        # entry), reporting the guard as broken when the guard was right.
        #
        # A case whose premise the environment controls has to test the premise.
        register_gewijzigd = subprocess.run(
            [GIT, "diff", "--name-only", "github/master...HEAD", "--",
             "docs/UPSTREAM_FORKS.toml"],
            cwd=ROOT, capture_output=True, text=True, check=False,
            env=schone_omgeving(),
        ).stdout.strip()

        cache_gebouwd = (leeg / "pdfluent").exists()
        if register_gewijzigd and not cache_gebouwd:
            failures.append(
                "this branch DOES change docs/UPSTREAM_FORKS.toml and no upstream "
                "cache was built, so the fork points were never checked against "
                "upstream -- the guard passed without asking the question"
            )
        if not register_gewijzigd and cache_gebouwd:
            failures.append(
                "a branch that does not touch the register still built an upstream "
                "cache -- the clone is being demanded before the question is asked"
            )

    if (waarom := een_geplant_object_wordt_niet_geaccepteerd()) is not None:
        failures.append(waarom)

    if (waarom := een_volledige_cache_wordt_nog_steeds_ondervraagd()) is not None:
        failures.append(waarom)


    if failures:
        print("[registerwachters] the guards can reach the real repository:\n", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print("[registerwachters] OK: the fetch is destructive without a clean "
          "environment and harmless with one")
    return 0


if __name__ == "__main__":
    sys.exit(main())
