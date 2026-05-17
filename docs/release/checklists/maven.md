# Channel checklist — Maven Central / GitLab Maven

Governed by `docs/release/PUBLISH_PROTOCOL.md`. Audit report goes to `benchmarks/runs/prepublish_audits/<artifact>-<version>.md`.

## Prepublish audit

- [ ] `git status --porcelain` empty.
- [ ] `pom.xml` (or `build.gradle`) has `groupId`, `artifactId`, `version`, `name`, `description`, `url`, `licenses`, `developers`, `scm`.
- [ ] `<licenses>` block names the licence and links to its URL.
- [ ] Version is **new** — `https://repo1.maven.org/maven2/<groupPath>/<artifactId>/<version>/` returns 404.
- [ ] GPG signing key is available and tested: `gpg --list-secret-keys` includes the release key.
- [ ] For Maven Central: Sonatype OSSRH account has rights for the namespace.

## Package build commands

```
mvn -DskipTests clean package
```

Produces `target/<artifactId>-<version>.jar`, `<artifactId>-<version>-sources.jar`, `<artifactId>-<version>-javadoc.jar`.

For sources + javadoc + signed:

```
mvn -DskipTests -Pdeploy clean verify
```

## Package content inspection

- [ ] `unzip -l target/<artifactId>-<version>.jar`.
- [ ] Verify `META-INF/MANIFEST.MF`, `META-INF/LICENSE` (or `META-INF/LICENSE.txt`), `META-INF/NOTICE` if applicable.
- [ ] Extract to `/tmp/audit-<artifact>-<version>-jar/` and run `scripts/release/audit_package_tree.py`.
- [ ] Verify sources jar contains `*.java` only, no `.class`.
- [ ] Verify javadoc jar contains generated HTML / CSS only.
- [ ] Verify jar size ≤ 50 MiB (or waiver).

## Licence verification

- [ ] `META-INF/LICENSE` present inside the jar with the correct licence text.
- [ ] For `MIT OR Apache-2.0`: both `META-INF/LICENSE-APACHE` and `META-INF/LICENSE-MIT` present.
- [ ] `META-INF/NOTICE` present if Apache-2.0-derived.
- [ ] `<licenses>` in `pom.xml` matches the file content.

## Dependency check

- [ ] `mvn dependency:tree` shows no `SNAPSHOT` deps.
- [ ] No transitive deps pulled from non-Central, non-allowlisted repositories without an explicit `<repositories>` block.
- [ ] No yanked / withdrawn versions in the closure (Maven Central is append-only; "yanking" is via deprecation, but we should still check known-broken).

## Signing

- [ ] `mvn verify -Pgpg-sign` produces `.asc` signature files alongside each artifact.
- [ ] All four artifacts (jar, sources, javadoc, pom) carry signatures.

## Dry-run command

For Maven Central via Sonatype OSSRH, the staging repository acts as a dry-run:

```
mvn -DskipTests -Pdeploy clean deploy -DaltDeploymentRepository=ossrh-staging::default::https://s01.oss.sonatype.org/service/local/staging/deploy/maven2/
```

This uploads to a staging repo; you can inspect and **close** the staging repo before **releasing** it to Central. The Nexus Staging Maven Plugin runs validation rules (signature presence, licence headers, javadoc presence, sources presence) and will fail the staging close if any are missing.

For GitLab Maven, GitLab's package registry does not have a separate staging concept; treat the artifact's local upload (`mvn -DaltDeploymentRepository=file:///tmp/maven-staging`) as the dry-run.

## Publish command

For Maven Central (after staging close passes):

- Manually **release** the staging repo via the OSSRH web UI (recommended), or
- `mvn nexus-staging:release -Pdeploy`.

For GitLab Maven:

```
mvn -Pdeploy clean deploy
```

with the GitLab registry as `<distributionManagement><repository>` in `pom.xml`.

## Post-publish verification

- [ ] Wait for Central sync (10 minutes to a few hours).
- [ ] `curl https://repo1.maven.org/maven2/<groupPath>/<artifactId>/<version>/<artifactId>-<version>.jar.sha1` returns the published sha1.
- [ ] Download the jar from Central and re-verify `META-INF/LICENSE`.
- [ ] Smoke test: `mvn dependency:get -Dartifact=<groupId>:<artifactId>:<version>` resolves.
- [ ] Record PASS in the audit report.

## Rollback / yank / remediation

- Maven Central is append-only — **no yanking**. Mistakes live forever.
- For a defect, ship a fixed version and add a `<deprecation>` notice in the next version's `pom.xml` description.
- Sonatype will accept removal requests only for security or legal issues, via support ticket.

## Failure modes specific to Maven Central

- **No staging close**. If the staging close fails, the deploy is silently dropped from sync. Always confirm the close succeeded.
- **Missing javadoc / sources jars**. Central rejects releases without them.
- **`SNAPSHOT` in deps**. Central rejects any non-release version in production deps.
- **Unsigned artifacts**. Sonatype staging rejects unsigned files; verify all four `.asc` files.
