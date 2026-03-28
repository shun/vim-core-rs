# Upstream test classification

This page summarizes the intended scope classification for vendored
upstream Vim `src/testdir/test_*.vim` cases. It follows
`docs/adr/0002-define-compatibility-boundaries.md` and points to the
machine-readable manifest in `upstream-test-classification.json`.

## Summary

- Total cases: `311`
- Preserve directly: `223`
- Preserve through adaptation: `42`
- Out of scope: `38`
- Temporarily excluded: `8`

## How to use this manifest

Use the JSON manifest when you need to review test coverage, discuss
whether a case belongs in the generated upstream runner, or map a Vim
feature area to repository contract tests. `build_test_runner.rs` now
consumes this manifest and emits a generated manifest that records both
the selected cases and the exclusion reasons for everything else.

Generated upstream tests now cover only `preserve_directly` cases.
Repository contract tests are the source of truth for
`preserve_through_adaptation` behavior. `out_of_scope` and
`temporarily_excluded` cases stay out of the generated runner.

## Classification rules

- **Preserve directly**: Modal editing semantics and runtime behavior that
  `vim-core-rs` preserves as embedded Vim compatibility.
- **Preserve through adaptation**: Behavior that crosses the host boundary
  and is validated mainly through repository contract tests.
- **Out of scope**: Behavior outside the embedded-core product boundary,
  such as GUI, terminal-emulator parity, plugin hosting, clientserver, and
  external language interpreter integrations.
- **Temporarily excluded**: Cases that are conceptually adjacent to the
  scope but are currently skipped for explicit repository reasons, such as
  encoding limits.

## Operating rules

- Keep vendored upstream coverage in the generated runner only for
  `preserve_directly` cases.
- Move host-boundary behavior into repository contract tests, even when
  upstream Vim has script coverage for similar scenarios.
- Record policy exclusions in the classification manifest, not in the
  skiplist.
- Reserve the skiplist for `temporarily_excluded` cases that still fit the
  repository boundary but cannot run yet for explicit reasons.

## Next steps

- Refine per-case rationales when a feature area gains or loses scope.
- Add or tighten repository contract tests when an adapted area gains new
  host-facing behavior.
