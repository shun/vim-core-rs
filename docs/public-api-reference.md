# Public API reference

This document describes the full crate-public API of `vim-core-rs`. It is
written so that an LLM or host developer can understand the callable surface,
the data model, and the behavioral intent without reopening the source code.

## Public surface summary

The public surface is intentionally concentrated in one place.

- The crate root exposes `VimCoreSession` as the main stateful facade.
- The crate root also exposes plain data types that describe snapshots,
  transactions, events, host actions, VFS contracts, options, undo trees,
  and rendering data.
- The `ffi` module exposes a small FFI-facing contract for POD structs and
  VFS-related constants.
- `src/vfs.rs` contributes public types through crate-root re-exports. The
  `vfs` module itself is not public.

## Public module: `ffi`

The `ffi` module exists for narrow interop. It is not the preferred host API
for normal Rust callers, but it is part of the stable crate surface.

### Re-exported POD structs

- `vim_core_buffer_commit_t`: The FFI struct used when the Rust side applies a
  loaded or saved buffer payload back into the embedded Vim runtime.
- `vim_core_buffer_info_t`: The raw FFI buffer metadata struct used by bridge
  code and some contract tests.

### Exported constants

These constants mirror the C enum values that describe VFS operations and
buffer source kinds.

- `VIM_CORE_VFS_OPERATION_NONE`
- `VIM_CORE_VFS_OPERATION_RESOLVE`
- `VIM_CORE_VFS_OPERATION_EXISTS`
- `VIM_CORE_VFS_OPERATION_LOAD`
- `VIM_CORE_VFS_OPERATION_SAVE`
- `VIM_CORE_BUFFER_SOURCE_LOCAL`
- `VIM_CORE_BUFFER_SOURCE_VFS`

## Session type: `VimCoreSession`

`VimCoreSession` is the main public object. It owns one embedded Vim runtime,
tracks host-facing queues, and coordinates VFS and VFD bridges.

### Ownership and concurrency model

You must understand these constraints before using any method.

- The process may hold only one live `VimCoreSession` at a time.
- `VimCoreSession` is intentionally neither `Send` nor `Sync`.
- Dropping the session releases the global single-session lock and clears VFD
  state.
- The session is stateful. Method results depend on prior commands, prior host
  actions, and prior VFS responses.

### Lifecycle and snapshot methods

These methods create the session and extract high-level state.

- `new(initial_text: &str) -> Result<Self, CoreSessionError>`
  Creates a new embedded Vim session seeded with `initial_text`. It fails with
  `CoreSessionError::SessionAlreadyActive` if another session is still alive.
- `new_with_options(initial_text: &str, options: CoreSessionOptions)
  -> Result<Self, CoreSessionError>`
  Creates a session with explicit runtime and debug-log options. The current
  implementation supports `CoreRuntimeMode::Embedded` and rejects
  `Standalone`.
- `snapshot(&self) -> CoreSnapshot`
  Returns a coherent state capture. It includes text, revision, dirty state,
  mode, pending input, cursor position, pending host-action count, buffer
  metadata, window metadata, and pop-up menu state.
- `mode(&self) -> CoreMode`
  Returns the current mode. It is equivalent to `snapshot().mode` but cheaper
  to consume conceptually.
- `runtime_mode(&self) -> CoreRuntimeMode`
  Returns the active runtime-mode contract for the session.
- `pending_input(&self) -> CorePendingInput`
  Returns whether Vim is waiting for another keystroke category, such as a
  register name or a mark target.

### Navigation and cursor-adjacent methods

These methods let you inspect or update navigation state.

- `mark(&self, mark_name: char) -> Option<CoreMarkPosition>`
  Returns the mark location if the mark is set.
- `set_mark(&mut self, mark_name: char, buf_id: i32, row: usize, col: usize)
  -> Result<(), CoreCommandError>`
  Sets a mark programmatically.
- `jumplist(&self) -> CoreJumpList`
  Returns the current jumplist and the current index within it.
- `switch_to_buffer(&mut self, buf_id: i32) -> Result<(), CoreCommandError>`
  Changes the active buffer.
- `switch_to_window(&mut self, win_id: i32) -> Result<(), CoreCommandError>`
  Changes the active window.
- `buffer_text(&self, buf_id: i32) -> Option<String>`
  Returns the full text content of one buffer.

### Command execution methods

These methods mutate editor state or inspect Vimscript-level behavior.

- `apply_normal_command(&mut self, command: &str)
  -> Result<CoreCommandOutcome, CoreCommandError>`
  Executes a Normal-mode key sequence through the bridge layer and preserves
  legacy queue-based integration.
- `apply_ex_command(&mut self, command: &str)
  -> Result<CoreCommandOutcome, CoreCommandError>`
  Executes an Ex command. The crate intercepts key path-like commands such as
  `:edit`, `:write`, `:update`, `:wq`, `:xit`, and `:quit` so it can route
  file operations through VFS or host actions instead of blindly delegating to
  native Vim file I/O.
- `execute_normal_command_v2(&mut self, command: &str)
  -> Result<CoreCommandTransaction, CoreCommandError>`
  Executes a Normal-mode key sequence and returns the transaction result that
  includes the final snapshot, emitted events, and emitted host actions.
- `execute_ex_command_v2(&mut self, command: &str)
  -> Result<CoreCommandTransaction, CoreCommandError>`
  Executes an Ex command with the same routing behavior as `apply_ex_command`,
  but returns the full transaction result instead of only a coarse outcome.
- `eval_string(&mut self, expr: &str) -> Option<String>`
  Evaluates a Vimscript expression and returns the result as a string if the
  bridge returns one.

### Host integration methods

These methods connect the session to the application that embeds it.

- `take_pending_host_action(&mut self) -> Option<CoreHostAction>`
  Drains newly emitted native host actions into the Rust queue, then pops one
  action in FIFO order. Call this repeatedly until it returns `None`.
- `take_pending_event(&mut self) -> Option<CoreEvent>`
  Drains newly emitted native events into the Rust queue, then pops one event
  in FIFO order. Call this repeatedly until it returns `None` when you use the
  queue-based integration path.
- `set_screen_size(&mut self, rows: i32, cols: i32)`
  Updates the runtime with the host UI dimensions.
- `submit_vfs_response(&mut self, response: CoreVfsResponse)
  -> Result<CoreCommandOutcome, CoreCommandError>`
  Applies one host-produced VFS response. A `Resolved` response automatically
  queues a `Load` request. A successful `Saved` response may resume a deferred
  quit. An unknown request ID is rejected as `CoreCommandError::InvalidInput`.

### Buffer and window inspection methods

These methods extract structural information about the editor state.

- `buffers(&self) -> Vec<CoreBufferInfo>`
  Returns the buffer list from the latest snapshot.
- `windows(&self) -> Vec<CoreWindowInfo>`
  Returns the window list from the latest snapshot.
- `buffer_binding(&self, buf_id: i32) -> Option<CoreBufferBinding>`
  Returns VFS binding metadata for one buffer after synchronizing bindings with
  the current snapshot.
- `vfs_request_ledger(&self) -> Vec<CoreRequestEntry>`
  Returns the full request ledger, including completed, failed, stale, and
  timed-out requests.
- `vfs_transaction_log(&self) -> Vec<VfsLogEntry>`
  Returns the chronological VFS transaction log used for diagnostics.

### Register and option methods

These methods expose register state and typed option accessors.

- `register(&self, regname: char) -> Option<String>`
  Returns the current contents of a register.
- `set_register(&mut self, regname: char, text: &str)`
  Replaces the register contents.
- `get_option_number(&self, name: &str, scope: CoreOptionScope)
  -> Result<i64, CoreOptionError>`
  Returns a numeric option after validating the expected type.
- `get_option_bool(&self, name: &str, scope: CoreOptionScope)
  -> Result<bool, CoreOptionError>`
  Returns a boolean option after validating the expected type.
- `get_option_string(&self, name: &str, scope: CoreOptionScope)
  -> Result<String, CoreOptionError>`
  Returns a string option after validating the expected type.
- `set_option_number(&mut self, name: &str, value: i64,
  scope: CoreOptionScope) -> Result<(), CoreOptionError>`
  Writes a numeric option.
- `set_option_bool(&mut self, name: &str, value: bool,
  scope: CoreOptionScope) -> Result<(), CoreOptionError>`
  Writes a boolean option. Internally, the crate routes this through the
  numeric setter with `0` and `1`.
- `set_option_string(&mut self, name: &str, value: &str,
  scope: CoreOptionScope) -> Result<(), CoreOptionError>`
  Writes a string option.

### Search, syntax, and rendering methods

These methods expose rendering-relevant state that the host can draw directly.

- `get_search_pattern(&self) -> Option<String>`
  Returns the current search pattern if one exists.
- `is_hlsearch_active(&self) -> bool`
  Returns whether persistent search highlighting is active.
- `get_search_direction(&self) -> CoreSearchDirection`
  Returns the current search direction.
- `get_search_highlights(&self, window_id: i32, start_row: i32,
  end_row: i32) -> Vec<CoreMatchRange>`
  Returns search matches for a row range in a specific window.
- `get_cursor_match_info(&self, window_id: i32, row: i32, col: i32,
  max_count: i32, timeout_ms: i32) -> CoreCursorMatchInfo`
  Returns cursor-relative match count metadata, including timeout and
  max-count saturation signals.
- `is_incsearch_active(&self) -> bool`
  Returns whether incremental search is active.
- `get_incsearch_pattern(&self) -> Option<String>`
  Returns the incremental search pattern if one exists.
- `get_syntax_name(&self, syn_id: i32) -> Option<String>`
  Maps a syntax ID to its human-readable syntax group name.
- `get_line_syntax(&self, win_id: i32, lnum: i64)
  -> Result<Vec<CoreSyntaxChunk>, CoreCommandError>`
  Returns syntax chunks for one line in one window.

In the current implementation, the two incremental-search getters are
exposed, but the native bridge still returns placeholder values. Read
`known-limitations.md` before assuming they reflect live Vim state.

### Undo and backend methods

These methods expose runtime metadata that is not part of normal text
execution.

- `get_undo_tree(&self, buf_id: i32)
  -> Result<CoreUndoTree, CoreCommandError>`
  Returns the undo tree for one buffer.
- `undo_jump(&mut self, buf_id: i32, seq: i32)
  -> Result<(), CoreCommandError>`
  Moves the buffer to a specific undo node sequence number.
- `backend_identity(&self) -> CoreBackendIdentity`
  Returns whether the embedded runtime is the real upstream runtime or a
  bridge stub.

### Job and VFD bridge methods

These methods support host-managed jobs and virtual file descriptors.

- `inject_vfd_data(&mut self, vfd: i32, data: &[u8])
  -> Result<(), CoreCommandError>`
  Pushes process output bytes into one VFD queue.
- `notify_job_status(&mut self, job_id: i32, status: JobStatus,
  exit_code: i32) -> Result<(), CoreCommandError>`
  Updates the status of a host-managed job. When the status is terminal, the
  crate closes the associated VFDs to signal EOF to Vim.

## Public value types

The public value types are plain data carriers. They exist so hosts can reason
about state without touching raw FFI.

### Mode and pending-input enums

These enums describe the editor input state.

- `CoreMode`: `Normal`, `Insert`, `Visual`, `VisualLine`, `VisualBlock`,
  `Replace`, `Select`, `SelectLine`, `SelectBlock`, `CommandLine`,
  `OperatorPending`
- `CorePendingInput`: `None`, `Char`, `Replace`, `MarkSet`, `MarkJump`,
  `Register`
- `CoreRuntimeMode`: `Embedded`, `Standalone`

### Transaction, event, and host-action types

These types describe the observable result surface for embedded execution.

- `CoreCommandTransaction { outcome, snapshot, events, host_actions }`
- `CoreEvent`: `Message(CoreMessageEvent)`,
  `PagerPrompt(CorePagerPromptKind)`, `Bell`,
  `Redraw { full, clear_before_draw }`, `BufferAdded { buf_id }`,
  `WindowCreated { win_id }`, `LayoutChanged`
- `CoreMessageKind`: `Normal`, `Error`
- `CorePagerPromptKind`: `More`, `HitReturn`
- `CoreHostAction`: `VfsRequest`, `Write`, `Quit`, `Redraw`,
  `RequestInput`, `Bell`, `JobStart`, `JobStop`

### Session configuration types

These types configure session startup behavior.

- `CoreSessionOptions { runtime_mode, debug_log_path }`

### Position and navigation structs

These structs describe marks and jump locations.

- `CoreMarkPosition { buf_id, row, col }`
- `CoreJumpListEntry { buf_id, row, col }`
- `CoreJumpList { current_index, entries }`

### Command and session result enums

These enums define the common result vocabulary of public methods.

- `CoreCommandOutcome`
  - `NoChange`
  - `BufferChanged { revision }`
  - `CursorChanged { row, col }`
  - `ModeChanged { mode }`
  - `HostActionQueued`
- `CoreCommandError`
  - `InvalidInput`
  - `OperationFailed { reason_code }`
  - `UnknownStatus { status, reason_code }`
- `CoreSessionError`
  - `SessionAlreadyActive`
  - `InitializationFailed { reason_code }`
  - `CommandFailed(CoreCommandError)`

### Host-action types

These types define the host-driven side effects that the core cannot execute
on its own.

- `CoreInputRequestKind`: `CommandLine`, `Confirmation`, `Secret`
- `CoreJobStartRequest { job_id, argv, cwd, vfd_in, vfd_out, vfd_err }`
- `JobStatus`: `Running`, `Finished`, `Failed`
- `CoreHostAction`
  - `VfsRequest(CoreVfsRequest)`
  - `Write { path, force, issued_after_revision }`
  - `Quit { force, issued_after_revision }`
  - `Redraw { full, clear_before_draw }`
  - `RequestInput { prompt, input_kind, correlation_id }`
  - `Bell`
  - `JobStart(CoreJobStartRequest)`
  - `JobStop { job_id }`

### Buffer, window, and snapshot structs

These structs expose the current editor layout.

- `CoreBufferInfo`
  - `id`
  - `name`
  - `dirty`
  - `is_active`
  - `source_kind`
  - `document_id`
  - `pending_vfs_operation`
  - `deferred_close`
  - `last_vfs_error`
- `CoreWindowInfo`
  - `id`
  - `buf_id`
  - `row`
  - `col`
  - `width`
  - `height`
  - `topline`
  - `botline`
  - `leftcol`
  - `skipcol`
  - `is_active`
- `CoreSnapshot`
  - `text`
  - `revision`
  - `dirty`
  - `mode`
  - `pending_input`
  - `cursor_row`
  - `cursor_col`
  - `pending_host_actions`
  - `buffers`
  - `windows`
  - `pum`

### Undo, syntax, and completion structs

These structs feed editor UI and history views.

- `CoreUndoNode { seq, time, save_nr, prev_seq, next_seq, alt_next_seq,
  alt_prev_seq, is_newhead, is_curhead }`
- `CoreUndoTree { nodes, synced, seq_last, save_last, seq_cur, time_cur,
  save_cur }`
- `CoreSyntaxChunk { start_col, end_col, syn_id, name }`
- `CorePumItem { word, abbr, menu, kind, info }`
- `CorePumInfo { row, col, width, height, selected_index, items }`

### Search and message structs

These types capture message and search metadata.

- `CoreMessageKind`: `Normal`, `Error`
- `CoreMessageEvent { kind, content }`
- `CoreMatchType`: `Regular`, `IncSearch`, `CurSearch`
- `CoreMatchRange { start_row, start_col, end_row, end_col, match_type }`
- `MatchCountResult`: `Calculated(usize)`, `MaxReached(usize)`, `TimedOut`
- `CoreCursorMatchInfo { is_on_match, current_match_index, total_matches }`
- `CoreSearchDirection`: `Forward`, `Backward`

### Option types

These types define the typed option system.

- `CoreOptionScope`: `Default`, `Global`, `Local`
- `CoreOptionType`: `Bool`, `Number`, `String`
- `CoreOptionError`
  - `UnknownOption { name }`
  - `TypeMismatch { name, expected, actual }`
  - `SetFailed { name, reason }`
  - `ScopeNotSupported { name, scope }`
  - `InternalError { name, detail }`

## Public VFS types

The VFS types are public because the host must handle VFS requests and return
VFS responses.

### Buffer-source and request-state enums

These enums tell the host what kind of buffer and operation it is dealing
with.

- `CoreBufferSourceKind`: `Local`, `Virtual`
- `CoreVfsOperationKind`: `Resolve`, `Exists`, `Load`, `Save`
- `CoreDeferredClose`: `Quit`, `SaveAndClose`, `SaveIfDirtyAndClose`
- `CoreVfsErrorKind`: `ResolveFailed`, `ExistsFailed`, `LoadFailed`,
  `SaveFailed`, `NotFound`, `InvalidResponse`, `HostUnavailable`,
  `Cancelled`, `TimedOut`, `RevisionMismatch`

### VFS structs

These structs represent buffer bindings, request state, and transaction logs.

- `CoreVfsError { kind, message }`
- `CorePendingVfsOperation { request_id, kind, issued_order }`
- `CoreBufferBinding { buf_id, source_kind, locator, document_id,
  display_name, committed_revision, pending_operation, deferred_close,
  last_saved_revision, last_vfs_error }`
- `CoreRequestEntry { request_id, operation_kind, target_buf_id, document_id,
  locator, base_revision, status, issued_order }`
- `VfsLogEntry { event, operation_kind, request_id, buf_id, document_id,
  locator, base_revision, current_revision, detail }`

### VFS request and response enums

These enums form the host-facing VFS protocol.

- `CoreRequestStatus`
  - `Pending`
  - `Succeeded`
  - `Failed(CoreVfsError)`
  - `Cancelled`
  - `TimedOut`
  - `Stale { reason }`
  - `ProtocolMismatch { expected, actual }`
- `CoreVfsRequest`
  - `Resolve { request_id, target_buf_id, locator }`
  - `Exists { request_id, locator }`
  - `Load { request_id, target_buf_id, document_id }`
  - `Save { request_id, target_buf_id, document_id, target_locator, text,
    base_revision, force }`
- `CoreVfsResponse`
  - `Resolved { request_id, document_id, display_name }`
  - `ResolvedLocalFallback { request_id, locator }`
  - `ResolvedMissing { request_id, locator }`
  - `ExistsResult { request_id, exists }`
  - `Loaded { request_id, document_id, text }`
  - `Saved { request_id, document_id }`
  - `Failed { request_id, error }`
  - `Cancelled { request_id }`
  - `TimedOut { request_id }`

## Operational notes

This section captures the public behaviors that are easy to miss when you only
read signatures.

- `apply_ex_command` does not treat all Ex commands equally. File-oriented
  commands are parsed into intents so the host can remain authoritative for
  storage.
- `take_pending_host_action` and `take_pending_event` are the public queue-drain
  methods for the legacy integration path. If you do not call them, queued
  host work and queued events remain buffered.
- VFS save requests are revision-aware. A `Saved` response can be rejected as
  stale if the buffer revision has advanced.
- `snapshot().pending_host_actions` includes both already buffered Rust-side
  actions and actions that native host draining would add later in the turn.

## Next steps

Read `api-contracts.md` when you need the state machine behind these APIs. Read
`internal-api-reference.md` when you need to understand the non-public helpers
that implement this surface.
