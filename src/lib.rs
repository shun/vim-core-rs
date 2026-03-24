use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::CString;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::rc::Rc;
use std::slice;
use std::str;
use std::sync::atomic::{AtomicBool, Ordering};

macro_rules! debug_log {
    ($($arg:tt)*) => {
        crate::debug_log::emit(format_args!($($arg)*))
    };
}

mod debug_log;
#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    unused_imports
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

mod vfd;
mod vfs;

/// FFI 境界の POD 型を公開するモジュール。
/// bindgen で生成された型のうち、VFS POD 契約に必要なものを re-export する。
/// テストコードから FFI 契約の存在を検証するために使用する。
pub mod ffi {
    pub use crate::bindings::vim_core_buffer_commit_t;
    pub use crate::bindings::vim_core_buffer_info_t;

    // VFS operation kind 定数
    pub const VIM_CORE_VFS_OPERATION_NONE: u32 =
        crate::bindings::vim_core_vfs_operation_kind_VIM_CORE_VFS_OPERATION_NONE;
    pub const VIM_CORE_VFS_OPERATION_RESOLVE: u32 =
        crate::bindings::vim_core_vfs_operation_kind_VIM_CORE_VFS_OPERATION_RESOLVE;
    pub const VIM_CORE_VFS_OPERATION_EXISTS: u32 =
        crate::bindings::vim_core_vfs_operation_kind_VIM_CORE_VFS_OPERATION_EXISTS;
    pub const VIM_CORE_VFS_OPERATION_LOAD: u32 =
        crate::bindings::vim_core_vfs_operation_kind_VIM_CORE_VFS_OPERATION_LOAD;
    pub const VIM_CORE_VFS_OPERATION_SAVE: u32 =
        crate::bindings::vim_core_vfs_operation_kind_VIM_CORE_VFS_OPERATION_SAVE;

    // Buffer source kind 定数
    pub const VIM_CORE_BUFFER_SOURCE_LOCAL: u32 =
        crate::bindings::vim_core_buffer_source_kind_VIM_CORE_BUFFER_SOURCE_LOCAL;
    pub const VIM_CORE_BUFFER_SOURCE_VFS: u32 =
        crate::bindings::vim_core_buffer_source_kind_VIM_CORE_BUFFER_SOURCE_VFS;
}

use vfs::DocumentCoordinator;
pub use vfs::{
    CoreBufferBinding, CoreBufferSourceKind, CoreDeferredClose, CorePendingVfsOperation,
    CoreRequestEntry, CoreRequestStatus, CoreVfsError, CoreVfsErrorKind, CoreVfsOperationKind,
    CoreVfsRequest, CoreVfsResponse, VfsLogEntry, VfsLogEvent,
};

static ACTIVE_SESSION: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreMode {
    Normal,
    Insert,
    Visual,
    VisualLine,
    VisualBlock,
    Replace,
    Select,
    SelectLine,
    SelectBlock,
    CommandLine,
    OperatorPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorePendingInput {
    None,
    Char,
    Replace,
    MarkSet,
    MarkJump,
    Register,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreMarkPosition {
    pub buf_id: i32,
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreJumpListEntry {
    pub buf_id: i32,
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreJumpList {
    pub current_index: usize,
    pub entries: Vec<CoreJumpListEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreCommandOutcome {
    NoChange,
    BufferChanged { revision: u64 },
    CursorChanged { row: usize, col: usize },
    ModeChanged { mode: CoreMode },
    HostActionQueued,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreInputRequestKind {
    CommandLine,
    Confirmation,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreBackendIdentity {
    BridgeStub,
    UpstreamRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreOptionScope {
    Default,
    Global,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreOptionType {
    Bool,
    Number,
    String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreOptionError {
    UnknownOption {
        name: String,
    },
    TypeMismatch {
        name: String,
        expected: CoreOptionType,
        actual: CoreOptionType,
    },
    SetFailed {
        name: String,
        reason: String,
    },
    ScopeNotSupported {
        name: String,
        scope: CoreOptionScope,
    },
    InternalError {
        name: String,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreJobStartRequest {
    pub job_id: i32,
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub vfd_in: i32,
    pub vfd_out: i32,
    pub vfd_err: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Running = 0,
    Finished = 1,
    Failed = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreHostAction {
    VfsRequest(CoreVfsRequest),
    Write {
        path: String,
        force: bool,
        issued_after_revision: u64,
    },
    Quit {
        force: bool,
        issued_after_revision: u64,
    },
    Redraw {
        full: bool,
        clear_before_draw: bool,
    },
    RequestInput {
        prompt: String,
        input_kind: CoreInputRequestKind,
        correlation_id: u64,
    },
    Bell,
    BufAdd {
        buf_id: i32,
    },
    WinNew {
        win_id: i32,
    },
    LayoutChanged,
    JobStart(CoreJobStartRequest),
    JobStop {
        job_id: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreBufferInfo {
    pub id: i32,
    pub name: String,
    pub dirty: bool,
    pub is_active: bool,
    pub source_kind: CoreBufferSourceKind,
    pub document_id: Option<String>,
    pub pending_vfs_operation: Option<CorePendingVfsOperation>,
    pub deferred_close: Option<CoreDeferredClose>,
    pub last_vfs_error: Option<CoreVfsError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreUndoTree {
    pub nodes: Vec<CoreUndoNode>,
    pub synced: bool,
    pub seq_last: i32,
    pub save_last: i32,
    pub seq_cur: i32,
    pub time_cur: i64,
    pub save_cur: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreSyntaxChunk {
    pub start_col: usize,
    pub end_col: usize,
    pub syn_id: i32,
    pub name: Option<String>,
}

/// Vimから発せられたメッセージの種別
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreMessageKind {
    /// 通常のメッセージ
    Normal,
    /// エラーメッセージ（Eから始まるメッセージ等）
    Error,
}

/// メッセージイベントのペイロード
#[derive(Debug, Clone)]
pub struct CoreMessageEvent {
    pub kind: CoreMessageKind,
    pub content: String,
}

/// メッセージハンドラの型定義
pub type MessageHandler = Box<dyn FnMut(CoreMessageEvent) + Send + 'static>;

/// 補完候補1件分の情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorePumItem {
    pub word: String,
    pub abbr: String,
    pub menu: String,
    pub kind: String,
    pub info: String,
}

/// ポップアップメニュー（補完候補メニュー）全体の情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorePumInfo {
    pub row: i32,
    pub col: i32,
    pub width: i32,
    pub height: i32,
    /// 現在選択されている候補のインデックス。未選択時は None。
    pub selected_index: Option<usize>,
    pub items: Vec<CorePumItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreMatchType {
    Regular,
    IncSearch,
    CurSearch,
}

impl From<bindings::vim_core_match_type_t> for CoreMatchType {
    fn from(t: bindings::vim_core_match_type_t) -> Self {
        match t {
            bindings::vim_core_match_type_t_VIM_CORE_MATCH_REGULAR => CoreMatchType::Regular,
            bindings::vim_core_match_type_t_VIM_CORE_MATCH_INCSEARCH => CoreMatchType::IncSearch,
            bindings::vim_core_match_type_t_VIM_CORE_MATCH_CURSEARCH => CoreMatchType::CurSearch,
            _ => CoreMatchType::Regular,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreMatchRange {
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
    pub match_type: CoreMatchType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchCountResult {
    Calculated(usize),
    MaxReached(usize),
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreCursorMatchInfo {
    pub is_on_match: bool,
    pub current_match_index: usize,
    pub total_matches: MatchCountResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreSearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// ポップアップメニュー情報。補完メニューが表示中の場合に Some。
    pub pum: Option<CorePumInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCommandError {
    InvalidInput,
    OperationFailed { reason_code: u32 },
    UnknownStatus { status: u32, reason_code: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreSessionError {
    SessionAlreadyActive,
    InitializationFailed { reason_code: &'static str },
    CommandFailed(CoreCommandError),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoreSessionOptions {
    pub debug_log_path: Option<PathBuf>,
}

pub struct VimCoreSession {
    state: NonNull<bindings::vim_bridge_state_t>,
    document_coordinator: RefCell<DocumentCoordinator>,
    pending_host_actions: RefCell<VecDeque<CoreHostAction>>,
    message_handler: Option<MessageHandler>,
    not_send_sync: PhantomData<Rc<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedExIntent {
    Edit { locator: String },
    Write { path: String, force: bool },
    Update { path: String, force: bool },
    SaveAndClose { force: bool },
    SaveIfDirtyAndClose,
    Quit { force: bool },
}

impl VimCoreSession {
    pub fn new(initial_text: &str) -> Result<Self, CoreSessionError> {
        Self::new_with_options(initial_text, CoreSessionOptions::default())
    }

    pub fn new_with_options(
        initial_text: &str,
        options: CoreSessionOptions,
    ) -> Result<Self, CoreSessionError> {
        if ACTIVE_SESSION
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(CoreSessionError::SessionAlreadyActive);
        }

        let log_config = debug_log::DebugLogConfig {
            path: options.debug_log_path,
        };
        if debug_log::configure(&log_config).is_err() {
            ACTIVE_SESSION.store(false, Ordering::Release);
            return Err(CoreSessionError::InitializationFailed {
                reason_code: "debug_log_init_failed",
            });
        }
        let native_log_path = log_config
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        match native_log_path.as_deref() {
            Some(path) => unsafe {
                bindings::vim_bridge_set_debug_log_path(path.as_ptr().cast(), path.len())
            },
            None => unsafe { bindings::vim_bridge_set_debug_log_path(std::ptr::null(), 0) },
        }

        let state_ptr = unsafe {
            bindings::vim_bridge_state_new(initial_text.as_ptr().cast(), initial_text.len())
        };

        let Some(state) = NonNull::new(state_ptr) else {
            ACTIVE_SESSION.store(false, Ordering::Release);
            return Err(CoreSessionError::InitializationFailed {
                reason_code: "state_new_returned_null",
            });
        };

        let session = Self {
            state,
            document_coordinator: RefCell::new(DocumentCoordinator::new()),
            pending_host_actions: RefCell::new(VecDeque::new()),
            message_handler: None,
            not_send_sync: PhantomData,
        };

        // /* println debug removed */
        Ok(session)
    }

    pub fn snapshot(&self) -> CoreSnapshot {
        let snapshot = unsafe { bindings::vim_bridge_snapshot(self.state.as_ptr()) };
        let mut snapshot = convert_snapshot(snapshot);
        let buffer_seeds = snapshot
            .buffers
            .iter()
            .map(|buffer| (buffer.id, buffer.name.clone()))
            .collect::<Vec<_>>();

        let mut coordinator = self.document_coordinator.borrow_mut();
        coordinator.sync_buffers(&buffer_seeds);
        for buffer in &mut snapshot.buffers {
            if let Some(binding) = coordinator.binding(buffer.id) {
                buffer.source_kind = binding.source_kind;
                buffer.document_id = binding.document_id.clone();
                buffer.pending_vfs_operation = binding.pending_operation;
                buffer.deferred_close = binding.deferred_close;
                buffer.last_vfs_error = binding.last_vfs_error.clone();
            }
        }

        snapshot.pending_host_actions += self.pending_host_actions.borrow().len();

        snapshot
    }

    pub fn mode(&self) -> CoreMode {
        self.snapshot().mode
    }

    pub fn pending_input(&self) -> CorePendingInput {
        let pending_input = unsafe { bindings::vim_bridge_get_pending_input(self.state.as_ptr()) };
        convert_pending_input(pending_input)
    }

    pub fn mark(&self, mark_name: char) -> Option<CoreMarkPosition> {
        let mark_name_c = mark_name as std::os::raw::c_char;
        let mut mark = unsafe { std::mem::zeroed::<bindings::vim_core_mark_position_t>() };
        let is_set =
            unsafe { bindings::vim_bridge_get_mark(self.state.as_ptr(), mark_name_c, &mut mark) };
        if !is_set || !mark.is_set {
            return None;
        }

        Some(convert_mark_position(mark))
    }

    pub fn set_mark(
        &mut self,
        mark_name: char,
        buf_id: i32,
        row: usize,
        col: usize,
    ) -> Result<(), CoreCommandError> {
        let mark_name_c = mark_name as std::os::raw::c_char;
        let status = unsafe {
            bindings::vim_bridge_set_mark(self.state.as_ptr(), mark_name_c, buf_id, row, col)
        };
        convert_status(status)
    }

    pub fn jumplist(&self) -> CoreJumpList {
        let jumplist = unsafe { bindings::vim_bridge_get_jumplist(self.state.as_ptr()) };
        convert_jumplist(jumplist)
    }

    pub fn inject_vfd_data(&mut self, vfd: i32, data: &[u8]) -> Result<(), CoreCommandError> {
        let mut mgr = crate::vfd::get_manager();
        if mgr.inject_data(vfd, data) {
            Ok(())
        } else {
            Err(CoreCommandError::InvalidInput)
        }
    }

    pub fn notify_job_status(
        &mut self,
        job_id: i32,
        status: JobStatus,
        exit_code: i32,
    ) -> Result<(), CoreCommandError> {
        let mut mgr = crate::vfd::get_manager();
        if mgr.update_job_status(job_id, status as i32, exit_code) {
            Ok(())
        } else {
            Err(CoreCommandError::InvalidInput)
        }
    }

    pub fn apply_normal_command(
        &mut self,
        command: &str,
    ) -> Result<CoreCommandOutcome, CoreCommandError> {
        let result = unsafe {
            bindings::vim_bridge_apply_normal_command(
                self.state.as_ptr(),
                command.as_ptr().cast(),
                command.len(),
            )
        };
        let outcome = convert_command_result(result);
        self.poll_and_dispatch_messages();
        outcome
    }

    pub fn apply_ex_command(
        &mut self,
        command: &str,
    ) -> Result<CoreCommandOutcome, CoreCommandError> {
        if let Some(intent) = parse_ex_intent(command) {
            return self.apply_intent(intent);
        }

        self.apply_native_ex_command(command)
    }

    pub fn submit_vfs_response(
        &mut self,
        response: CoreVfsResponse,
    ) -> Result<CoreCommandOutcome, CoreCommandError> {
        let request_id = response.request_id();
        let mut coordinator = self.document_coordinator.borrow_mut();
        let Some(entry) = coordinator.request_entry(request_id) else {
            debug_log!(
                "[DEBUG] submit_vfs_response: unknown request_id={} response={:?}",
                request_id,
                response
            );
            return Err(CoreCommandError::InvalidInput);
        };

        debug_log!(
            "[DEBUG] submit_vfs_response: request_id={} buf_id={} operation={:?} response={:?}",
            request_id,
            entry.target_buf_id,
            entry.operation_kind,
            response
        );

        match response.clone() {
            CoreVfsResponse::Resolved { document_id, .. } => {
                let apply_outcome = coordinator.apply_response(response);
                if !matches!(apply_outcome, vfs::CoreResponseApplyOutcome::Applied) {
                    return Err(CoreCommandError::OperationFailed { reason_code: 0 });
                }
                let request = coordinator.issue_load(entry.target_buf_id, document_id);
                drop(coordinator);
                self.pending_host_actions
                    .borrow_mut()
                    .push_back(CoreHostAction::VfsRequest(request));
                Ok(CoreCommandOutcome::HostActionQueued)
            }
            CoreVfsResponse::ResolvedLocalFallback { locator, .. } => {
                let apply_outcome = coordinator.apply_response(response);
                if !matches!(apply_outcome, vfs::CoreResponseApplyOutcome::Applied) {
                    return Err(CoreCommandError::OperationFailed { reason_code: 0 });
                }
                drop(coordinator);
                let command = format!(":edit {}", locator);
                let outcome = self.apply_native_ex_command(&command)?;
                let snapshot = self.snapshot();
                let revision = snapshot.revision;
                let active_buffer = snapshot
                    .buffers
                    .iter()
                    .find(|buffer| buffer.is_active)
                    .ok_or(CoreCommandError::OperationFailed { reason_code: 0 })?;
                self.document_coordinator.borrow_mut().bind_local_buffer(
                    active_buffer.id,
                    Some(locator),
                    active_buffer.name.clone(),
                    revision,
                );
                Ok(outcome)
            }
            CoreVfsResponse::Loaded {
                document_id, text, ..
            } => {
                let apply_outcome = coordinator.apply_response(response.clone());
                if !matches!(apply_outcome, vfs::CoreResponseApplyOutcome::Applied) {
                    return Err(CoreCommandError::OperationFailed { reason_code: 0 });
                }
                let display_name = coordinator
                    .binding(entry.target_buf_id)
                    .map(|binding| binding.display_name.clone())
                    .unwrap_or_else(|| document_id.clone());
                drop(coordinator);

                self.apply_loaded_buffer(entry.target_buf_id, &display_name, &text)?;

                let snapshot = self.snapshot();
                self.document_coordinator
                    .borrow_mut()
                    .commit_loaded_revision(entry.target_buf_id, snapshot.revision);

                Ok(CoreCommandOutcome::BufferChanged {
                    revision: snapshot.revision,
                })
            }
            CoreVfsResponse::Saved { document_id, .. } => {
                let target_buf_id = entry.target_buf_id;
                let apply_outcome = coordinator.apply_response(response.clone());

                match apply_outcome {
                    vfs::CoreResponseApplyOutcome::Applied => {
                        debug_log!(
                            "[DEBUG] submit_vfs_response: save success applied buf_id={} document_id={}",
                            target_buf_id,
                            document_id
                        );
                        // dirty フラグをクリア
                        drop(coordinator);
                        let status = unsafe {
                            bindings::vim_bridge_set_buffer_dirty(
                                self.state.as_ptr(),
                                target_buf_id,
                                false,
                            )
                        };
                        convert_status(status)?;

                        // deferred close をチェック
                        let deferred = self
                            .document_coordinator
                            .borrow()
                            .deferred_close(target_buf_id);
                        if let Some(close_kind) = deferred {
                            debug_log!(
                                "[DEBUG] submit_vfs_response: deferred close triggered buf_id={} kind={:?}",
                                target_buf_id,
                                close_kind
                            );
                            self.document_coordinator
                                .borrow_mut()
                                .clear_deferred_close(target_buf_id, "save applied");
                            let snapshot = self.snapshot();
                            self.pending_host_actions.borrow_mut().push_back(
                                CoreHostAction::Quit {
                                    force: false,
                                    issued_after_revision: snapshot.revision,
                                },
                            );
                        }

                        Ok(CoreCommandOutcome::NoChange)
                    }
                    vfs::CoreResponseApplyOutcome::StaleRejected => {
                        debug_log!(
                            "[DEBUG] submit_vfs_response: save stale rejected buf_id={} document_id={}",
                            target_buf_id,
                            document_id
                        );
                        // revision mismatch の場合、deferred close もクリアして拒否
                        let deferred = coordinator.deferred_close(target_buf_id);
                        if deferred.is_some() {
                            coordinator
                                .clear_deferred_close(target_buf_id, "save rejected as stale");
                            debug_log!(
                                "[DEBUG] submit_vfs_response: deferred close cleared due to stale save buf_id={}",
                                target_buf_id
                            );
                        }
                        drop(coordinator);
                        Ok(CoreCommandOutcome::NoChange)
                    }
                    _ => {
                        drop(coordinator);
                        Ok(CoreCommandOutcome::NoChange)
                    }
                }
            }
            CoreVfsResponse::Failed { .. }
            | CoreVfsResponse::Cancelled { .. }
            | CoreVfsResponse::TimedOut { .. } => {
                let target_buf_id = entry.target_buf_id;
                let apply_outcome = coordinator.apply_response(response);

                // save failure, cancel, timeout の場合は deferred close をクリア
                let deferred = coordinator.deferred_close(target_buf_id);
                if deferred.is_some() {
                    coordinator.clear_deferred_close(target_buf_id, "save failed or interrupted");
                    debug_log!(
                        "[DEBUG] submit_vfs_response: deferred close cleared due to save failure/cancel/timeout buf_id={}",
                        target_buf_id
                    );
                }
                drop(coordinator);

                if matches!(apply_outcome, vfs::CoreResponseApplyOutcome::UnknownRequest) {
                    return Err(CoreCommandError::InvalidInput);
                }
                Ok(CoreCommandOutcome::NoChange)
            }
            _ => {
                let apply_outcome = coordinator.apply_response(response);
                if matches!(apply_outcome, vfs::CoreResponseApplyOutcome::UnknownRequest) {
                    return Err(CoreCommandError::InvalidInput);
                }
                Ok(CoreCommandOutcome::NoChange)
            }
        }
    }

    fn apply_native_ex_command(
        &mut self,
        command: &str,
    ) -> Result<CoreCommandOutcome, CoreCommandError> {
        let result = unsafe {
            bindings::vim_bridge_apply_ex_command(
                self.state.as_ptr(),
                command.as_ptr().cast(),
                command.len(),
            )
        };
        let outcome = convert_command_result(result)?;
        self.drain_native_host_actions();
        self.poll_and_dispatch_messages();
        Ok(outcome)
    }

    fn apply_intent(
        &mut self,
        intent: ParsedExIntent,
    ) -> Result<CoreCommandOutcome, CoreCommandError> {
        match intent {
            ParsedExIntent::Edit { locator } => {
                let target_buf_id = self
                    .snapshot()
                    .buffers
                    .iter()
                    .find(|buffer| buffer.is_active)
                    .map(|buffer| buffer.id)
                    .ok_or(CoreCommandError::OperationFailed { reason_code: 0 })?;
                let request = self
                    .document_coordinator
                    .borrow_mut()
                    .issue_resolve(target_buf_id, locator);
                self.pending_host_actions
                    .borrow_mut()
                    .push_back(CoreHostAction::VfsRequest(request));
                Ok(CoreCommandOutcome::HostActionQueued)
            }
            ParsedExIntent::Write { path, force } => self.apply_write_intent(path, force, None),
            ParsedExIntent::Update { path, force } => {
                let snapshot = self.snapshot();
                let active_buf = snapshot
                    .buffers
                    .iter()
                    .find(|b| b.is_active)
                    .ok_or(CoreCommandError::OperationFailed { reason_code: 0 })?;
                let buf_id = active_buf.id;

                let is_vfs = self.document_coordinator.borrow().is_vfs_buffer(buf_id);

                // VFS buffer の場合: :update は dirty な場合のみ save
                if is_vfs && !snapshot.dirty {
                    debug_log!(
                        "[DEBUG] apply_intent: :update on clean VFS buffer buf_id={}, skipping save",
                        buf_id
                    );
                    return Ok(CoreCommandOutcome::NoChange);
                }

                self.apply_write_intent(path, force, None)
            }
            ParsedExIntent::SaveAndClose { force } => {
                let snapshot = self.snapshot();
                let active_buf = snapshot
                    .buffers
                    .iter()
                    .find(|b| b.is_active)
                    .ok_or(CoreCommandError::OperationFailed { reason_code: 0 })?;
                let buf_id = active_buf.id;

                let is_vfs = self.document_coordinator.borrow().is_vfs_buffer(buf_id);
                if is_vfs {
                    debug_log!(
                        "[DEBUG] apply_intent: :wq on VFS buffer buf_id={}, initiating save-and-close",
                        buf_id
                    );
                    self.document_coordinator
                        .borrow_mut()
                        .set_deferred_close(buf_id, CoreDeferredClose::SaveAndClose);
                    self.apply_write_intent(String::new(), force, None)
                } else {
                    // local buffer: 既存フロー -- :wq は Quit として扱う（host が save を管理）
                    let revision = snapshot.revision;
                    self.pending_host_actions
                        .borrow_mut()
                        .push_back(CoreHostAction::Quit {
                            force,
                            issued_after_revision: revision,
                        });
                    Ok(CoreCommandOutcome::HostActionQueued)
                }
            }
            ParsedExIntent::SaveIfDirtyAndClose => {
                let snapshot = self.snapshot();
                let active_buf = snapshot
                    .buffers
                    .iter()
                    .find(|b| b.is_active)
                    .ok_or(CoreCommandError::OperationFailed { reason_code: 0 })?;
                let buf_id = active_buf.id;

                let is_vfs = self.document_coordinator.borrow().is_vfs_buffer(buf_id);
                if is_vfs && snapshot.dirty {
                    debug_log!(
                        "[DEBUG] apply_intent: :xit on dirty VFS buffer buf_id={}, initiating save-if-dirty-and-close",
                        buf_id
                    );
                    self.document_coordinator
                        .borrow_mut()
                        .set_deferred_close(buf_id, CoreDeferredClose::SaveIfDirtyAndClose);
                    self.apply_write_intent(String::new(), false, None)
                } else if is_vfs {
                    // clean VFS buffer -> そのまま close
                    let revision = snapshot.revision;
                    self.pending_host_actions
                        .borrow_mut()
                        .push_back(CoreHostAction::Quit {
                            force: false,
                            issued_after_revision: revision,
                        });
                    Ok(CoreCommandOutcome::HostActionQueued)
                } else {
                    // local buffer: 既存フロー -- :xit は Quit として扱う（host が save を管理）
                    let revision = snapshot.revision;
                    self.pending_host_actions
                        .borrow_mut()
                        .push_back(CoreHostAction::Quit {
                            force: false,
                            issued_after_revision: revision,
                        });
                    Ok(CoreCommandOutcome::HostActionQueued)
                }
            }
            ParsedExIntent::Quit { force } => {
                let snapshot = self.snapshot();
                let active_buf = snapshot
                    .buffers
                    .iter()
                    .find(|b| b.is_active)
                    .ok_or(CoreCommandError::OperationFailed { reason_code: 0 })?;
                let buf_id = active_buf.id;

                // :quit! は常に許可
                if force {
                    let revision = snapshot.revision;
                    self.pending_host_actions
                        .borrow_mut()
                        .push_back(CoreHostAction::Quit {
                            force: true,
                            issued_after_revision: revision,
                        });
                    return Ok(CoreCommandOutcome::HostActionQueued);
                }

                // pending save がある VFS buffer では :quit を拒否
                let coordinator = self.document_coordinator.borrow();
                if coordinator.has_pending_save(buf_id) {
                    debug_log!(
                        "[DEBUG] apply_intent: :quit rejected on VFS buffer buf_id={} with pending save",
                        buf_id
                    );
                    drop(coordinator);
                    self.document_coordinator
                        .borrow_mut()
                        .log_quit_denied(buf_id, "pending save blocks quit");
                    return Err(CoreCommandError::OperationFailed { reason_code: 1 });
                }
                drop(coordinator);

                let revision = snapshot.revision;
                self.pending_host_actions
                    .borrow_mut()
                    .push_back(CoreHostAction::Quit {
                        force: false,
                        issued_after_revision: revision,
                    });
                Ok(CoreCommandOutcome::HostActionQueued)
            }
        }
    }

    /// VFS buffer の場合は host save flow に接続し、local buffer は既存フローを維持する。
    fn apply_write_intent(
        &mut self,
        path: String,
        force: bool,
        _deferred_close: Option<CoreDeferredClose>,
    ) -> Result<CoreCommandOutcome, CoreCommandError> {
        let snapshot = self.snapshot();
        let active_buf = snapshot
            .buffers
            .iter()
            .find(|b| b.is_active)
            .ok_or(CoreCommandError::OperationFailed { reason_code: 0 })?;
        let buf_id = active_buf.id;

        let is_vfs = self.document_coordinator.borrow().is_vfs_buffer(buf_id);

        if is_vfs {
            let coordinator = self.document_coordinator.borrow();
            let (document_id, _current_rev) = coordinator
                .buffer_text_snapshot(buf_id)
                .ok_or(CoreCommandError::OperationFailed { reason_code: 0 })?;
            drop(coordinator);

            // バッファテキストを取得
            let text = self.buffer_text(buf_id).unwrap_or_default();

            let target_locator = if path.is_empty() { None } else { Some(path) };

            // revision を更新してから save 発行
            self.document_coordinator
                .borrow_mut()
                .note_buffer_revision(buf_id, snapshot.revision);

            let request = self.document_coordinator.borrow_mut().issue_save(
                buf_id,
                document_id,
                target_locator,
                text,
                force,
            );

            debug_log!(
                "[DEBUG] apply_write_intent: VFS save queued buf_id={} request={:?}",
                buf_id,
                request
            );

            self.pending_host_actions
                .borrow_mut()
                .push_back(CoreHostAction::VfsRequest(request));
            Ok(CoreCommandOutcome::HostActionQueued)
        } else {
            // local buffer: 既存フロー
            let revision = snapshot.revision;
            debug_log!(
                "[DEBUG] apply_write_intent: local write buf_id={} path={} force={} revision={}",
                buf_id,
                path,
                force,
                revision
            );
            self.pending_host_actions
                .borrow_mut()
                .push_back(CoreHostAction::Write {
                    path,
                    force,
                    issued_after_revision: revision,
                });
            Ok(CoreCommandOutcome::HostActionQueued)
        }
    }

    pub fn take_pending_host_action(&mut self) -> Option<CoreHostAction> {
        self.drain_native_host_actions();
        self.pending_host_actions.borrow_mut().pop_front()
    }

    pub fn get_undo_tree(&self, buf_id: i32) -> Result<CoreUndoTree, CoreCommandError> {
        let mut tree = unsafe { std::mem::zeroed() };
        let result =
            unsafe { bindings::vim_bridge_get_undo_tree(self.state.as_ptr(), buf_id, &mut tree) };
        if result != 0 {
            return Err(CoreCommandError::OperationFailed {
                reason_code: result as u32,
            });
        }
        Ok(convert_undo_tree(tree))
    }

    pub fn undo_jump(&mut self, buf_id: i32, seq: i32) -> Result<(), CoreCommandError> {
        let result = unsafe {
            bindings::vim_bridge_undo_jump(
                self.state.as_ptr(),
                buf_id,
                seq as ::std::os::raw::c_long,
            )
        };
        if result != 0 {
            return Err(CoreCommandError::OperationFailed {
                reason_code: result as u32,
            });
        }
        Ok(())
    }

    pub fn backend_identity(&self) -> CoreBackendIdentity {
        let identity = unsafe { bindings::vim_bridge_backend_identity(self.state.as_ptr()) };
        match identity {
            value
                if value
                    == bindings::vim_runtime_backend_identity_VIM_CORE_BACKEND_IDENTITY_UPSTREAM_RUNTIME =>
            {
                CoreBackendIdentity::UpstreamRuntime
            }
            value if value == bindings::vim_runtime_backend_identity_VIM_CORE_BACKEND_IDENTITY_BRIDGE_STUB => {
                CoreBackendIdentity::BridgeStub
            }
            _ => CoreBackendIdentity::BridgeStub,
        }
    }

    pub fn get_syntax_name(&self, syn_id: i32) -> Option<String> {
        let name_ptr = unsafe { bindings::vim_bridge_get_syntax_name(self.state.as_ptr(), syn_id) };
        if name_ptr.is_null() {
            return None;
        }
        let c_str = unsafe { std::ffi::CStr::from_ptr(name_ptr) };
        let s = c_str.to_string_lossy().into_owned();
        if s.is_empty() { None } else { Some(s) }
    }

    pub fn get_line_syntax(
        &self,
        win_id: i32,
        lnum: i64,
    ) -> Result<Vec<CoreSyntaxChunk>, CoreCommandError> {
        let mut out_ids = vec![0i32; 1024]; // allocate enough space for a line
        let cols = unsafe {
            bindings::vim_bridge_get_line_syntax(
                self.state.as_ptr(),
                win_id,
                lnum as std::os::raw::c_long,
                out_ids.as_mut_ptr(),
                out_ids.len() as std::os::raw::c_int,
            )
        };

        if cols < 0 {
            return Err(CoreCommandError::OperationFailed {
                reason_code: cols as u32,
            });
        }

        let mut chunks = Vec::new();
        if cols == 0 {
            return Ok(chunks);
        }

        let mut current_id = out_ids[0];
        let mut start_col = 0;

        for (i, syn_id) in out_ids
            .iter()
            .copied()
            .enumerate()
            .take(cols as usize)
            .skip(1)
        {
            if syn_id != current_id {
                chunks.push(CoreSyntaxChunk {
                    start_col,
                    end_col: i,
                    syn_id: current_id,
                    name: self.get_syntax_name(current_id),
                });
                current_id = syn_id;
                start_col = i;
            }
        }

        chunks.push(CoreSyntaxChunk {
            start_col,
            end_col: cols as usize,
            syn_id: current_id,
            name: self.get_syntax_name(current_id),
        });

        Ok(chunks)
    }

    /// メッセージイベントを受信するハンドラを登録する
    pub fn set_message_handler(&mut self, handler: MessageHandler) {
        /* println debug removed */
        // ハンドラ登録前に蓄積されたメッセージ履歴と v:errmsg をクリアする
        let _ = self.eval_string("execute('messages clear')");
        let _ = self.eval_string("execute('let v:errmsg = \"\"')");
        /* println debug removed */
        self.message_handler = Some(handler);
    }

    /// Vimscript式を評価し、結果を文字列として返す
    pub fn eval_string(&mut self, expr: &str) -> Option<String> {
        /* println debug removed */
        let expr_c = CString::new(expr).ok()?;
        let ptr = unsafe { bindings::vim_bridge_eval_string(self.state.as_ptr(), expr_c.as_ptr()) };
        if ptr.is_null() {
            /* println debug removed */
            return None;
        }
        let len = unsafe { std::ffi::CStr::from_ptr(ptr).to_bytes().len() };
        let s = string_from_parts(ptr, len);
        unsafe { bindings::vim_bridge_free_string(ptr) };
        /* println debug removed */
        Some(s)
    }

    /// 内部利用: コマンド実行後にメッセージ履歴をポーリングし、ハンドラへディスパッチする
    pub(crate) fn poll_and_dispatch_messages(&mut self) {
        // ハンドラが未登録の場合はポーリング自体をスキップする
        if self.message_handler.is_none() {
            /* println debug removed */
            return;
        }

        // v:errmsg を取得してエラーメッセージの検出に使用する
        let errmsg = self.eval_string("v:errmsg").unwrap_or_default();
        debug_log!("[DEBUG] poll_and_dispatch_messages: v:errmsg={}", errmsg);

        // execute('messages') でメッセージ履歴を取得
        let messages_output = self.eval_string("execute('messages')");
        debug_log!(
            "[DEBUG] poll_and_dispatch_messages: messages_output={:?}",
            messages_output
        );

        // 履歴をクリア
        let _ = self.eval_string("execute('messages clear')");
        // v:errmsg もクリア
        let _ = self.eval_string("execute('let v:errmsg = \"\"')");

        // ハンドラが未登録なら終了（再チェック）
        let handler = match &mut self.message_handler {
            Some(h) => h,
            None => return,
        };

        // 出力が空なら終了
        let output = match messages_output {
            Some(ref s) if !s.trim().is_empty() => s.clone(),
            _ => return,
        };

        // 行ごとに分割してパース
        // エラー判定: E[0-9]+: パターン、または v:errmsg に含まれるメッセージ
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let kind =
                if is_error_message(trimmed) || (!errmsg.is_empty() && trimmed.contains(&errmsg)) {
                    CoreMessageKind::Error
                } else {
                    CoreMessageKind::Normal
                };

            debug_log!(
                "[DEBUG] poll_and_dispatch_messages: dispatching kind={:?} content={}",
                kind,
                trimmed
            );

            handler(CoreMessageEvent {
                kind,
                content: trimmed.to_string(),
            });
        }
    }

    pub fn register(&self, regname: char) -> Option<String> {
        let regname_c = regname as std::os::raw::c_char;
        let ptr = unsafe { bindings::vim_bridge_get_register(self.state.as_ptr(), regname_c) };
        if ptr.is_null() {
            return None;
        }

        let len = unsafe { std::ffi::CStr::from_ptr(ptr).to_bytes().len() };
        let s = string_from_parts(ptr, len);

        unsafe { bindings::vim_bridge_free_string(ptr) };
        Some(s)
    }

    pub fn set_screen_size(&mut self, rows: i32, cols: i32) {
        unsafe {
            bindings::vim_bridge_set_screen_size(self.state.as_ptr(), rows, cols);
        }
    }

    pub fn buffers(&self) -> Vec<CoreBufferInfo> {
        self.snapshot().buffers
    }

    pub fn windows(&self) -> Vec<CoreWindowInfo> {
        self.snapshot().windows
    }

    pub fn buffer_binding(&self, buf_id: i32) -> Option<CoreBufferBinding> {
        let _ = self.snapshot();
        self.document_coordinator.borrow().binding(buf_id).cloned()
    }

    pub fn vfs_request_ledger(&self) -> Vec<CoreRequestEntry> {
        self.document_coordinator.borrow().ledger_entries()
    }

    pub fn vfs_transaction_log(&self) -> Vec<VfsLogEntry> {
        self.document_coordinator
            .borrow()
            .transaction_log()
            .to_vec()
    }

    pub fn switch_to_buffer(&mut self, buf_id: i32) -> Result<(), CoreCommandError> {
        let status = unsafe { bindings::vim_bridge_switch_to_buffer(self.state.as_ptr(), buf_id) };
        convert_status(status)
    }

    pub fn switch_to_window(&mut self, win_id: i32) -> Result<(), CoreCommandError> {
        let status = unsafe { bindings::vim_bridge_switch_to_window(self.state.as_ptr(), win_id) };
        convert_status(status)
    }

    pub fn buffer_text(&self, buf_id: i32) -> Option<String> {
        let ptr = unsafe { bindings::vim_bridge_get_buffer_text(self.state.as_ptr(), buf_id) };
        if ptr.is_null() {
            return None;
        }
        let len = unsafe { std::ffi::CStr::from_ptr(ptr).to_bytes().len() };
        let s = string_from_parts(ptr, len);
        unsafe { bindings::vim_bridge_free_string(ptr) };
        Some(s)
    }

    /// VFS load/save の結果を target buffer に反映する runtime apply contract。
    /// vim_bridge_apply_buffer_commit を使用して text, name, dirty を一括で反映する。
    fn apply_loaded_buffer(
        &mut self,
        buf_id: i32,
        display_name: &str,
        text: &str,
    ) -> Result<(), CoreCommandError> {
        debug_log!(
            "[DEBUG] apply_loaded_buffer: buf_id={} display_name={} text_len={}",
            buf_id,
            display_name,
            text.len()
        );
        let commit = bindings::vim_core_buffer_commit_t {
            target_buf_id: buf_id,
            replace_text: true,
            text_ptr: text.as_ptr().cast(),
            text_len: text.len(),
            display_name_ptr: display_name.as_ptr().cast(),
            display_name_len: display_name.len(),
            clear_dirty: true,
        };
        let status =
            unsafe { bindings::vim_bridge_apply_buffer_commit(self.state.as_ptr(), &commit) };
        convert_status(status)
    }

    fn drain_native_host_actions(&mut self) {
        loop {
            let action =
                unsafe { bindings::vim_bridge_take_pending_host_action(self.state.as_ptr()) };
            let Some(action) = convert_host_action(action) else {
                break;
            };
            self.pending_host_actions.borrow_mut().push_back(action);
        }
    }

    pub fn set_register(&mut self, regname: char, text: &str) {
        let regname_c = regname as std::os::raw::c_char;
        unsafe {
            bindings::vim_bridge_set_register(
                self.state.as_ptr(),
                regname_c,
                text.as_ptr().cast(),
                text.len(),
            )
        }
    }

    pub fn get_option_number(
        &self,
        name: &str,
        scope: CoreOptionScope,
    ) -> Result<i64, CoreOptionError> {
        match self.get_option_value(name, scope, CoreOptionType::Number)? {
            ConvertedOptionValue::Number(value) => Ok(value),
            other => Err(CoreOptionError::InternalError {
                name: name.to_string(),
                detail: format!(
                    "number option getter received unexpected converted value: {:?}",
                    other
                ),
            }),
        }
    }

    pub fn get_option_bool(
        &self,
        name: &str,
        scope: CoreOptionScope,
    ) -> Result<bool, CoreOptionError> {
        match self.get_option_value(name, scope, CoreOptionType::Bool)? {
            ConvertedOptionValue::Bool(value) => Ok(value),
            other => Err(CoreOptionError::InternalError {
                name: name.to_string(),
                detail: format!(
                    "bool option getter received unexpected converted value: {:?}",
                    other
                ),
            }),
        }
    }

    pub fn get_option_string(
        &self,
        name: &str,
        scope: CoreOptionScope,
    ) -> Result<String, CoreOptionError> {
        match self.get_option_value(name, scope, CoreOptionType::String)? {
            ConvertedOptionValue::String(value) => Ok(value),
            other => Err(CoreOptionError::InternalError {
                name: name.to_string(),
                detail: format!(
                    "string option getter received unexpected converted value: {:?}",
                    other
                ),
            }),
        }
    }

    pub fn set_option_number(
        &mut self,
        name: &str,
        value: i64,
        scope: CoreOptionScope,
    ) -> Result<(), CoreOptionError> {
        debug_log!(
            "[DEBUG] VimCoreSession::set_option_number: name={} value={} scope={:?}",
            name,
            value,
            scope
        );
        let name_c = option_name_to_cstring(name)?;
        let result = unsafe {
            bindings::vim_bridge_set_option_number(
                self.state.as_ptr(),
                name_c.as_ptr(),
                value,
                convert_option_scope(scope),
            )
        };

        convert_option_set_result(name, result)
    }

    pub fn set_option_bool(
        &mut self,
        name: &str,
        value: bool,
        scope: CoreOptionScope,
    ) -> Result<(), CoreOptionError> {
        debug_log!(
            "[DEBUG] VimCoreSession::set_option_bool: name={} value={} scope={:?}",
            name,
            value,
            scope
        );
        self.set_option_number(name, i64::from(value), scope)
    }

    pub fn set_option_string(
        &mut self,
        name: &str,
        value: &str,
        scope: CoreOptionScope,
    ) -> Result<(), CoreOptionError> {
        debug_log!(
            "[DEBUG] VimCoreSession::set_option_string: name={} value={} scope={:?}",
            name,
            value,
            scope
        );
        let name_c = option_name_to_cstring(name)?;
        let value_c = option_value_to_cstring(name, value)?;
        let result = unsafe {
            bindings::vim_bridge_set_option_string(
                self.state.as_ptr(),
                name_c.as_ptr(),
                value_c.as_ptr(),
                convert_option_scope(scope),
            )
        };

        convert_option_set_result(name, result)
    }

    fn get_option_value(
        &self,
        name: &str,
        scope: CoreOptionScope,
        expected: CoreOptionType,
    ) -> Result<ConvertedOptionValue, CoreOptionError> {
        debug_log!(
            "[DEBUG] VimCoreSession::get_option_value: name={} scope={:?} expected={:?}",
            name,
            scope,
            expected
        );
        let name_c = option_name_to_cstring(name)?;
        let result = unsafe {
            bindings::vim_bridge_get_option(
                self.state.as_ptr(),
                name_c.as_ptr(),
                convert_option_scope(scope),
            )
        };

        convert_option_get_result(name, scope, expected, result)
    }

    pub fn get_search_pattern(&self) -> Option<String> {
        unsafe {
            let ptr = bindings::vim_bridge_get_search_pattern();
            if ptr.is_null() {
                None
            } else {
                let c_str = std::ffi::CStr::from_ptr(ptr);
                let s = c_str.to_string_lossy().into_owned();
                if s.is_empty() { None } else { Some(s) }
            }
        }
    }

    pub fn is_hlsearch_active(&self) -> bool {
        unsafe { bindings::vim_bridge_is_hlsearch_active() != 0 }
    }

    pub fn get_search_direction(&self) -> CoreSearchDirection {
        let dir = unsafe { bindings::vim_bridge_get_search_direction() };
        if dir == 1 {
            CoreSearchDirection::Forward
        } else {
            CoreSearchDirection::Backward
        }
    }

    pub fn get_search_highlights(
        &self,
        window_id: i32,
        start_row: i32,
        end_row: i32,
    ) -> Vec<CoreMatchRange> {
        unsafe {
            let list = bindings::vim_bridge_get_search_highlights(window_id, start_row, end_row);
            let mut result = Vec::new();
            if !list.ranges.is_null() && list.count > 0 {
                let slice = std::slice::from_raw_parts(list.ranges, list.count as usize);
                for range in slice {
                    result.push(CoreMatchRange {
                        start_row: range.start_row as usize,
                        start_col: range.start_col as usize,
                        end_row: range.end_row as usize,
                        end_col: range.end_col as usize,
                        match_type: CoreMatchType::from(range.match_type),
                    });
                }
            }
            bindings::vim_bridge_free_match_list(list);
            result
        }
    }

    pub fn get_cursor_match_info(
        &self,
        window_id: i32,
        row: i32,
        col: i32,
        max_count: i32,
        timeout_ms: i32,
    ) -> CoreCursorMatchInfo {
        unsafe {
            let info = bindings::vim_bridge_get_cursor_match_info(
                window_id, row, col, max_count, timeout_ms,
            );
            let total_matches = match info.status {
                bindings::vim_core_match_count_status_t_VIM_CORE_MATCH_COUNT_MAX_REACHED => {
                    MatchCountResult::MaxReached(info.total_matches as usize)
                }
                bindings::vim_core_match_count_status_t_VIM_CORE_MATCH_COUNT_TIMED_OUT => {
                    MatchCountResult::TimedOut
                }
                _ => MatchCountResult::Calculated(info.total_matches as usize),
            };

            CoreCursorMatchInfo {
                is_on_match: info.is_on_match != 0,
                current_match_index: info.current_match_index as usize,
                total_matches,
            }
        }
    }

    pub fn is_incsearch_active(&self) -> bool {
        unsafe { bindings::vim_bridge_is_incsearch_active() != 0 }
    }

    pub fn get_incsearch_pattern(&self) -> Option<String> {
        unsafe {
            let ptr = bindings::vim_bridge_get_incsearch_pattern();
            if ptr.is_null() {
                None
            } else {
                let c_str = std::ffi::CStr::from_ptr(ptr);
                let s = c_str.to_string_lossy().into_owned();
                if s.is_empty() { None } else { Some(s) }
            }
        }
    }
}

impl Drop for VimCoreSession {
    fn drop(&mut self) {
        unsafe {
            bindings::vim_bridge_state_free(self.state.as_ptr());
        }
        crate::vfd::get_manager().clear_all();
        ACTIVE_SESSION.store(false, Ordering::Release);
    }
}

fn convert_command_result(
    result: bindings::vim_core_command_result_t,
) -> Result<CoreCommandOutcome, CoreCommandError> {
    match result.status {
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_OK => Ok(match result.outcome {
            outcome if outcome == bindings::vim_core_command_outcome_kind_VIM_CORE_COMMAND_OUTCOME_NO_CHANGE => {
                CoreCommandOutcome::NoChange
            }
            outcome
                if outcome
                    == bindings::vim_core_command_outcome_kind_VIM_CORE_COMMAND_OUTCOME_BUFFER_CHANGED =>
            {
                CoreCommandOutcome::BufferChanged {
                    revision: result.snapshot.revision,
                }
            }
            outcome
                if outcome
                    == bindings::vim_core_command_outcome_kind_VIM_CORE_COMMAND_OUTCOME_CURSOR_CHANGED =>
            {
                CoreCommandOutcome::CursorChanged {
                    row: result.snapshot.cursor_row,
                    col: result.snapshot.cursor_col,
                }
            }
            outcome
                if outcome
                    == bindings::vim_core_command_outcome_kind_VIM_CORE_COMMAND_OUTCOME_MODE_CHANGED =>
            {
                CoreCommandOutcome::ModeChanged {
                    mode: convert_mode(result.snapshot.mode),
                }
            }
            outcome
                if outcome
                    == bindings::vim_core_command_outcome_kind_VIM_CORE_COMMAND_OUTCOME_HOST_ACTION_QUEUED =>
            {
                CoreCommandOutcome::HostActionQueued
            }
            _ => CoreCommandOutcome::NoChange,
        }),
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_COMMAND_ERROR => {
            Err(CoreCommandError::OperationFailed {
                reason_code: result.reason_code,
            })
        }
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_SESSION_ERROR => {
            Err(CoreCommandError::OperationFailed {
                reason_code: result.reason_code,
            })
        }
        status => Err(CoreCommandError::UnknownStatus {
            status,
            reason_code: result.reason_code,
        }),
    }
}

fn option_name_to_cstring(name: &str) -> Result<CString, CoreOptionError> {
    CString::new(name).map_err(|_| CoreOptionError::InternalError {
        name: name.to_string(),
        detail: "option name contains interior NUL byte".to_string(),
    })
}

fn option_value_to_cstring(name: &str, value: &str) -> Result<CString, CoreOptionError> {
    CString::new(value).map_err(|_| CoreOptionError::InternalError {
        name: name.to_string(),
        detail: "option value contains interior NUL byte".to_string(),
    })
}

fn convert_status(status: bindings::vim_core_status_t) -> Result<(), CoreCommandError> {
    match status {
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_OK => Ok(()),
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_COMMAND_ERROR => {
            Err(CoreCommandError::InvalidInput)
        }
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_SESSION_ERROR => {
            Err(CoreCommandError::OperationFailed { reason_code: 0 })
        }
        status => Err(CoreCommandError::UnknownStatus {
            status,
            reason_code: 0,
        }),
    }
}

fn convert_snapshot(snapshot: bindings::vim_core_snapshot_t) -> CoreSnapshot {
    let buffers = convert_buffer_list(snapshot.buffers, snapshot.buffer_count);
    let windows = convert_window_list(snapshot.windows, snapshot.window_count);

    // Free the C-allocated arrays (the data has been copied into Rust Vecs)
    if !snapshot.buffers.is_null() {
        unsafe { libc_free(snapshot.buffers.cast()) };
    }
    if !snapshot.windows.is_null() {
        unsafe { libc_free(snapshot.windows.cast()) };
    }

    // ポップアップメニュー情報の変換とメモリ解放
    let pum = convert_pum_info(snapshot.pum);

    CoreSnapshot {
        text: string_from_parts(snapshot.text_ptr, snapshot.text_len),
        revision: snapshot.revision,
        dirty: snapshot.dirty,
        mode: convert_mode(snapshot.mode),
        pending_input: convert_pending_input(snapshot.pending_input),
        cursor_row: snapshot.cursor_row,
        cursor_col: snapshot.cursor_col,
        pending_host_actions: snapshot.pending_host_actions,
        buffers,
        windows,
        pum,
    }
}

/// C側のポップアップメニュー情報をRust型に変換し、C側メモリを解放する
fn convert_pum_info(pum_ptr: *mut bindings::vim_core_pum_info_t) -> Option<CorePumInfo> {
    if pum_ptr.is_null() {
        return None;
    }

    let pum = unsafe { &*pum_ptr };

    debug_log!(
        "[DEBUG] convert_pum_info: row={} col={} width={} height={} selected={} item_count={}",
        pum.row,
        pum.col,
        pum.width,
        pum.height,
        pum.selected_index,
        pum.item_count
    );

    // 候補配列を走査し、各候補のC文字列をRustのStringに変換
    let items = if !pum.items.is_null() && pum.item_count > 0 {
        let slice = unsafe { std::slice::from_raw_parts(pum.items, pum.item_count) };
        slice
            .iter()
            .map(|item| CorePumItem {
                word: c_str_to_string(item.word),
                abbr: c_str_to_string(item.abbr),
                menu: c_str_to_string(item.menu),
                kind: c_str_to_string(item.kind),
                info: c_str_to_string(item.info),
            })
            .collect()
    } else {
        Vec::new()
    };

    // 未選択状態 (selected_index == -1) は None にマッピング
    let selected_index = if pum.selected_index < 0 {
        None
    } else {
        Some(pum.selected_index as usize)
    };

    let result = CorePumInfo {
        row: pum.row,
        col: pum.col,
        width: pum.width,
        height: pum.height,
        selected_index,
        items,
    };

    // C側メモリを専用解放関数で解放
    unsafe {
        bindings::vim_bridge_free_pum_info(pum_ptr);
    }

    debug_log!(
        "[DEBUG] convert_pum_info: conversion complete, {} items, selected={:?}",
        result.items.len(),
        result.selected_index
    );

    Some(result)
}

/// NULLセーフなCストリング→Rust String変換
fn c_str_to_string(ptr: *const ::std::os::raw::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned() }
}

unsafe extern "C" {
    fn free(ptr: *mut std::ffi::c_void);
}

unsafe fn libc_free(ptr: *mut std::ffi::c_void) {
    unsafe { free(ptr) }
}

fn convert_buffer_list(
    ptr: *mut bindings::vim_core_buffer_info_t,
    count: usize,
) -> Vec<CoreBufferInfo> {
    if ptr.is_null() || count == 0 {
        return Vec::new();
    }

    let slice = unsafe { slice::from_raw_parts(ptr, count) };
    slice
        .iter()
        .map(|info| CoreBufferInfo {
            id: info.id,
            name: string_from_parts(info.name_ptr, info.name_len),
            dirty: info.dirty,
            is_active: info.is_active,
            source_kind: CoreBufferSourceKind::Local,
            document_id: None,
            pending_vfs_operation: None,
            deferred_close: None,
            last_vfs_error: None,
        })
        .collect()
}

fn convert_window_list(
    ptr: *mut bindings::vim_core_window_info_t,
    count: usize,
) -> Vec<CoreWindowInfo> {
    if ptr.is_null() || count == 0 {
        return Vec::new();
    }

    let slice = unsafe { slice::from_raw_parts(ptr, count) };
    slice
        .iter()
        .map(|info| CoreWindowInfo {
            id: info.id,
            buf_id: info.buf_id,
            row: info.row,
            col: info.col,
            width: info.width,
            height: info.height,
            topline: info.topline,
            botline: info.botline,
            leftcol: info.leftcol,
            skipcol: info.skipcol,
            is_active: info.is_active,
        })
        .collect()
}

fn convert_mode(mode: bindings::vim_core_mode_t) -> CoreMode {
    match mode {
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_INSERT => CoreMode::Insert,
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_VISUAL => CoreMode::Visual,
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_VISUAL_LINE => CoreMode::VisualLine,
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_VISUAL_BLOCK => {
            CoreMode::VisualBlock
        }
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_REPLACE => CoreMode::Replace,
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_SELECT => CoreMode::Select,
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_SELECT_LINE => CoreMode::SelectLine,
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_SELECT_BLOCK => {
            CoreMode::SelectBlock
        }
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_COMMAND_LINE => {
            CoreMode::CommandLine
        }
        value if value == bindings::vim_core_mode_VIM_CORE_MODE_OPERATOR_PENDING => {
            CoreMode::OperatorPending
        }
        _ => CoreMode::Normal,
    }
}

fn convert_pending_input(pending_input: bindings::vim_core_pending_input_t) -> CorePendingInput {
    match pending_input {
        value if value == bindings::vim_core_pending_input_VIM_CORE_PENDING_INPUT_CHAR => {
            CorePendingInput::Char
        }
        value if value == bindings::vim_core_pending_input_VIM_CORE_PENDING_INPUT_REPLACE => {
            CorePendingInput::Replace
        }
        value if value == bindings::vim_core_pending_input_VIM_CORE_PENDING_INPUT_MARK_SET => {
            CorePendingInput::MarkSet
        }
        value if value == bindings::vim_core_pending_input_VIM_CORE_PENDING_INPUT_MARK_JUMP => {
            CorePendingInput::MarkJump
        }
        value if value == bindings::vim_core_pending_input_VIM_CORE_PENDING_INPUT_REGISTER => {
            CorePendingInput::Register
        }
        _ => CorePendingInput::None,
    }
}

fn convert_mark_position(mark: bindings::vim_core_mark_position_t) -> CoreMarkPosition {
    CoreMarkPosition {
        buf_id: mark.buf_id,
        row: mark.row,
        col: mark.col,
    }
}

fn convert_jumplist(jumplist: bindings::vim_core_jumplist_t) -> CoreJumpList {
    let entries = convert_jumplist_entries(jumplist.entries, jumplist.entry_count);

    unsafe {
        bindings::vim_bridge_free_jumplist(jumplist);
    }

    CoreJumpList {
        current_index: if jumplist.has_current_index {
            jumplist.current_index
        } else {
            0
        },
        entries,
    }
}

fn convert_jumplist_entries(
    ptr: *mut bindings::vim_core_jumplist_entry_t,
    count: usize,
) -> Vec<CoreJumpListEntry> {
    if ptr.is_null() || count == 0 {
        return Vec::new();
    }

    let slice = unsafe { slice::from_raw_parts(ptr, count) };
    slice
        .iter()
        .map(|entry| CoreJumpListEntry {
            buf_id: entry.buf_id,
            row: entry.row,
            col: entry.col,
        })
        .collect()
}

fn convert_undo_tree(tree: bindings::vim_core_undo_tree_t) -> CoreUndoTree {
    let mut nodes = Vec::new();
    if !tree.nodes.is_null() && tree.length > 0 {
        let slice = unsafe { slice::from_raw_parts(tree.nodes, tree.length) };
        for node in slice {
            nodes.push(CoreUndoNode {
                seq: node.seq as i32,
                time: node.time,
                save_nr: node.save_nr as i32,
                prev_seq: if node.prev_seq > 0 {
                    Some(node.prev_seq as i32)
                } else {
                    None
                },
                next_seq: if node.next_seq > 0 {
                    Some(node.next_seq as i32)
                } else {
                    None
                },
                alt_next_seq: if node.alt_next_seq > 0 {
                    Some(node.alt_next_seq as i32)
                } else {
                    None
                },
                alt_prev_seq: if node.alt_prev_seq > 0 {
                    Some(node.alt_prev_seq as i32)
                } else {
                    None
                },
                is_newhead: node.is_newhead,
                is_curhead: node.is_curhead,
            });
        }
    }

    let result = CoreUndoTree {
        nodes,
        synced: tree.synced,
        seq_last: tree.seq_last as i32,
        save_last: tree.save_last as i32,
        seq_cur: tree.seq_cur as i32,
        time_cur: tree.time_cur,
        save_cur: tree.save_cur as i32,
    };

    unsafe {
        bindings::vim_bridge_free_undo_tree(tree);
    }

    result
}

fn parse_ex_intent(command: &str) -> Option<ParsedExIntent> {
    let trimmed = command.trim();
    let trimmed = trimmed.strip_prefix(':').unwrap_or(trimmed).trim();
    if trimmed.is_empty() || trimmed.contains('|') {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let head = parts.next()?;
    let bang = head.ends_with('!');
    let command_name = head.trim_end_matches('!');
    let tail = parts.collect::<Vec<_>>().join(" ");

    match command_name {
        "edit" | "e" if !tail.is_empty() => Some(ParsedExIntent::Edit { locator: tail }),
        "write" | "w" => Some(ParsedExIntent::Write {
            path: tail,
            force: bang,
        }),
        "update" | "up" => Some(ParsedExIntent::Update {
            path: tail,
            force: bang,
        }),
        "wq" | "wqall" => Some(ParsedExIntent::SaveAndClose { force: bang }),
        "exit" | "xit" | "x" | "xall" => Some(ParsedExIntent::SaveIfDirtyAndClose),
        "quit" | "q" | "quitall" | "qall" | "qa" => Some(ParsedExIntent::Quit { force: bang }),
        _ => None,
    }
}

fn convert_host_action(action: bindings::vim_host_action_t) -> Option<CoreHostAction> {
    match action.kind {
        value if value == bindings::VIM_HOST_ACTION_NONE => None,
        value if value == bindings::VIM_HOST_ACTION_WRITE => Some(CoreHostAction::Write {
            path: string_from_parts(action.primary_text_ptr, action.primary_text_len),
            force: action.force,
            issued_after_revision: action.issued_after_revision,
        }),
        value if value == bindings::VIM_HOST_ACTION_QUIT => Some(CoreHostAction::Quit {
            force: action.force,
            issued_after_revision: action.issued_after_revision,
        }),
        value if value == bindings::VIM_HOST_ACTION_REDRAW => Some(CoreHostAction::Redraw {
            full: true,
            clear_before_draw: action.redraw_force,
        }),
        value if value == bindings::VIM_HOST_ACTION_REQUEST_INPUT => {
            Some(CoreHostAction::RequestInput {
                prompt: string_from_parts(action.primary_text_ptr, action.primary_text_len),
                input_kind: convert_input_kind(action.input_kind),
                correlation_id: action.correlation_id,
            })
        }
        value if value == bindings::VIM_HOST_ACTION_BELL => Some(CoreHostAction::Bell),
        value if value == bindings::VIM_HOST_ACTION_BUF_ADD => Some(CoreHostAction::BufAdd {
            buf_id: action.event_buf_id,
        }),
        value if value == bindings::VIM_HOST_ACTION_WIN_NEW => Some(CoreHostAction::WinNew {
            win_id: action.event_win_id,
        }),
        value if value == bindings::VIM_HOST_ACTION_LAYOUT_CHANGED => {
            Some(CoreHostAction::LayoutChanged)
        }
        value if value == bindings::VIM_HOST_ACTION_JOB_START => {
            let req = action.job_start_request;
            crate::vfd::get_manager().register_job(
                req.job_id,
                req.vfd_in,
                req.vfd_out,
                req.vfd_err,
            );
            let mut argv = Vec::new();
            if !req.argv_buf.is_null() && req.argv_len > 0 {
                let slice =
                    unsafe { std::slice::from_raw_parts(req.argv_buf as *const u8, req.argv_len) };
                for arg_slice in slice.split(|&b| b == 0) {
                    if !arg_slice.is_empty()
                        && let Ok(s) = std::str::from_utf8(arg_slice)
                    {
                        argv.push(s.to_owned());
                    }
                }
                unsafe { bindings::vim_bridge_free_string(req.argv_buf) };
            }
            let cwd = if !req.cwd.is_null() {
                let s = unsafe { std::ffi::CStr::from_ptr(req.cwd) }
                    .to_string_lossy()
                    .into_owned();
                unsafe { bindings::vim_bridge_free_string(req.cwd) };
                Some(s)
            } else {
                None
            };
            Some(CoreHostAction::JobStart(CoreJobStartRequest {
                job_id: req.job_id,
                argv,
                cwd,
                vfd_in: req.vfd_in,
                vfd_out: req.vfd_out,
                vfd_err: req.vfd_err,
            }))
        }
        value if value == bindings::VIM_HOST_ACTION_JOB_STOP => Some(CoreHostAction::JobStop {
            job_id: action.job_start_request.job_id,
        }),
        _ => None,
    }
}

fn convert_input_kind(kind: bindings::vim_core_input_request_kind_t) -> CoreInputRequestKind {
    match kind {
        value
            if value
                == bindings::vim_core_input_request_kind_VIM_CORE_INPUT_REQUEST_CONFIRMATION =>
        {
            CoreInputRequestKind::Confirmation
        }
        value if value == bindings::vim_core_input_request_kind_VIM_CORE_INPUT_REQUEST_SECRET => {
            CoreInputRequestKind::Secret
        }
        _ => CoreInputRequestKind::CommandLine,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConvertedOptionValue {
    Bool(bool),
    Number(i64),
    String(String),
}

fn convert_option_scope(scope: CoreOptionScope) -> bindings::vim_core_option_scope_t {
    match scope {
        CoreOptionScope::Default => bindings::vim_core_option_scope_VIM_CORE_OPTION_SCOPE_DEFAULT,
        CoreOptionScope::Global => bindings::vim_core_option_scope_VIM_CORE_OPTION_SCOPE_GLOBAL,
        CoreOptionScope::Local => bindings::vim_core_option_scope_VIM_CORE_OPTION_SCOPE_LOCAL,
    }
}

fn convert_option_type(option_type: bindings::vim_core_option_type_t) -> Option<CoreOptionType> {
    match option_type {
        value if value == bindings::vim_core_option_type_VIM_CORE_OPTION_TYPE_BOOL => {
            Some(CoreOptionType::Bool)
        }
        value if value == bindings::vim_core_option_type_VIM_CORE_OPTION_TYPE_NUMBER => {
            Some(CoreOptionType::Number)
        }
        value if value == bindings::vim_core_option_type_VIM_CORE_OPTION_TYPE_STRING => {
            Some(CoreOptionType::String)
        }
        value if value == bindings::vim_core_option_type_VIM_CORE_OPTION_TYPE_UNKNOWN => None,
        _ => None,
    }
}

fn convert_option_get_result(
    name: &str,
    scope: CoreOptionScope,
    expected: CoreOptionType,
    result: bindings::vim_core_option_get_result_t,
) -> Result<ConvertedOptionValue, CoreOptionError> {
    let actual = convert_option_type(result.option_type);
    debug_log!(
        "[DEBUG] convert_option_get_result: name={} scope={:?} status={} expected={:?} actual={:?}",
        name,
        scope,
        result.status,
        expected,
        actual
    );

    match result.status {
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_OK => {}
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_COMMAND_ERROR => {
            return match actual {
                None => Err(CoreOptionError::UnknownOption {
                    name: name.to_string(),
                }),
                Some(_) if scope == CoreOptionScope::Local => {
                    Err(CoreOptionError::ScopeNotSupported {
                        name: name.to_string(),
                        scope,
                    })
                }
                Some(actual_type) => Err(CoreOptionError::InternalError {
                    name: name.to_string(),
                    detail: format!(
                        "option command error for {:?} without local scope support path",
                        actual_type
                    ),
                }),
            };
        }
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_SESSION_ERROR => {
            return Err(CoreOptionError::InternalError {
                name: name.to_string(),
                detail: "option get bridge returned session error".to_string(),
            });
        }
        status => {
            return Err(CoreOptionError::InternalError {
                name: name.to_string(),
                detail: format!("unknown option get status: {}", status),
            });
        }
    }

    let Some(actual_type) = actual else {
        return Err(CoreOptionError::InternalError {
            name: name.to_string(),
            detail: "option get succeeded with unknown type".to_string(),
        });
    };

    if actual_type != expected {
        return Err(CoreOptionError::TypeMismatch {
            name: name.to_string(),
            expected,
            actual: actual_type,
        });
    }

    match actual_type {
        CoreOptionType::Bool => Ok(ConvertedOptionValue::Bool(result.number_value != 0)),
        CoreOptionType::Number => Ok(ConvertedOptionValue::Number(result.number_value)),
        CoreOptionType::String => {
            let value = string_from_parts(result.string_value_ptr, result.string_value_len);
            if !result.string_value_ptr.is_null() {
                unsafe { bindings::vim_bridge_free_string(result.string_value_ptr.cast_mut()) };
            }
            Ok(ConvertedOptionValue::String(value))
        }
    }
}

fn convert_option_set_result(
    name: &str,
    result: bindings::vim_core_option_set_result_t,
) -> Result<(), CoreOptionError> {
    debug_log!(
        "[DEBUG] convert_option_set_result: name={} status={} error_len={}",
        name,
        result.status,
        result.error_message_len
    );

    let error_message = string_from_parts(result.error_message_ptr, result.error_message_len);
    if !result.error_message_ptr.is_null() {
        unsafe { bindings::vim_bridge_free_string(result.error_message_ptr.cast_mut()) };
    }

    match result.status {
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_OK => Ok(()),
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_COMMAND_ERROR => {
            Err(CoreOptionError::SetFailed {
                name: name.to_string(),
                reason: error_message,
            })
        }
        value if value == bindings::vim_core_status_VIM_CORE_STATUS_SESSION_ERROR => {
            Err(CoreOptionError::InternalError {
                name: name.to_string(),
                detail: if error_message.is_empty() {
                    "option set bridge returned session error".to_string()
                } else {
                    error_message
                },
            })
        }
        status => Err(CoreOptionError::InternalError {
            name: name.to_string(),
            detail: format!("unknown option set status: {}", status),
        }),
    }
}

/// エラーメッセージパターン（E[0-9]+: 形式）を判定する
fn is_error_message(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.is_empty() || bytes[0] != b'E' {
        return false;
    }
    let mut i = 1;
    // 少なくとも1桁の数字が必要
    if i >= bytes.len() || !bytes[i].is_ascii_digit() {
        return false;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    // コロンが続くこと
    i < bytes.len() && bytes[i] == b':'
}

fn string_from_parts(ptr: *const ::std::os::raw::c_char, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }

    let bytes = unsafe { slice::from_raw_parts(ptr.cast::<u8>(), len) };
    str::from_utf8(bytes)
        .expect("vim bridge returned non-utf8 text")
        .to_owned()
}

#[cfg(test)]
mod option_conversion_tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn option_get_result_returns_unknown_option_for_unknown_type() {
        let result = bindings::vim_core_option_get_result_t {
            status: bindings::vim_core_status_VIM_CORE_STATUS_COMMAND_ERROR,
            option_type: bindings::vim_core_option_type_VIM_CORE_OPTION_TYPE_UNKNOWN,
            number_value: 0,
            string_value_ptr: std::ptr::null(),
            string_value_len: 0,
        };

        assert_eq!(
            convert_option_get_result(
                "missing",
                CoreOptionScope::Default,
                CoreOptionType::Number,
                result,
            ),
            Err(CoreOptionError::UnknownOption {
                name: "missing".to_string(),
            })
        );
    }

    #[test]
    fn option_get_result_returns_type_mismatch_when_actual_type_differs() {
        let result = bindings::vim_core_option_get_result_t {
            status: bindings::vim_core_status_VIM_CORE_STATUS_OK,
            option_type: bindings::vim_core_option_type_VIM_CORE_OPTION_TYPE_STRING,
            number_value: 0,
            string_value_ptr: std::ptr::null(),
            string_value_len: 0,
        };

        assert_eq!(
            convert_option_get_result(
                "tabstop",
                CoreOptionScope::Default,
                CoreOptionType::Number,
                result,
            ),
            Err(CoreOptionError::TypeMismatch {
                name: "tabstop".to_string(),
                expected: CoreOptionType::Number,
                actual: CoreOptionType::String,
            })
        );
    }

    #[test]
    fn option_get_result_returns_scope_not_supported_for_local_known_option() {
        let result = bindings::vim_core_option_get_result_t {
            status: bindings::vim_core_status_VIM_CORE_STATUS_COMMAND_ERROR,
            option_type: bindings::vim_core_option_type_VIM_CORE_OPTION_TYPE_BOOL,
            number_value: 0,
            string_value_ptr: std::ptr::null(),
            string_value_len: 0,
        };

        assert_eq!(
            convert_option_get_result(
                "number",
                CoreOptionScope::Local,
                CoreOptionType::Bool,
                result,
            ),
            Err(CoreOptionError::ScopeNotSupported {
                name: "number".to_string(),
                scope: CoreOptionScope::Local,
            })
        );
    }

    #[test]
    fn option_get_result_copies_and_returns_string_values() {
        let value = CString::new("rust").expect("cstring");
        let len = value.as_bytes().len();
        let ptr = value.into_raw();
        let result = bindings::vim_core_option_get_result_t {
            status: bindings::vim_core_status_VIM_CORE_STATUS_OK,
            option_type: bindings::vim_core_option_type_VIM_CORE_OPTION_TYPE_STRING,
            number_value: 0,
            string_value_ptr: ptr,
            string_value_len: len,
        };

        assert_eq!(
            convert_option_get_result(
                "filetype",
                CoreOptionScope::Default,
                CoreOptionType::String,
                result,
            ),
            Ok(ConvertedOptionValue::String("rust".to_string()))
        );
    }

    #[test]
    fn option_set_result_returns_set_failed_with_reason() {
        let reason = CString::new("E487").expect("cstring");
        let len = reason.as_bytes().len();
        let ptr = reason.into_raw();
        let result = bindings::vim_core_option_set_result_t {
            status: bindings::vim_core_status_VIM_CORE_STATUS_COMMAND_ERROR,
            error_message_ptr: ptr,
            error_message_len: len,
        };

        assert_eq!(
            convert_option_set_result("tabstop", result),
            Err(CoreOptionError::SetFailed {
                name: "tabstop".to_string(),
                reason: "E487".to_string(),
            })
        );
    }

    #[test]
    fn option_scope_converts_to_ffi_values() {
        assert_eq!(
            convert_option_scope(CoreOptionScope::Default),
            bindings::vim_core_option_scope_VIM_CORE_OPTION_SCOPE_DEFAULT
        );
        assert_eq!(
            convert_option_scope(CoreOptionScope::Global),
            bindings::vim_core_option_scope_VIM_CORE_OPTION_SCOPE_GLOBAL
        );
        assert_eq!(
            convert_option_scope(CoreOptionScope::Local),
            bindings::vim_core_option_scope_VIM_CORE_OPTION_SCOPE_LOCAL
        );
    }
}

#[cfg(test)]
mod undo_conversion_tests {
    use super::*;

    #[test]
    fn convert_undo_tree_handles_empty_tree() {
        let tree = bindings::vim_core_undo_tree_t {
            nodes: std::ptr::null_mut(),
            length: 0,
            synced: true,
            seq_last: 0,
            save_last: 0,
            seq_cur: 0,
            time_cur: 0,
            save_cur: 0,
        };

        let core_tree = convert_undo_tree(tree);
        assert_eq!(core_tree.nodes.len(), 0);
        assert!(core_tree.synced);
        assert_eq!(core_tree.seq_last, 0);
    }

    #[test]
    fn convert_undo_tree_handles_populated_tree() {
        let raw_nodes = [
            bindings::vim_core_undo_node_t {
                seq: 1,
                time: 12345,
                save_nr: 0,
                prev_seq: 0,
                next_seq: 2,
                alt_next_seq: 0,
                alt_prev_seq: 0,
                is_newhead: true,
                is_curhead: false,
            },
            bindings::vim_core_undo_node_t {
                seq: 2,
                time: 12346,
                save_nr: 0,
                prev_seq: 1,
                next_seq: 0,
                alt_next_seq: 0,
                alt_prev_seq: 0,
                is_newhead: false,
                is_curhead: true,
            },
        ];

        unsafe extern "C" {
            fn malloc(size: usize) -> *mut std::ffi::c_void;
        }

        let ptr = unsafe {
            malloc(std::mem::size_of::<bindings::vim_core_undo_node_t>() * 2)
                as *mut bindings::vim_core_undo_node_t
        };
        unsafe {
            std::ptr::copy_nonoverlapping(raw_nodes.as_ptr(), ptr, 2);
        }

        let tree = bindings::vim_core_undo_tree_t {
            nodes: ptr,
            length: 2,
            synced: false,
            seq_last: 2,
            save_last: 0,
            seq_cur: 2,
            time_cur: 12346,
            save_cur: 0,
        };

        let core_tree = convert_undo_tree(tree);
        assert_eq!(core_tree.nodes.len(), 2);
        assert_eq!(core_tree.seq_last, 2);
        assert_eq!(core_tree.seq_cur, 2);

        let node1 = &core_tree.nodes[0];
        assert_eq!(node1.seq, 1);
        assert_eq!(node1.time, 12345);
        assert_eq!(node1.prev_seq, None);
        assert_eq!(node1.next_seq, Some(2));
        assert!(node1.is_newhead);

        let node2 = &core_tree.nodes[1];
        assert_eq!(node2.seq, 2);
        assert_eq!(node2.prev_seq, Some(1));
        assert_eq!(node2.next_seq, None);
        assert!(node2.is_curhead);
    }
}
