#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the seeding script must do, and above all what it must not.

The case that carries the design is the content one. A history rewrite that
quietly changes a file is a far worse failure than the trailer it was meant to
remove -- and it would not announce itself, because the trailer really would be
gone and the script really would say so.

Since 05-09-2026 there is a second class of case here, and it is the one that
cannot be taken back. Until then the script filtered no paths at all: it mirrored
the whole history, cleaned the messages, and pushed every blob this repository has
ever held -- including the eight golden documents #215 decided may be held and not
published, and 6746 commits carrying the owner's personal address. The trailer
count said 0 and the script said "done". So the cases below assert on the
RESULT -- the repository that would be published -- and not on what the script
printed about itself.
"""
from __future__ import annotations
import os, pathlib, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(CI))
from fixture_env import sealed_env  # noqa: E402

SCRIPT = CI.parent / "release" / "seed_public_repo.sh"


def git(*a, cwd, env=None):
    """A fixture git call that must succeed.

    Raising is the point. This swallowed a non-zero exit, so a fixture that
    built nothing produced an empty source repository and every case below
    failed on `git filter-branch` saying "You must specify a ref to rewrite" --
    a message about the script under test, pointing away from the fixture that
    was actually broken. (#132)
    """
    r = subprocess.run(["git", *a], cwd=str(cwd), capture_output=True,
                       text=True, env=env if env is not None else sealed_env(cwd=cwd))
    if r.returncode != 0:
        raise RuntimeError(
            f"fixture setup failed: git {' '.join(a)} in {cwd} "
            f"exited {r.returncode}\n{r.stdout}{r.stderr}")
    return r


def init_repo(r: pathlib.Path, env=None) -> None:
    """An empty repository that can commit without borrowing an identity.

    `sealed_env` hands git an empty global config, so the fixture had no
    `user.email` and `git commit` fell back to whatever the machine could
    auto-detect. A developer's machine lends one; a GitHub runner, whose
    hostname yields no usable address, refuses -- so the three commits below
    were never made there. Green here, red there, from 05-09-2026 on. The
    identity goes in the sandbox repository rather than the environment,
    because an env-level one overrides identities other fixtures configure on
    purpose (see fixture_env.sealed_env).
    """
    git("init", "-q", "-b", "master", cwd=r, env=env)
    git("config", "user.name", "fixture", cwd=r, env=env)
    # A noreply alias and not `fixture@invalid`: the script rewrites every
    # non-alias identity to the alias the history already carries, and refuses if
    # there is none. A fixture whose only identity is a personal address would be
    # testing that refusal in every case instead of the case it means to.
    git("config", "user.email", "1+fixture@users.noreply.github.com", cwd=r, env=env)


def source_repo(tmp: pathlib.Path, env=None) -> pathlib.Path:
    """Four commits carrying every property the seeding has to deal with.

    Four and not more, deliberately: `filter-branch` forks a shell per commit per
    filter, so a commit in this fixture costs about a second in each of the six
    cases that run the script end to end. The properties are what matters, and
    they all fit -- two trailers, an internal path that is DELETED before the
    seeding runs, a personal address, and a tag.
    """
    r = tmp / "source"
    r.mkdir()
    init_repo(r, env=env)
    (r / "a.txt").write_text("first\n", encoding="utf-8")
    git("add", "-A", cwd=r, env=env)
    git("commit", "-q", "-m", "first\n\nCo-Authored-By: Someone <s@example.invalid>", cwd=r, env=env)
    (r / "b.txt").write_text("second\n", encoding="utf-8")
    git("add", "-A", cwd=r, env=env)
    git("commit", "-q", "-m", "second\n\nAssisted-by: Another <a@example.invalid>", cwd=r, env=env)
    # A path `docs/PUBLIC_TREE.toml` calls internal -- and its name carries a
    # non-ASCII byte, deliberately. `git ls-tree` C-quotes such a path unless it
    # is asked for NUL-terminated output, and the quoted form starts with a
    # double quote, so it matches no prefix in `[internal].paths` and reads as
    # publishable. Found by running the seeding over the real history: eight rows
    # across five tags, every one an internal file the comparison then accused
    # the rewrite of having changed. A fixture with only ASCII names never sees
    # it.
    (r / "test-data").mkdir(exist_ok=True)
    (r / "test-data" / "golden.txt").write_text("a document we may hold\n",
                                                encoding="utf-8")
    # And one that is still at the tip when the seeding runs, because the
    # comparison reads REF TIPS: a quoted path that has already been deleted is
    # in neither list and the defect stays invisible. On the real history the
    # eight rows were on tags, whose tips still hold the file.
    (r / "test-data" / "kept-D\u03b2\u03b3.txt").write_text("held too\n",
                                                            encoding="utf-8")
    git("add", "-A", cwd=r, env=env)
    git("commit", "-q", "-m", "third, with no trailer", cwd=r, env=env)
    # Deleted before the seeding runs, and authored by a person rather than by an
    # alias. Both halves are the state this repository is really in: the eighth
    # golden document of #215 was deleted from the tree on 31-08-2026 and is in
    # the history all the same, and 6746 commits carry the owner's address. A
    # filter built from `git ls-files` sees neither.
    git("rm", "-q", "test-data/golden.txt", cwd=r, env=env)
    e2 = dict(env if env is not None else sealed_env(cwd=r))
    e2["GIT_AUTHOR_EMAIL"] = e2["GIT_COMMITTER_EMAIL"] = "someone@example.invalid"
    subprocess.run(["git", "commit", "-q", "-m", "and it is gone from the tree"],
                   cwd=str(r), capture_output=True, text=True, env=e2, check=True)
    git("tag", "v1.0.0", cwd=r, env=env)
    # The fixture says what it built. An empty source repository makes every
    # case below fail for a reason that has nothing to do with the script.
    commits = git("rev-list", "--count", "HEAD", cwd=r, env=env).stdout.strip()
    if commits != "4":
        raise RuntimeError(f"fixture source repo holds {commits} commit(s), expected 4")
    return r


# A term that exists nowhere in this repository, written into a throwaway list.
# The real list lives outside the tree by design (`geen_interne_zaken.py`: a
# denylist that ships its own terms publishes exactly what it forbids), so a test
# that needs the partner rule has to bring its own -- and one that used a real
# term would put it back in the tree.
FIXTURE_TERM = "Zorgvuldig-Fixture-Partner"


def seed_env(cwd: pathlib.Path, terms: pathlib.Path | None = None) -> dict:
    """The sealed environment plus the private term list the script insists on.

    Without it `geen_interne_zaken` refuses -- SKIPPED (not a pass) -- and every
    case here would fail for a reason that is about the fixture. Passed through
    `extra` rather than left to the ambient environment so the cases do not
    depend on whether the developer running them happens to have the real list.
    """
    env = sealed_env(cwd=cwd)
    env["PDFLUENT_INTERNE_TERMEN"] = str(terms) if terms else str(cwd / "terms.txt")
    # And the replacement list, sealed the same way and for a sharper reason: its
    # default is a file in the operator's home. A case that fell through to it
    # would be testing whatever redactions that machine happens to have written,
    # and would pass or fail differently on the runner -- while the cases that
    # mean to exercise the list pass their own through `extra`.
    env["PDFLUENT_SEED_VERVANGINGEN"] = str(cwd / "no-replacements.txt")
    return env


def terms_file(tmp: pathlib.Path) -> pathlib.Path:
    f = tmp / "terms.txt"
    f.write_text(FIXTURE_TERM + "\n", encoding="utf-8")
    return f


def run_seed(source: pathlib.Path, *extra: str, env=None):
    return subprocess.run(["bash", str(SCRIPT), str(source), *extra],
                          capture_output=True, text=True,
                          env=env if env is not None else seed_env(source),
                          timeout=600)


def seed_into(tmp: pathlib.Path, source: pathlib.Path, terms: pathlib.Path):
    """Seed into an empty bare repository and answer with what it received.

    The cases that matter are about the RESULT, and the script deliberately
    deletes its working mirror. Pushing into an empty destination is the same
    path a real seeding takes, so the fixture reads what would actually have been
    published rather than what the script said about it.
    """
    dest = tmp / "dest.git"
    subprocess.run(["git", "init", "-q", "--bare", str(dest)],
                   capture_output=True, env=sealed_env(cwd=tmp), check=True)
    u = run_seed(source, str(dest), "--push", env=seed_env(source, terms))
    return u, dest


def paths_in(repo: pathlib.Path) -> set[str]:
    """Every path the published history reaches, at any commit."""
    r = subprocess.run(["git", "-C", str(repo), "rev-list", "--objects", "--all"],
                       capture_output=True, text=True, env=sealed_env(cwd=repo))
    return {line.split(" ", 1)[1] for line in r.stdout.splitlines() if " " in line}


def addresses_in(repo: pathlib.Path) -> set[str]:
    r = subprocess.run(["git", "-C", str(repo), "log", "--all", "--format=%ae%n%ce"],
                       capture_output=True, text=True, env=sealed_env(cwd=repo))
    return set(r.stdout.split())


def case(label: str, ok: bool, why: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
    if not ok and why:
        print(f"        {why}")
    return ok


def main() -> int:
    if not SCRIPT.is_file():
        print(f"test_seed_public_repo: SKIPPED (not a pass) -- {SCRIPT} is missing.",
              file=sys.stderr)
        return 1

    ok_all = True

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        src = source_repo(tmp)
        u = run_seed(src, env=seed_env(src, terms_file(tmp)))
        out = u.stdout + u.stderr
        ok_all &= case("a history with trailers is rewritten clean",
                       u.returncode == 0 and "trailer lines after the rewrite: 0" in out,
                       out[-300:])
        ok_all &= case("and it says how many it removed",
                       "trailer lines in the source history: 2" in out, out[-300:])
        ok_all &= case("and it says how many internal paths it dropped",
                       "internal path(s) to drop" in out, out[-300:])
        # The check that matters: messages changed, content did not.
        # The claim is no longer "no tree changed" -- a path filter changes trees
        # on purpose. It is the narrower and still sufficient one: of the files
        # that stay, every one is the same blob.
        ok_all &= case("and it verifies that no surviving file changed",
                       "every publishable path holds the blob it held" in out,
                       out[-300:])
        ok_all &= case("tags come along", "2 ref(s) ready" in out or "ref(s) ready" in out,
                       out[-300:])
        # Publishing is a decision, not a step.
        ok_all &= case("it does not push without being asked",
                       "not pushing" in out, out[-300:])

    # A source with nothing to remove must still be green: a seeding step that
    # only works on dirty history is one nobody dares run twice.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r = tmp / "clean"
        r.mkdir()
        init_repo(r)
        (r / "a.txt").write_text("only\n", encoding="utf-8")
        git("add", "-A", cwd=r)
        git("commit", "-q", "-m", "no trailer here", cwd=r)
        # An internal path all the same: with nothing to exclude the script
        # refuses, deliberately -- "the manifest excludes nothing" is the state it
        # was in for months and is never a reason to publish.
        (r / "test-data").mkdir()
        (r / "test-data" / "x.txt").write_text("held\n", encoding="utf-8")
        git("add", "-A", cwd=r)
        git("commit", "-q", "-m", "an internal one", cwd=r)
        u = run_seed(r, env=seed_env(r, terms_file(tmp)))
        out = u.stdout + u.stderr
        ok_all &= case("a clean history is green and removes nothing",
                       u.returncode == 0 and "trailer lines in the source history: 0" in out,
                       out[-300:])

    # Refusing to seed over an existing history.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        src = source_repo(tmp)
        dest = tmp / "dest.git"
        subprocess.run(["git", "init", "-q", "--bare", str(dest)],
                       capture_output=True, env=sealed_env(cwd=tmp))
        # Give the destination a branch, so it is not empty.
        subprocess.run(["git", "push", "-q", str(dest), "master"], cwd=str(src),
                       capture_output=True, env=sealed_env(cwd=src))
        u = run_seed(src, str(dest), "--push", env=seed_env(src, terms_file(tmp)))
        out = u.stdout + u.stderr
        ok_all &= case("it refuses to seed over an existing history",
                       u.returncode == 1 and "already has branches" in out, out[-300:])

    # ---------------------------------------------------------------------
    # THE CASES THE 05-09 REWRITE EXISTS FOR. Each one reads the repository that
    # would have been PUBLISHED, because that is the artefact nobody can take
    # back, and none of them can be satisfied by what the script prints.
    # ---------------------------------------------------------------------

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        src = source_repo(tmp)
        u, dest = seed_into(tmp, src, terms_file(tmp))
        out = u.stdout + u.stderr
        paden = paths_in(dest)
        ok_all &= case("the seeding completes and pushes to an empty destination",
                       u.returncode == 0 and "done" in out, out[-400:])
        # The irreversible one. `test-data/` is on docs/PUBLIC_TREE.toml's
        # internal list, and the file was DELETED before the seeding ran -- so a
        # filter built from the tip would have found nothing to do.
        ok_all &= case("no path the manifest calls internal reaches the published "
                       "history",
                       not any(p.startswith("test-data") for p in paden),
                       f"published paths: {sorted(paden)}")
        # And the content, under any name: a path filter is a name filter, and
        # #260's finding is that names are not what stays fetchable.
        blobs = subprocess.run(
            ["git", "-C", str(dest), "cat-file", "--batch-all-objects",
             "--batch-check=%(objectname) %(objecttype)"],
            capture_output=True, text=True, env=sealed_env(cwd=dest)).stdout
        inhoud = subprocess.run(
            ["git", "-C", str(dest), "grep", "-h", "a document we may hold",
             "--all-match", "--", "."],
            capture_output=True, text=True, env=sealed_env(cwd=dest)).stdout
        ok_all &= case("and the withdrawn document's content is not there either",
                       "a document we may hold" not in inhoud, inhoud[:200])
        # The files that stay, byte for byte. The rewrite that removes a path
        # must not be a rewrite that touches a neighbour.
        gebleven = subprocess.run(
            ["git", "-C", str(dest), "show", "master:a.txt"],
            capture_output=True, text=True, env=sealed_env(cwd=dest)).stdout
        ok_all &= case("a file that stays is byte for byte what it was",
                       gebleven == "first\n", repr(gebleven))
        # The identity half. One fixture commit is authored by a person.
        adressen = addresses_in(dest)
        ok_all &= case("no personal address reaches the published history",
                       "someone@example.invalid" not in adressen,
                       f"addresses: {sorted(adressen)}")
        ok_all &= case("and they were rewritten to the alias the history carried, "
                       "not to something invented",
                       adressen == {"1+fixture@users.noreply.github.com"},
                       f"addresses: {sorted(adressen)}")

    # The internal-terms scan. A partner name in a file that would be PUBLISHED
    # is a refusal; the same name in a file the manifest calls internal is not,
    # because that file does not travel. Both halves, because a scan that refuses
    # everything is as useless as one that refuses nothing.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        termen = terms_file(tmp)
        src = source_repo(tmp)
        (src / "README.md").write_text(f"written for {FIXTURE_TERM}\n",
                                       encoding="utf-8")
        git("add", "-A", cwd=src)
        git("commit", "-q", "-m", "a readme", cwd=src)
        u = run_seed(src, env=seed_env(src, termen))
        out = u.stdout + u.stderr
        # With no replacement list the answer is a refusal, and it comes BEFORE
        # the rewrite rather than after it: the blob scan that would build the
        # replacements is the scan that finds the term, and telling the operator
        # an hour of filter-branch later that the check was always going to fail
        # is telling them nothing they can act on sooner.
        ok_all &= case("an internal term in a file that would be published is a "
                       "refusal when no replacement covers it",
                       u.returncode != 0
                       and "still carry an internal term" in out
                       and "does not guess at a redaction" in out,
                       out[-400:])

    # ---------------------------------------------------------------------
    # THE REPLACEMENT LIST (#222). The path filter cannot reach a term inside a
    # file that has to go out. Each case reads the repository that would have
    # been PUBLISHED.
    # ---------------------------------------------------------------------

    def with_terms_everywhere(tmp: pathlib.Path) -> pathlib.Path:
        """A source whose term is in file content, in a message, and in history."""
        src = source_repo(tmp)
        (src / "README.md").write_text(f"written for {FIXTURE_TERM}\n", encoding="utf-8")
        git("add", "-A", cwd=src)
        git("commit", "-q", "-m", f"a readme, for {FIXTURE_TERM}", cwd=src)
        # And a later commit that takes it out of the TREE. The tip is then clean
        # and the history is not, which is the state the real repository is in:
        # every content hit measured on master on 05-09-2026 was in an old blob.
        (src / "README.md").write_text("written for a customer\n", encoding="utf-8")
        git("add", "-A", cwd=src)
        git("commit", "-q", "-m", "take the name out of the readme", cwd=src)
        return src

    def replacement_list(tmp: pathlib.Path, text: str) -> pathlib.Path:
        f = tmp / "replacements.txt"
        f.write_text(text, encoding="utf-8")
        return f

    def seed_env_with(cwd, terms, repl):
        env = seed_env(cwd, terms)
        env["PDFLUENT_SEED_VERVANGINGEN"] = str(repl)
        return env

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        termen = terms_file(tmp)
        src = with_terms_everywhere(tmp)
        repl = replacement_list(tmp, f"# reviewed\n{FIXTURE_TERM}==>a customer\n")
        dest = tmp / "dest.git"
        subprocess.run(["git", "init", "-q", "--bare", str(dest)],
                       capture_output=True, env=sealed_env(cwd=tmp), check=True)
        u = subprocess.run(["bash", str(SCRIPT), str(src), str(dest), "--push"],
                           capture_output=True, text=True,
                           env=seed_env_with(src, termen, repl), timeout=600)
        out = u.stdout + u.stderr
        ok_all &= case("with a reviewed replacement the seeding completes",
                       u.returncode == 0 and "done" in out, out[-500:])
        # The property that matters, and it cannot be read off what the script
        # printed: the term is in NO object of the published repository, at any
        # commit, under any name.
        alles = subprocess.run(
            ["git", "-C", str(dest), "grep", "-h", FIXTURE_TERM, "--all-match", "--", "."],
            capture_output=True, text=True, env=sealed_env(cwd=dest)).stdout
        rev = subprocess.run(["git", "-C", str(dest), "rev-list", "--all"],
                             capture_output=True, text=True,
                             env=sealed_env(cwd=dest)).stdout.split()
        inhoud = vervangen = ""
        for sha in rev:
            inhoud += subprocess.run(
                ["git", "-C", str(dest), "grep", "-h", FIXTURE_TERM, sha],
                capture_output=True, text=True, env=sealed_env(cwd=dest)).stdout
            vervangen += subprocess.run(
                ["git", "-C", str(dest), "grep", "-h", "a customer", sha],
                capture_output=True, text=True, env=sealed_env(cwd=dest)).stdout
        ok_all &= case("the term is in no published blob, at any commit",
                       FIXTURE_TERM not in alles and FIXTURE_TERM not in inhoud,
                       (alles + inhoud)[:300])
        berichten = subprocess.run(["git", "-C", str(dest), "log", "--all", "--format=%B"],
                                   capture_output=True, text=True,
                                   env=sealed_env(cwd=dest)).stdout
        ok_all &= case("the term is in no published commit message",
                       FIXTURE_TERM not in berichten, berichten[:300])
        # Not only "the term is gone": a filter that dropped the file would
        # satisfy that too. The replacement has to be standing there, in the old
        # commit as well as in the message.
        ok_all &= case("and the replacement is what stands in its place",
                       "a customer" in vervangen and "a customer" in berichten,
                       (vervangen + berichten)[:300])
        # A rewrite that changes a neighbour is worse than the term it removed.
        gebleven = subprocess.run(["git", "-C", str(dest), "show", "master:a.txt"],
                                  capture_output=True, text=True,
                                  env=sealed_env(cwd=dest)).stdout
        ok_all &= case("a file carrying no term is byte for byte what it was",
                       gebleven == "first\n", repr(gebleven))

    # The list may only replace what a rule calls internal. Anything else would
    # make the seeding a general rewriting facility over published content,
    # operated by whoever edits a file outside the tree.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        termen = terms_file(tmp)
        src = with_terms_everywhere(tmp)
        repl = replacement_list(tmp, "AGPL-3.0-only==>MIT\n")
        u = subprocess.run(["bash", str(SCRIPT), str(src)], capture_output=True,
                           text=True, env=seed_env_with(src, termen, repl), timeout=600)
        out = u.stdout + u.stderr
        ok_all &= case("a replacement of something no rule calls internal is refused",
                       u.returncode != 0 and "no rule in geen_interne_zaken" in out,
                       out[-400:])

    # And the other direction: a substitution that carries a term of its own
    # would move the exposure rather than end it.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        termen = terms_file(tmp)
        src = with_terms_everywhere(tmp)
        # The fixture's own term on the right-hand side, rather than something
        # in the shape of a hostname or a revenue word: any literal that matches
        # a rule would do, and every one of those except this one would put a
        # string the guards call internal into a tracked file.
        repl = replacement_list(tmp, f"{FIXTURE_TERM}==>a note about {FIXTURE_TERM}\n")
        u = subprocess.run(["bash", str(SCRIPT), str(src)], capture_output=True,
                           text=True, env=seed_env_with(src, termen, repl), timeout=600)
        out = u.stdout + u.stderr
        ok_all &= case("a replacement that carries an internal term is refused",
                       u.returncode != 0 and "also calls internal" in out, out[-400:])

    # A line that is neither a comment nor `literal==>replacement` has no
    # readable intention, and guessing at one rewrites published history.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        termen = terms_file(tmp)
        src = with_terms_everywhere(tmp)
        repl = replacement_list(tmp, f"{FIXTURE_TERM}\n")
        u = subprocess.run(["bash", str(SCRIPT), str(src)], capture_output=True,
                           text=True, env=seed_env_with(src, termen, repl), timeout=600)
        out = u.stdout + u.stderr
        ok_all &= case("a malformed replacement line is refused, before the rewrite",
                       u.returncode != 0 and "carries no `==>`" in out
                       and "trailer lines after the rewrite" not in out, out[-400:])

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        termen = terms_file(tmp)
        src = source_repo(tmp)
        (src / "test-data").mkdir(exist_ok=True)
        (src / "test-data" / "notes.md").write_text(f"written for {FIXTURE_TERM}\n",
                                                    encoding="utf-8")
        # And the one file the TREE scan exempts from itself, for the reason
        # written in it: a denylist that ships its own terms publishes them, so
        # what is left in `geen_interne_zaken.py` is ordinary trade vocabulary
        # and the terms that were secret live outside the tree. The history scan
        # has to make the same exemption or the two answer one rule differently
        # -- measured on the real history, where the seeding refused on that file
        # while `geen_interne_zaken --boom` was green on it.
        eigen = src / "scripts" / "ci"
        eigen.mkdir(parents=True, exist_ok=True)
        (eigen / "geen_interne_zaken.py").write_text(
            f"# a rule naming {FIXTURE_TERM}\n", encoding="utf-8")
        git("add", "-A", cwd=src)
        git("commit", "-q", "-m", "internal notes", cwd=src)
        u = run_seed(src, env=seed_env(src, termen))
        out = u.stdout + u.stderr
        ok_all &= case("the same term is allowed in a file the manifest calls "
                       "internal, and in the denylist itself",
                       u.returncode == 0 and "no internal path, no withdrawn object" in out,
                       out[-400:])

    # Without the private list the partner rule cannot be judged, and a seeding
    # that cannot be judged is not a seeding that passed.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        src = source_repo(tmp)
        env = sealed_env(cwd=src)
        env["PDFLUENT_INTERNE_TERMEN"] = str(tmp / "absent.txt")
        u = run_seed(src, env=env)
        out = u.stdout + u.stderr
        ok_all &= case("without the private term list it refuses instead of passing",
                       u.returncode != 0 and "SKIPPED (not a pass)" in out, out[-400:])

    # The filter is not an optional extra. If it is missing the script must stop,
    # because the alternative -- carrying on with the trailer rewrite alone -- is
    # exactly the state that made this issue's blocker.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        src = source_repo(tmp)
        nep = tmp / "release"
        nep.mkdir()
        (nep / "seed_public_repo.sh").write_text(SCRIPT.read_text(encoding="utf-8"),
                                                 encoding="utf-8")
        u = subprocess.run(["bash", str(nep / "seed_public_repo.sh"), str(src)],
                           capture_output=True, text=True,
                           env=seed_env(src, terms_file(tmp)), timeout=120)
        out = u.stdout + u.stderr
        ok_all &= case("with the history filter missing it refuses to seed at all",
                       u.returncode != 0 and "would be published" in out, out[-400:])

    # A source whose every identity is a person: there is no alias to rewrite TO,
    # and inventing one would silently detach every commit from its author.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r = tmp / "personal"
        r.mkdir()
        git("init", "-q", "-b", "master", cwd=r)
        git("config", "user.name", "fixture", cwd=r)
        git("config", "user.email", "someone@example.invalid", cwd=r)
        (r / "test-data").mkdir()
        (r / "test-data" / "x.txt").write_text("held\n", encoding="utf-8")
        (r / "a.txt").write_text("only\n", encoding="utf-8")
        git("add", "-A", cwd=r)
        git("commit", "-q", "-m", "only", cwd=r)
        u = run_seed(r, env=seed_env(r, terms_file(tmp)))
        out = u.stdout + u.stderr
        ok_all &= case("with no alias in the history it refuses rather than "
                       "inventing one",
                       u.returncode != 0 and "nothing to rewrite the personal "
                       "addresses TO" in out, out[-400:])

    # THE CASE THAT WOULD HAVE CAUGHT IT. A machine that lends git an identity
    # hides this entirely, which is why the suite was green here and red on the
    # runner. `useConfigOnly` takes that loan away, so the fixture has to carry
    # its own identity or build nothing at all.
    #
    # One key of the seal is deliberately replaced here, and only here: this
    # case is ABOUT the config git reads, so it has to name the file.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        cfg = tmp / "no-identity.gitconfig"
        cfg.write_text("[user]\n\tuseConfigOnly = true\n", encoding="utf-8")
        env = sealed_env(cwd=tmp)
        env["GIT_CONFIG_GLOBAL"] = str(cfg)
        try:
            src = source_repo(tmp, env=env)
            built = git("rev-list", "--count", "HEAD", cwd=src, env=env).stdout.strip()
            ok_all &= case("the fixture commits without borrowing the machine's identity",
                           built == "4", f"the source repo holds {built} commit(s)")
        except RuntimeError as e:
            ok_all &= case("the fixture commits without borrowing the machine's identity",
                           False, str(e)[:300])

    print("test_seed_public_repo: " + ("OK" if ok_all else "FAILED"))
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
