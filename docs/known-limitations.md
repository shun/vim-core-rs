# Known limitations

This page lists current limitations and intentionally incomplete behavior in
`vim-core-rs`. Read it before changing code or relying on an API. The goal is
to prevent LLMs and humans from mistaking exposed symbols for fully realized
features.

## Current implementation gaps

The following public methods exist, but the current native implementation does
not yet provide the expected dynamic data.

- `is_incsearch_active()`
  Currently returns `false` because the native bridge stub returns `0`.
- `get_incsearch_pattern()`
  Currently returns `None` because the native bridge stub returns an empty
  string.

Treat these methods as placeholders in the current implementation, not as
reliable state queries.

## Job bridge limits

The job bridge is real, but intentionally narrow.

- The crate does not spawn OS processes itself.
- `CoreHostAction::JobStart` is a host request, not process execution.
- `vim_core_vfd_write()` currently ignores bytes written from Vim and simply
  reports success.
- Job working directory propagation is not implemented in the current bridge.
  `CoreJobStartRequest.cwd` may therefore be `None` even when Vimscript-level
  job options suggest a directory.
- When a job reaches a terminal status, the associated VFDs are closed so Vim
  observes EOF. Late injected data is rejected.

## Local file-command semantics

Local buffers and VFS-backed buffers do not follow the same save path.

- For local buffers, `:write` and `:update` enqueue `CoreHostAction::Write`.
- For VFS-backed buffers, `:write` and `:update` enqueue
  `CoreHostAction::VfsRequest(CoreVfsRequest::Save)`.
- For local buffers, `:wq` and `:xit` currently enqueue `CoreHostAction::Quit`
  instead of automatically sequencing a `Write` followed by `Quit`.

That last point is easy to misread. If a host wants local save-and-quit
semantics, the host must coordinate them after observing the queued action.

## Build and feature limits

The embedded upstream Vim is compiled with explicit feature reductions.

- Native terminal support is disabled.
- Native socket server support is disabled.
- Native channel support is disabled in the vendored configure step.
- Some upstream Vim tests are intentionally skipped because they require
  disabled features such as terminal UI or channel support.

Do not infer "missing tests" from those skips. They are repository-declared
feature boundaries.

When an upstream case is marked as `temporarily_excluded`, treat that as a
test-infrastructure or encoding gap, not as a scope decision. Policy
exclusions belong in the upstream classification manifest as `out_of_scope`
or `preserve_through_adaptation`, not in the skiplist.

## Architecture limits

These are intentional design constraints, not temporary bugs.

- Only one `VimCoreSession` may exist per process.
- The crate is not a multi-session or multi-tenant engine.
- The crate does not own semantic parsing, virtual text, terminal emulation,
  plugin hosting, or generalized editor UI.
- The VFS ledger is session-local and is not persisted across sessions.
- The VFD manager is global process state and is cleared when a session drops.

## Documentation interpretation rules

Use these rules when an API looks broader than the current behavior.

- If a type or method exists, first check whether contract tests exercise it.
- If a method is present but the native bridge returns placeholder data, treat
  it as exposed-but-incomplete.
- If a behavior depends on host action, the crate is only one half of the
  feature.

## Next steps

Read `api-contracts.md` for sequencing rules. Read `public-api-reference.md`
for the callable surface, then compare it with this page before assuming an
API is production-complete.
