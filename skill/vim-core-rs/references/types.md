# vim-core-rs types and models

This document summarizes the public data model that host applications and tests
observe when working with `VimCoreSession`.

## Core snapshot and state
```rust
pub struct CoreSnapshot {
    pub text: String,
    pub revision: u64,
    pub dirty: bool,
    pub mode: CoreMode,
    pub pending_input: CorePendingInput,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub pending_host_actions: usize,
    pub buffers: Vec<CoreBufferInfo>,
    pub windows: Vec<CoreWindowInfo>,
    pub pum: Option<CorePumInfo>, // Pop-up menu (autocompletion)
}

pub enum CoreMode {
    Normal, Insert, Visual, VisualLine, VisualBlock,
    Replace, Select, SelectLine, SelectBlock,
    CommandLine, OperatorPending,
}

pub enum CorePendingInput {
    None, Char, Replace, MarkSet, MarkJump, Register,
}
```

`CoreSnapshot` is the most complete state capture. Prefer it when a test or
host update needs text, cursor, mode, window layout, and completion state from
one coherent read.

## Command outcomes and errors
```rust
pub enum CoreCommandOutcome {
    NoChange,
    BufferChanged { revision: u64 },
    CursorChanged { row: usize, col: usize },
    ModeChanged { mode: CoreMode },
    HostActionQueued, // Signals that host actions should be processed
}

pub enum CoreCommandError {
    InvalidInput,
    OperationFailed { reason_code: u32 },
    UnknownStatus { status: u32, reason_code: u32 },
}

pub enum CoreSessionError {
    SessionAlreadyActive,
    InitializationFailed { reason_code: &'static str },
    CommandFailed(CoreCommandError),
}
```

## Host actions
```rust
pub enum CoreHostAction {
    VfsRequest(CoreVfsRequest),
    Write { path: String, force: bool, issued_after_revision: u64 }, // Local save
    Quit { force: bool, issued_after_revision: u64 },
    Redraw { full: bool, clear_before_draw: bool },
    RequestInput { prompt: String, input_kind: CoreInputRequestKind, correlation_id: u64 },
    Bell,
    BufAdd { buf_id: i32 },
    WinNew { win_id: i32 },
    LayoutChanged,
    JobStart(CoreJobStartRequest),
    JobStop { job_id: i32 },
}

pub enum CoreInputRequestKind {
    CommandLine, Confirmation, Secret,
}
```

Treat `CoreHostAction` as part of the public API contract. Repository tests
assert both its presence and its payload details.

## Navigation, search, and rendering models
```rust
pub struct CoreMarkPosition {
    pub buf_id: i32,
    pub row: usize,
    pub col: usize,
}

pub struct CoreJumpListEntry {
    pub buf_id: i32,
    pub row: usize,
    pub col: usize,
}

pub struct CoreJumpList {
    pub current_index: usize,
    pub entries: Vec<CoreJumpListEntry>,
}

pub enum CoreSearchDirection {
    Forward,
    Backward,
}

pub enum CoreMatchType {
    Regular,
    IncSearch,
    CurSearch,
}

pub struct CoreMatchRange {
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
    pub match_type: CoreMatchType,
}
```

## Undo, syntax, and completion
```rust
pub struct CoreUndoTree {
    pub nodes: Vec<CoreUndoNode>,
    pub synced: bool,
    pub seq_last: i32,
    pub save_last: i32,
    pub seq_cur: i32,
    pub time_cur: i64,
    pub save_cur: i32,
}

pub struct CoreUndoNode {
    pub seq: i32,
    pub time: i64,
    pub save_nr: i32,
    pub prev_seq: Option<i32>,
    pub next_seq: Option<i32>,
    pub alt_next_seq: Option<i32>,
    pub alt_prev_seq: Option<i32>,
    pub is_newhead: bool,
    pub is_curhead: bool,
}

pub struct CoreSyntaxChunk {
    pub start_col: usize,
    pub end_col: usize,
    pub syn_id: i32,
    pub name: Option<String>,
}

pub struct CorePumItem {
    pub word: String,
    pub abbr: String,
    pub menu: String,
    pub kind: String,
    pub info: String,
}

pub struct CorePumInfo {
    pub row: i32,
    pub col: i32,
    pub width: i32,
    pub height: i32,
    pub selected_index: Option<usize>,
    pub items: Vec<CorePumItem>,
}
```

## Search match metadata and messaging
```rust
pub struct CoreCursorMatchInfo {
    pub is_on_match: bool,
    pub current_match_index: usize,
    pub total_matches: MatchCountResult,
}

pub enum MatchCountResult {
    Calculated(usize), MaxReached(usize), TimedOut,
}

pub struct CoreMessageEvent {
    pub kind: CoreMessageKind,
    pub content: String,
}

pub enum CoreMessageKind {
    Normal, Error, // Distinguishes between standard echo and e.g., 'E487: ...'
}
```

## Buffer, window, and option metadata
```rust
pub struct CoreBufferInfo {
    pub id: i32,
    pub name: String,
    pub dirty: bool,
    pub is_active: bool,
    pub source_kind: CoreBufferSourceKind, // Local or Vfs
    pub document_id: Option<String>,
    pub pending_vfs_operation: Option<CorePendingVfsOperation>,
    pub deferred_close: Option<CoreDeferredClose>,
    pub last_vfs_error: Option<CoreVfsError>,
}

pub struct CoreWindowInfo {
    pub id: i32,
    pub buf_id: i32,
    pub row: usize,
    pub col: usize,
    pub width: usize,
    pub height: usize,
    pub topline: usize,
    pub botline: usize,
    pub leftcol: usize,
    pub skipcol: usize,
    pub is_active: bool,
}

pub enum CoreOptionScope {
    Default,
    Global,
    Local,
}

pub enum CoreOptionType {
    Bool,
    Number,
    String,
}

pub enum CoreOptionError {
    UnknownOption { name: String },
    TypeMismatch {
        name: String,
        expected: CoreOptionType,
        actual: CoreOptionType,
    },
    SetFailed { name: String, reason: String },
    ScopeNotSupported { name: String, scope: CoreOptionScope },
    InternalError { name: String, detail: String },
}
```

## VFS and job-facing models

For VFS payloads, request status, deferred close variants, and job start
requests, read [vfs-vfd.md](vfs-vfd.md). Those types are operationally dense
enough that they are better understood with their flow semantics.
