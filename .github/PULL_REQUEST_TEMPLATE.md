<!--
  What this is for: the first outside pull request will otherwise cost two or
  three round trips on the same three questions. Delete any heading that does
  not apply -- an empty heading is worse than none.
-->

## What this changes

<!-- One or two sentences. What is different afterwards, not what you did. -->

## Why

<!-- The problem, and how it shows itself. An issue number is enough if the
     issue says it. -->

## What goes red if this breaks

<!-- Name the test. "Nothing yet" is an honest answer and tells the reviewer
     what to look at first; a change with no test that can fail is the pattern
     this repository keeps finding, not a style preference. -->

## Checklist

- [ ] Every commit is signed off (`git commit -s`) — see [CONTRIBUTING.md](../CONTRIBUTING.md)
- [ ] `cargo fmt --all --check` and `cargo clippy --workspace -- -D warnings` pass
- [ ] Tests covering the change run in CI, not only on my machine
