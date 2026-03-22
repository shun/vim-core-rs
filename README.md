# vim-core-rs

`vim-core-rs` is a Rust-facing host integration layer around one embedded
upstream Vim runtime. It is written for LLM-first consumption, with humans as
the second audience. The goal of this README is to remove ambiguity before an
agent reads source code, edits behavior, or integrates the crate into a host
application.

This crate is not a generic editor toolkit. It exists to give another
application access to Vim's modal editing engine, buffer state, selected
rendering data, host-mediated virtual document I/O, and host-managed job
bridging.

## Read this first

Read these documents in order when you need the full mental model.

1. `README.md`
   This page. It explains purpose, invariants, boundaries, and the most
   dangerous assumptions to avoid.
2. `docs/SCOPE.md`
   The formal in-scope and out-of-scope boundary for the crate.
3. `docs/known-limitations.md`
   Current implementation gaps, intentionally incomplete areas, and features
   that exist in the type surface but are not fully implemented.
4. `docs/api-index.md`
   The map to the exhaustive public and internal API references.
5. `docs/api-contracts.md`
   The behavior contracts that matter more than raw signatures.
6. `docs/public-api-reference.md`
   The full crate-public symbol reference.
7. `docs/internal-api-reference.md`
   The implementation-only helper and coordination reference.

## Repository purpose

The crate owns the embedded Vim editing core. The host application owns the
environment around it.

The crate does these jobs:

- Creates exactly one embedded Vim runtime per process.
- Executes Normal-mode and Ex-mode commands against that runtime.
- Extracts coherent snapshots of text, cursor, buffers, windows, undo state,
  search data, syntax chunks, and pop-up menu state.
- Converts file-like commands into host-visible VFS requests instead of
  letting embedded Vim perform direct file I/O.
- Bridges Vim job and channel behavior into host-managed processes through
  virtual file descriptors.

The crate does not do these jobs:

- It does not own the UI event loop or rendering pipeline.
- It does not persist files by itself.
- It does not spawn or supervise real OS processes by itself.
- It does not aim to expose the whole Vim runtime as a general-purpose
  scripting platform.

## Hard invariants

These points are not optional design preferences. They are repository-level
truths enforced by code and tests.

- Only one live `VimCoreSession` may exist per process.
- `VimCoreSession` is stateful, not `Send`, and not `Sync`.
- `take_pending_host_action()` is part of the normal control flow, not an
  optional diagnostics API.
- VFS requests are explicit and host-owned. The host must answer them with
  `submit_vfs_response()`.
- Job execution is host-owned. The host must react to `JobStart`, then feed
  bytes back with `inject_vfd_data()` and lifecycle updates with
  `notify_job_status()`.
- Contract tests are the source of truth for behavior when prose and intuition
  disagree.

## Mental model

The safest way to reason about the crate is as four coupled state machines.

- Session machine
  Owns lifetime, the embedded runtime pointer, and the single-session lock.
- Command machine
  Executes Normal or Ex input and returns one coarse outcome instead of a full
  diff.
- VFS machine
  Tracks buffer-to-document bindings, request sequencing, request ledger
  status, and deferred close behavior.
- VFD machine
  Tracks virtual file descriptors and host-managed job status.

Do not model the crate as "a library that edits files." That framing leads to
incorrect assumptions about ownership, persistence, and concurrency.

## Host responsibilities

An embedding host must implement these behaviors.

- Drain `take_pending_host_action()` until it returns `None`.
- Handle `CoreHostAction::Write` and `CoreHostAction::Quit`.
- Handle every `CoreHostAction::VfsRequest` and send one matching
  `CoreVfsResponse`.
- Spawn real jobs when `CoreHostAction::JobStart` appears.
- Feed stdout and stderr bytes back through `inject_vfd_data()`.
- Report terminal job status through `notify_job_status()`.
- Set UI size with `set_screen_size()` when screen geometry matters.
- Register `set_message_handler()` before a command if Vim messages are part
  of the desired observable behavior.

## Dangerous assumptions to avoid

An agent reading this repository must not assume any of the following.

- Do not assume file writes happen automatically inside the crate.
- Do not assume `CoreCommandOutcome::HostActionQueued` means the side effect
  already completed.
- Do not assume every Ex command reaches native Vim unchanged. A file-like
  subset is intercepted and rerouted through Rust policy.
- Do not assume VFS request IDs are reusable or unordered. They are monotonic
  session-local identities.
- Do not assume the latest `CoreVfsResponse::Saved` always applies. Save
  responses can be rejected as stale when revisions advance.
- Do not assume the embedded runtime owns process execution. It only requests
  host action.

## Source-of-truth hierarchy

When repository artifacts disagree, use this order.

1. Contract tests in `tests/`
2. Rust implementation in `src/`
3. Native bridge implementation in `native/`
4. Documentation in `docs/`
5. README-style summaries elsewhere

This hierarchy is intentional. The crate is contract-driven, and many
observable behaviors are locked by tests rather than by Rust type signatures.

## Repository layout

These paths matter most for reasoning and maintenance.

- `src/lib.rs`
  Public API facade, command routing, snapshot conversion, option accessors,
  message dispatch, and top-level VFS integration.
- `src/vfs.rs`
  VFS request ledger, transaction log, buffer bindings, and deferred close.
- `src/vfd.rs`
  Virtual file descriptor and job-bridge state.
- `native/`
  C bridge and embedded Vim runtime shims.
- `tests/`
  Contract suites that define large parts of the intended behavior.
- `build.rs` and `build_*.rs`
  Bindgen, native compilation, audit generation, and traceability artifacts.

## Build facts that affect behavior

The vendored Vim build is intentionally constrained.

- The generated upstream build runs with `--with-features=normal`.
- The build disables native terminal support.
- The build disables native socket server support.
- The build disables native channel support at configure time, even though the
  crate exposes host-bridged job behavior through its own control plane.

Because of that setup, do not assume this repository behaves like a full
desktop Vim or like Neovim.

## Next steps

Read `docs/SCOPE.md` to understand design boundaries. Read
`docs/known-limitations.md` before planning changes so you do not mistake a
current gap for a guaranteed feature.
