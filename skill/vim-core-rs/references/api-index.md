# vim-core-rs bundled API index

This file is the standalone entry point for the bundled API documentation in
this skill. Use it when the skill is copied outside the repository and you
still need the full mental model of the crate.

## What each bundled document covers

Read the bundled pages in this order when you need complete coverage.

- `readme-summary.md`
  Documents repository purpose, invariants, host obligations, and dangerous
  assumptions.
- `architecture.md`
  Documents design boundaries and non-goals.
- `known-limitations.md`
  Documents current implementation gaps and intentionally incomplete behavior.
- `public-api-reference.md`
  Documents the crate-public surface that a host application can call.
- `internal-api-reference.md`
  Documents non-public Rust APIs, coordination layers, and internal C ABI
  shims that matter when maintaining the repository.
- `api-contracts.md`
  Documents sequencing, invariants, and host obligations that matter more than
  raw signatures.

## Fast symbol map

### Public root symbols

- Module: `ffi`
- Session type: `VimCoreSession`
- Type alias: `MessageHandler`
- Enums: `CoreMode`, `CorePendingInput`, `CoreCommandOutcome`,
  `CoreInputRequestKind`, `CoreBackendIdentity`, `CoreOptionScope`,
  `CoreOptionType`, `CoreOptionError`, `JobStatus`, `CoreHostAction`,
  `CoreMessageSeverity`, `CoreMessageCategory`, `CoreMatchType`,
  `MatchCountResult`,
  `CoreSearchDirection`, `CoreCommandError`, `CoreSessionError`
- Structs: `CoreMarkPosition`, `CoreJumpListEntry`, `CoreJumpList`,
  `CoreJobStartRequest`, `CoreBufferInfo`, `CoreWindowInfo`, `CoreUndoNode`,
  `CoreUndoTree`, `CoreSyntaxChunk`, `CoreMessageEvent`,
  `CoreCommandTransaction`, `CorePumItem`, `CorePumInfo`, `CoreMatchRange`,
  `CoreCursorMatchInfo`, `CoreSessionOptions`, `CoreSnapshot`
- Re-exported VFS items: `CoreBufferBinding`, `CoreBufferSourceKind`,
  `CoreDeferredClose`, `CorePendingVfsOperation`, `CoreRequestEntry`,
  `CoreRequestStatus`, `CoreVfsError`, `CoreVfsErrorKind`,
  `CoreVfsOperationKind`, `CoreVfsRequest`, `CoreVfsResponse`,
  `VfsLogEntry`, `VfsLogEvent`

### Public `VimCoreSession` methods

- Lifecycle and snapshots: `new`, `new_with_options`, `snapshot`, `mode`,
  `pending_input`
- Navigation and state writes: `mark`, `set_mark`, `jumplist`,
  `switch_to_buffer`, `switch_to_window`, `buffer_text`
- Command execution: `execute_normal_command`, `execute_ex_command`,
  `eval_string`
- Host integration: `take_pending_host_action`, `set_screen_size`,
  `set_message_handler`, `submit_vfs_response`
- Buffer and window inspection: `buffers`, `windows`, `buffer_binding`,
  `vfs_request_ledger`, `vfs_transaction_log`
- Registers and options: `register`, `set_register`, `get_option_number`,
  `get_option_bool`, `get_option_string`, `set_option_number`,
  `set_option_bool`, `set_option_string`
- Search and syntax: `get_search_pattern`, `is_hlsearch_active`,
  `get_search_direction`, `get_search_highlights`,
  `get_cursor_match_info`, `is_incsearch_active`,
  `get_incsearch_pattern`, `get_syntax_name`, `get_line_syntax`
- Undo and backend metadata: `get_undo_tree`, `undo_jump`,
  `backend_identity`
- Job and VFD bridge helpers: `inject_vfd_data`, `notify_job_status`

### Internal-only areas

- `src/lib.rs`
  `ParsedExIntent`, `invoke_native_normal_command`,
  `invoke_native_ex_command`, `apply_intent`,
  `apply_write_intent`, `apply_loaded_buffer`, `drain_native_host_actions`,
  `drain_native_events`, `get_option_value`, and conversion helpers
- `src/vfs.rs`
  `DocumentCoordinator`, `BufferState`, `CoreResponseApplyOutcome`, and
  response helper methods
- `src/vfd.rs`
  `pollfd`, `POLLIN`, `POLLOUT`, `VfdState`, `JobState`, `VfdManager`,
  `get_manager`, and `vim_core_*` exported shims

## How to choose the next document

- If you need callable surface area as a crate user, read
  [public-api-reference.md](public-api-reference.md).
- If you need implementation boundaries as a maintainer, read
  [internal-api-reference.md](internal-api-reference.md).
- If you need sequencing or invariants, read
  [api-contracts.md](api-contracts.md).
- If you need current caveats before trusting an exposed API, read
  [known-limitations.md](known-limitations.md).
