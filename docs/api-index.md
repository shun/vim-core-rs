# API documentation index

This directory documents the complete API surface of `vim-core-rs` for
LLM-first consumption. Use these pages when you need to reason about the
crate without reopening `src/lib.rs`, `src/vfs.rs`, or `src/vfd.rs`.

Start with the repository-root `README.md` for purpose and invariants, then
`SCOPE.md` for design boundaries, then `known-limitations.md` for current
gaps. Only after that should you descend into the API reference pages.

## What each document covers

Read the pages in this order when you need the full mental model.

- The repository-root `README.md` explains repository purpose, hard
  invariants, host obligations, and the main assumptions that are unsafe.
- `SCOPE.md` defines the intended product boundary and non-goals.
- `known-limitations.md` lists current implementation gaps and intentionally
  incomplete behavior.
- `public-api-reference.md` documents every crate-public module, type, enum
  variant family, alias, and method that a host application can call.
- `internal-api-reference.md` documents every non-public Rust API that the
  crate uses to implement the public surface, including private helper
  functions, crate-visible coordination layers, and C ABI shims.
- `api-contracts.md` documents the behavior contracts that matter more than
  raw signatures, including session ownership, host-action flow, VFS
  sequencing, message delivery, and job bridging.

## Symbol map

This section gives you a fast lookup path before you jump into the detailed
references.

### Public root symbols

The crate root exports these user-facing symbols.

- Module: `ffi`
- Session type: `VimCoreSession`
- Type alias: `MessageHandler`
- Enums: `CoreMode`, `CorePendingInput`, `CoreCommandOutcome`,
  `CoreInputRequestKind`, `CoreBackendIdentity`, `CoreOptionScope`,
  `CoreOptionType`, `CoreOptionError`, `JobStatus`, `CoreHostAction`,
  `CoreMessageKind`, `CoreMatchType`, `MatchCountResult`,
  `CoreSearchDirection`, `CoreCommandError`, `CoreSessionError`
- Structs: `CoreMarkPosition`, `CoreJumpListEntry`, `CoreJumpList`,
  `CoreJobStartRequest`, `CoreBufferInfo`, `CoreWindowInfo`, `CoreUndoNode`,
  `CoreUndoTree`, `CoreSyntaxChunk`, `CoreMessageEvent`, `CorePumItem`,
  `CorePumInfo`, `CoreMatchRange`, `CoreCursorMatchInfo`, `CoreSnapshot`
- Re-exported VFS enums and structs: `CoreBufferBinding`,
  `CoreBufferSourceKind`, `CoreDeferredClose`, `CorePendingVfsOperation`,
  `CoreRequestEntry`, `CoreRequestStatus`, `CoreVfsError`,
  `CoreVfsErrorKind`, `CoreVfsOperationKind`, `CoreVfsRequest`,
  `CoreVfsResponse`, `VfsLogEntry`, `VfsLogEvent`

### Public `VimCoreSession` methods

The public session methods are grouped by role.

- Lifecycle and snapshots: `new`, `snapshot`, `mode`, `pending_input`
- Navigation and state writes: `mark`, `set_mark`, `jumplist`,
  `switch_to_buffer`, `switch_to_window`, `buffer_text`
- Command execution: `apply_normal_command`, `apply_ex_command`,
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

### Internal-only symbols

The internal reference breaks non-public APIs into these areas.

- `src/lib.rs`: `ParsedExIntent`, `apply_native_ex_command`, `apply_intent`,
  `apply_write_intent`, `apply_loaded_buffer`, `drain_native_host_actions`,
  `get_option_value`, `poll_and_dispatch_messages`, conversion helpers, and
  parser helpers
- `src/vfs.rs`: `DocumentCoordinator`, `BufferState`,
  `CoreResponseApplyOutcome`, and `CoreVfsResponse` helper methods
- `src/vfd.rs`: `pollfd`, `POLLIN`, `POLLOUT`, `VfdState`, `JobState`,
  `VfdManager`, `get_manager`, and the `vim_core_*` exported C functions

## Reachability rules

This section explains how to interpret "public" versus "internal" in this
crate, because `src/vfd.rs` contains `pub` items inside a private module.

- A symbol counts as public if a downstream crate can name it through the
  crate root.
- A symbol counts as internal if it is only visible inside this crate, even
  when it uses the Rust `pub` keyword in a private module.
- The `ffi` module is public, but it is intentionally narrow. It exists to
  expose POD types and constants that tests and hosts need for FFI contracts.

## Next steps

Use `public-api-reference.md` when you need callable surface area. Use
`internal-api-reference.md` when you need implementation boundaries. Use
`api-contracts.md` when you need sequencing, invariants, or host obligations.
