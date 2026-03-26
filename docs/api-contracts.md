# API contracts

This document captures the behavioral contracts of `vim-core-rs`. Read this
page when raw signatures are not enough, which is most of the time for this
crate because the central API is a state machine, not a stateless function
catalog.

## Contract model

You can understand the crate as a composition of four contracts.

- Session contract: one embedded Vim runtime per process
- Command contract: commands mutate editor state and may enqueue host work
- VFS contract: the host owns document storage and answers explicit requests
- VFD contract: the host owns process execution and feeds bytes back into Vim

## Session contract

The session contract defines object lifetime and process-level exclusivity.

- A process may hold only one live `VimCoreSession` at a time.
- `VimCoreSession::new` acquires a global lock. Failure to acquire it produces
  `CoreSessionError::SessionAlreadyActive`.
- Dropping the session releases the global lock and clears the VFD manager.
- The public API assumes mutation happens through one owner. The type is not
  `Send` and not `Sync`.

## Snapshot contract

The snapshot contract explains what `snapshot()` means.

- `snapshot()` returns a coherent point-in-time read of the embedded runtime.
- The method overlays VFS binding metadata from `DocumentCoordinator` onto the
  raw buffer list before returning.
- `snapshot().pending_host_actions` includes the length of the Rust-side queue
  in addition to whatever the runtime already reported.
- `buffers()` and `windows()` are convenience wrappers over `snapshot()`.

## Command contract

The command contract explains how mutations report outcomes.

- `execute_normal_command` and `execute_ex_command` return one
  `CoreCommandTransaction` that contains the outcome, final snapshot, emitted
  events, and emitted host actions.
- `CoreCommandOutcome::HostActionQueued` means the host must drain pending
  actions. It does not mean the side effect already completed.
- `execute_ex_command` first parses command text into a `ParsedExIntent` for
  file-like commands. Other Ex commands go straight to native execution.

## Ex intent routing contract

The Ex command parser is part of the public behavior even though the parser
type is private.

- `:edit` becomes a `CoreVfsRequest::Resolve` against the active buffer.
- `:write` and `:update` become either `CoreHostAction::Write` for local
  buffers or `CoreVfsRequest::Save` for virtual buffers.
- `:update` on a clean VFS buffer becomes `CoreCommandOutcome::NoChange`.
- `:wq` on a VFS buffer sets `CoreDeferredClose::SaveAndClose`, issues a save,
  and only later resumes quit after save success.
- `:xit` on a dirty VFS buffer sets
  `CoreDeferredClose::SaveIfDirtyAndClose`, issues a save, and resumes quit
  only after save success.
- `:quit` on a VFS buffer with a pending save is rejected unless forced.
- `:quit!` always queues a forced `CoreHostAction::Quit`.

## Host-action queue contract

The host-action queue is the bridge between the embedded runtime and the host
application.

- The queue is FIFO.
- The queue can contain actions emitted directly by Rust policy code and
  actions drained from the native runtime.
- The host must repeatedly call `take_pending_host_action()` until it returns
  `None`.
- `Write`, `Quit`, `Redraw`, `RequestInput`, `JobStart`, `JobStop`, and
  `VfsRequest` are requests to the host. The crate does not complete them by
  itself.

## Event delivery contract

The event contract explains how embedded-mode observability works.

- Native code enqueues `CoreEvent` values directly at the source of the Vim
  side effect.
- `take_pending_event()` drains the native event queue into a Rust FIFO and
  returns one event at a time.
- `execute_normal_command()` and `execute_ex_command()` drain both the
  event queue and the host-action queue before returning the transaction.
- Message delivery does not depend on `execute('messages')`, `v:errmsg`, or a
  registered callback.
- `CoreSnapshot` is state-only. Reading a snapshot does not drain pending
  events.
- UI-like notifications such as bell, redraw, buffer creation, window
  creation, and layout changes are modeled as `CoreEvent`, not duplicated
  host actions in v2 transactions.

## VFS request contract

The VFS contract exists because the core does not own storage.

- The core emits explicit VFS requests through `CoreHostAction::VfsRequest`.
- The host must answer with `submit_vfs_response`.
- Request IDs are monotonic and unique within one session.
- `CoreRequestEntry.issued_order` is monotonic and tracks causal order across
  requests.
- The VFS ledger records every request until the session ends. It is not only
  a queue of pending work.

## VFS operation flow contract

The common VFS flows have fixed sequencing rules.

### Resolve and load

Resolve and load happen in two stages.

1. The core issues `CoreVfsRequest::Resolve`.
2. The host replies with one of:
   - `Resolved`, which transitions into an automatic `Load` request
   - `ResolvedLocalFallback`, which switches the buffer to local ownership
   - `ResolvedMissing`, which records a `NotFound` error
3. If the host replies with `Resolved`, the core issues
   `CoreVfsRequest::Load`.
4. When the host replies with `Loaded`, the core applies the buffer contents
   into Vim and updates binding metadata.

### Save

Save is revision-aware and intentionally conservative.

1. The core snapshots the active buffer revision and text.
2. The core issues `CoreVfsRequest::Save { base_revision, text, ... }`.
3. The host persists the payload and replies with one of:
   - `Saved`
   - `Failed`
   - `Cancelled`
   - `TimedOut`
4. On `Saved`, the core accepts the response only when the buffer still has
   the same `document_id` and `current_revision == base_revision`.
5. If the revision has advanced, the response becomes
   `CoreRequestStatus::Stale`, the buffer remains dirty, and the transaction
   log records `VfsLogEvent::StaleRejected`.

## VFS validation contract

The VFS coordinator validates more than request IDs.

- A response for an unknown request ID is rejected as
  `CoreResponseApplyOutcome::UnknownRequest` and logged as
  `VfsLogEvent::UnknownRequestRejected`.
- A response whose logical operation does not match the ledger entry becomes
  `CoreRequestStatus::ProtocolMismatch`, records
  `CoreVfsErrorKind::InvalidResponse`, and logs
  `VfsLogEvent::ProtocolMismatchRejected`.
- A `Saved` response with a mismatched `document_id` is treated as a protocol
  error.
- A `Saved` response without a known base revision is also treated as a
  protocol error.

## Deferred close contract

Deferred close exists to support `:wq` and `:xit` on VFS-backed buffers.

- The coordinator stores deferred close state on the active buffer binding.
- `SaveAndClose` means a quit must happen after save completion regardless of
  prior dirty state.
- `SaveIfDirtyAndClose` means a quit must happen after save completion only for
  the save path that was triggered by dirty state.
- The transaction log records `QuitDeferred`, `QuitResumed`, and `QuitDenied`
  events so the host can explain why closing did or did not happen.

## Option contract

The option system is typed and scope-aware.

- Getter methods require the expected type up front and return
  `CoreOptionError::TypeMismatch` when the runtime type differs.
- Unknown options return `CoreOptionError::UnknownOption`.
- Unsupported scope combinations return
  `CoreOptionError::ScopeNotSupported`.
- String getters and setters validate embedded NUL bytes through `CString`
  conversion before crossing the FFI boundary.

## Search and syntax contract

The search and syntax methods are read-only rendering helpers.

- Search highlight methods return plain ranges. They do not own rendering.
- `get_cursor_match_info` can signal `TimedOut` or `MaxReached` instead of a
  concrete full count.
- Syntax extraction groups consecutive columns with the same syntax ID into one
  `CoreSyntaxChunk`.
- `get_syntax_name` may return `None` when Vim does not provide a non-empty
  group name for the ID.

## VFD and job contract

The VFD contract explains what the host must do for jobs.

- `CoreHostAction::JobStart` means Vim requested a process. The host must spawn
  it and retain the requested job and VFD IDs.
- The host must feed stdout and stderr bytes back through
  `inject_vfd_data(vfd, bytes)`.
- The host must report lifecycle transitions through
  `notify_job_status(job_id, status, exit_code)`.
- Terminal job statuses close the three associated VFDs so that Vim sees EOF.
- `vim_core_job_get_status` reports an ended job exactly once as `1`, then as
  `2` on subsequent reads after the job has been reaped.

## Diagnostics contract

The crate exposes enough state for host debugging without requiring direct
inspection of internal fields.

- `vfs_request_ledger()` is the source of truth for request status.
- `vfs_transaction_log()` is the source of truth for chronological VFS events.
- `buffer_binding()` is the source of truth for current per-buffer VFS state.
- `backend_identity()` tells you whether you are running against the real Vim
  runtime or a stubbed backend.

## Testing contract

Repository tests define large parts of the intended behavior.

- `tests/public_api_contract.rs` covers stable public surface expectations.
- `tests/vfs_contract.rs` covers VFS sequencing and error handling.
- `tests/job_api_contract.rs` and `tests/job_contract.rs` cover job bridging.
- `tests/mode_transition_contract.rs`, `tests/undo_contract.rs`,
  `tests/search_highlight_contract.rs`, and related contract tests lock in
  important user-visible behavior beyond type signatures.

## Next steps

Read `public-api-reference.md` for the symbol catalog and
`internal-api-reference.md` for the helper inventory behind these contracts.
