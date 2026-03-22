# Scope and non-goals

This document defines what `vim-core-rs` is responsible for and what it is
not allowed to become. Read this page before extending the crate. The project
is intentionally narrow.

If you need exact signatures or behavior contracts, continue with
`api-index.md`, `public-api-reference.md`, `internal-api-reference.md`, and
`api-contracts.md`.

## Design context

This crate exists as the editing-core component of a larger editor
architecture.

- Rust owns rendering, the event loop, I/O orchestration, and higher-level
  state such as Tree-sitter integration.
- The extension or scripting layer belongs outside this crate.
- `vim-core-rs` owns embedded Vim editing semantics and selected runtime state
  extraction.

Because of that split, the crate must behave like a modal text-editing engine
with a host integration boundary. It must not try to become the whole editor.

## In scope

These capabilities are inside the intended product boundary.

### Buffer and text state

The crate owns core text-editing semantics.

- Insert, delete, and replace text through embedded Vim behavior.
- Track revisions, dirty state, cursor position, and window viewport state.
- Expose undo tree information and allow undo jumps.

### Modal input and command semantics

The crate owns Vim-like input behavior.

- Maintain mode transitions across Normal, Insert, Visual, Select, Replace,
  Command-line, and Operator-pending modes.
- Execute Normal-mode command strings.
- Execute Ex commands, including Rust-side interception of file-like Ex
  commands that must cross the host boundary.
- Expose registers, marks, jumplist state, and message events.

### Rendering-adjacent extraction

The crate may expose data that a host renderer can consume directly.

- Snapshot buffers and windows.
- Search pattern state and search match ranges.
- Syntax chunks derived from the embedded Vim runtime.
- Pop-up menu state and items.

### Host-mediated file and job integration

The crate may request work from the host.

- Convert file-like Ex flows into explicit VFS requests or host write actions.
- Track VFS request ledger state, transaction logs, and deferred close flows.
- Convert Vim job requests into host-managed process actions with VFD bridging.

## Out of scope

These areas are intentionally outside the project boundary.

### Full scripting-platform exposure

The crate is not a general embedding of all Vimscript or Lua capabilities.

- Do not expand the crate toward a full plugin-hosting surface.
- Do not treat Vimscript or Lua interoperability as the primary extension
  model.
- Keep complex extension logic outside the core.

### Host-owned asynchronous orchestration

The crate is not the application's async runtime.

- Do not move general async orchestration, networking, or background task
  ownership into the core.
- Do not model the embedded Vim runtime as the authoritative event loop.
- Keep modern async coordination in the host.

### Modern semantic parsing and highlighting

The crate is not the main syntax or semantic analysis engine.

- Do not expand Vim regex syntax extraction into a full semantic pipeline.
- Treat Vim-derived syntax information as renderer input or fallback behavior.
- Keep modern parsing systems such as Tree-sitter outside this crate.

### Virtual text and overlay composition

The crate is not responsible for host-side overlay rendering.

- Do not move inline hints, diagnostics overlays, or virtual text layout into
  the embedded Vim core.
- Keep overlay composition in Rust-side rendering systems.

### Terminal emulator ownership

The crate is not the terminal subsystem.

- Do not embed a rich terminal emulator into the core.
- Do not widen the scope toward `:terminal` feature parity.
- Keep PTY ownership and terminal rendering in the host.

## Decision filter

Use this filter when a proposed change is ambiguous.

- If the feature is pure modal editing semantics, it likely belongs here.
- If the feature is state extraction from embedded Vim, it may belong here.
- If the feature is persistence, process management, rendering, plugin
  hosting, semantic parsing, or async orchestration, it likely belongs in the
  host instead.

## Next steps

Read `known-limitations.md` for current gaps and partially implemented areas.
Then read `api-contracts.md` for the sequencing rules that sit on top of this
scope boundary.
