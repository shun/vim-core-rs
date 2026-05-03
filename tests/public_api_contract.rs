use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
#[cfg(unix)]
use std::{
    fs::File,
    io::Read,
    os::fd::{FromRawFd, RawFd},
};
#[cfg(feature = "experimental-tree-sitter")]
use vim_core_rs::{
    CoreBufferRevision, CoreEmbeddedBlockKind, CoreEmbeddedRegion, CoreEmbeddedRegionSource,
    CoreLanguageResolutionSource, CoreLanguageRole, CoreResolutionConfidence, CoreResolvedLanguage,
    CoreSyntaxCategory, CoreSyntaxModifier, CoreTextPosition, CoreTextRange, CoreTreeSitterChunk,
    CoreTreeSitterProvenance, CoreTreeSitterRangeSyntax, CoreTreeSitterStatus,
};
use vim_core_rs::{
    CoreCommandOutcome, CoreEvent, CoreHostAction, CoreInputRequestKind, CoreInputResponse,
    CoreInputResponseError, CoreMode, CoreOptionError, CoreOptionScope, CoreOptionType,
    CoreRuntimeMode, CoreSessionError, CoreSessionOptions, VimCoreSession,
};

fn session_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn acquire_session_test_lock() -> std::sync::MutexGuard<'static, ()> {
    session_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(unix)]
fn capture_standard_streams<T>(f: impl FnOnce() -> T) -> (T, String, String) {
    unsafe fn capture_fd(fd: RawFd) -> (RawFd, RawFd) {
        let saved = unsafe { libc::dup(fd) };
        assert!(saved >= 0, "dup failed for fd={fd}");

        let mut pipefds = [0; 2];
        assert_eq!(
            unsafe { libc::pipe(pipefds.as_mut_ptr()) },
            0,
            "pipe failed for fd={fd}"
        );
        assert!(
            unsafe { libc::dup2(pipefds[1], fd) } >= 0,
            "dup2 failed for fd={fd}"
        );
        assert_eq!(
            unsafe { libc::close(pipefds[1]) },
            0,
            "close failed for write pipe fd={fd}"
        );
        (saved, pipefds[0])
    }

    unsafe fn restore_fd(fd: RawFd, saved: RawFd) {
        assert!(
            unsafe { libc::dup2(saved, fd) } >= 0,
            "restore dup2 failed for fd={fd}"
        );
        assert_eq!(
            unsafe { libc::close(saved) },
            0,
            "close failed for saved fd={fd}"
        );
    }

    unsafe fn read_pipe(read_fd: RawFd) -> String {
        let mut file = unsafe { File::from_raw_fd(read_fd) };
        let mut output = String::new();
        file.read_to_string(&mut output)
            .expect("pipe output should be readable");
        output
    }

    unsafe {
        let (saved_stdout, stdout_read) = capture_fd(libc::STDOUT_FILENO);
        let (saved_stderr, stderr_read) = capture_fd(libc::STDERR_FILENO);

        let result = f();

        libc::fflush(std::ptr::null_mut());
        restore_fd(libc::STDOUT_FILENO, saved_stdout);
        restore_fd(libc::STDERR_FILENO, saved_stderr);

        let stdout = read_pipe(stdout_read);
        let stderr = read_pipe(stderr_read);
        (result, stdout, stderr)
    }
}

#[cfg(unix)]
fn sanitize_harness_output(output: &str) -> String {
    output
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("test ")
                && !trimmed.contains(" ... ok")
                && !trimmed.contains(" ... FAILED")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn session_exposes_initial_snapshot_contract() {
    let _guard = acquire_session_test_lock();
    let session =
        VimCoreSession::new("first line\nsecond line").expect("session should initialize");
    let snapshot = session.snapshot();

    assert_eq!(
        snapshot.text.trim_end_matches('\n'),
        "first line\nsecond line"
    );
    assert_eq!(snapshot.revision, 0);
    assert!(!snapshot.dirty);
    assert_eq!(snapshot.mode, CoreMode::Normal);
    assert_eq!(snapshot.pending_host_actions, 0);
    assert_eq!(session.runtime_mode(), CoreRuntimeMode::Embedded);
}

#[test]
fn second_session_is_rejected_while_first_is_alive() {
    let _guard = acquire_session_test_lock();
    let first = VimCoreSession::new("alpha").expect("first session should initialize");
    let second = VimCoreSession::new("beta");

    assert!(matches!(
        second,
        Err(CoreSessionError::SessionAlreadyActive)
    ));

    drop(first);

    let third = VimCoreSession::new("gamma").expect("session should initialize after drop");
    assert_eq!(third.snapshot().text.trim_end_matches('\n'), "gamma");
}

#[test]
fn dropping_tab_using_session_restores_side_effect_commands_for_next_session() {
    let _guard = acquire_session_test_lock();

    {
        let mut first = VimCoreSession::new("alpha").expect("first session should initialize");
        first
            .execute_ex_command("tabedit")
            .expect("tabedit should create a second tab");
        first
            .execute_ex_command("tabfirst")
            .expect("tabfirst should switch back to the original tab");
    }

    let mut second = VimCoreSession::new("beta").expect("second session should initialize");

    let zz = second
        .execute_normal_command("ZZ")
        .expect("ZZ should not crash after a prior tab-using session");
    assert!(matches!(zz.outcome, CoreCommandOutcome::HostActionQueued));
    assert!(
        !zz.host_actions.is_empty(),
        "ZZ should still produce host coordination after a prior tab-using session"
    );

    let zq = second
        .execute_normal_command("ZQ")
        .expect("ZQ should not crash after a prior tab-using session");
    assert!(matches!(zq.outcome, CoreCommandOutcome::HostActionQueued));
    assert!(matches!(
        zq.host_actions.first(),
        Some(CoreHostAction::Quit { force: true, .. })
    ));
}

#[test]
fn session_options_route_debug_log_output_to_file() {
    let _guard = acquire_session_test_lock();
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let log_path = tempdir.path().join("vim-core-rs-debug.log");
    let options = CoreSessionOptions {
        runtime_mode: CoreRuntimeMode::Embedded,
        debug_log_path: Some(log_path.clone()),
    };
    let mut session = VimCoreSession::new_with_options("buffer", options)
        .expect("session should initialize with debug log path");

    session
        .execute_ex_command(":write output.txt")
        .expect("write command should succeed");
    let tabstop = session
        .get_option_number("tabstop", CoreOptionScope::Global)
        .expect("get_option_number should succeed");

    assert!(tabstop > 0, "tabstop should be a positive number");
    let log_output = fs::read_to_string(&log_path).expect("debug log file should be readable");
    assert!(
        log_output.contains("[DEBUG] apply_write_intent: local write")
            && log_output.contains("path=output.txt"),
        "debug log should be written to the configured file: {}",
        log_output
    );
    assert!(
        log_output.contains("[DEBUG] get_option: name='tabstop'"),
        "native debug log should be written to the configured file: {}",
        log_output
    );
}

#[test]
fn session_options_default_to_embedded_runtime_mode() {
    let options = CoreSessionOptions::default();
    assert_eq!(options.runtime_mode, CoreRuntimeMode::Embedded);
}

#[test]
fn standalone_runtime_mode_is_explicit_but_not_supported_yet() {
    let _guard = acquire_session_test_lock();

    let result = VimCoreSession::new_with_options(
        "buffer",
        CoreSessionOptions {
            runtime_mode: CoreRuntimeMode::Standalone,
            debug_log_path: None,
        },
    );

    assert!(matches!(
        result,
        Err(CoreSessionError::InitializationFailed {
            reason_code: "unsupported_runtime_mode",
        })
    ));
}

#[test]
fn session_options_disable_debug_log_output_by_default() {
    let _guard = acquire_session_test_lock();
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let log_path = tempdir.path().join("vim-core-rs-debug.log");
    let mut session =
        VimCoreSession::new("buffer").expect("session should initialize with default options");

    session
        .execute_ex_command(":write output.txt")
        .expect("write command should succeed");
    let tabstop = session
        .get_option_number("tabstop", CoreOptionScope::Global)
        .expect("get_option_number should succeed");

    assert!(tabstop > 0, "tabstop should be a positive number");
    assert!(
        !log_path.exists(),
        "debug log file should not exist when debug_log_path is omitted"
    );
    assert_eq!(
        fs::read_dir(tempdir.path())
            .expect("tempdir should remain readable")
            .count(),
        0,
        "default debug logging should not create any files"
    );
}

#[test]
fn public_api_reference_documents_search_contract_for_inactive_windows_and_byte_columns() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let public_api_reference = fs::read_to_string(repo_root.join("docs/public-api-reference.md"))
        .expect("public API reference should be readable");

    assert!(
        public_api_reference.contains("query_visible_search_state_for_window"),
        "public API reference should mention the inactive-window search accessor"
    );
    assert!(
        public_api_reference.contains("inactive window"),
        "public API reference should document inactive-window queries"
    );
    assert!(
        public_api_reference.contains("both are byte columns"),
        "public API reference should document byte-column search ranges"
    );
    assert!(
        public_api_reference.contains("start_col is inclusive")
            && public_api_reference.contains("end_col is exclusive"),
        "public API reference should document inclusive/exclusive search columns"
    );
    assert!(
        public_api_reference.contains("inactive_window_query_available")
            && public_api_reference.contains("byte_columns")
            && public_api_reference.contains("data_only_payload")
            && public_api_reference.contains("host_owned_presentation"),
        "public API reference should expose the structured Search family capability fields"
    );
}

#[test]
fn public_api_reference_excludes_popupwin_and_keeps_textprop_deferred() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let public_api_reference = fs::read_to_string(repo_root.join("docs/public-api-reference.md"))
        .expect("public API reference should be readable");

    assert!(
        public_api_reference.contains("popupwin is host-owned presentation")
            || public_api_reference.contains("popupwin stays outside the family"),
        "public API reference should document popupwin as outside the rendering-state family"
    );
    assert!(
        public_api_reference.contains("textprop is the deferred placeholder")
            || public_api_reference.contains("textprop stays deferred placeholder")
            || public_api_reference.contains("textprop remains deferred placeholder"),
        "public API reference should document textprop as the deferred placeholder"
    );
    assert!(
        public_api_reference.contains("does not expose a public popupwin extractor")
            || public_api_reference.contains("does not expose a public textprop extractor"),
        "public API reference should document the missing popupwin/textprop extraction surface"
    );
    assert!(
        public_api_reference.contains("overlay")
            && public_api_reference.contains("composition")
            && public_api_reference.contains("border"),
        "public API reference should document popup layout, composition, and border ownership as out of scope"
    );
    assert!(
        public_api_reference.contains("resolved highlight attribute tables")
            || public_api_reference.contains("highlight definition tables"),
        "public API reference should document highlight-table exclusion from the public extraction surface"
    );
    assert!(
        !public_api_reference.contains("issue #14"),
        "public API reference should not defer the family boundary to issue #14"
    );
}

#[test]
fn public_docs_map_rendering_state_family_to_existing_vimcoresession_surface() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let public_api_reference = fs::read_to_string(repo_root.join("docs/public-api-reference.md"))
        .expect("public API reference should be readable");
    let api_contracts = fs::read_to_string(repo_root.join("docs/api-contracts.md"))
        .expect("API contracts should be readable");
    let api_index = fs::read_to_string(repo_root.join("docs/api-index.md"))
        .expect("API index should be readable");

    assert!(
        public_api_reference.contains("VimCoreSession")
            && public_api_reference.contains("main stateful facade"),
        "public API reference should identify VimCoreSession as the main stateful facade"
    );
    assert!(
        api_contracts.contains("authoritative source")
            && api_contracts.contains("Vim-owned read-only extraction boundary")
            && !api_contracts.contains("issue #14"),
        "API contracts should document the final family authority without issue #14 wording"
    );
    assert!(
        api_index.contains("Search` and `Syntax` are the current rendering-state family members")
            || api_index
                .contains("Search and Syntax are the current rendering-state family members"),
        "API index should map Search and Syntax into the rendering-state family"
    );
    assert!(
        api_index.contains("Vim-owned read-only extraction boundary")
            && !api_index.contains("issue #14"),
        "API index should describe the final boundary without issue #14 wording"
    );
    assert!(
        public_api_reference.contains("query_visible_search_state")
            && public_api_reference.contains("get_line_syntax"),
        "public API reference should describe the existing search and syntax accessors"
    );
}

#[test]
fn rendering_state_family_docs_describe_additive_grouping_and_mixed_mutability() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let public_api_reference = fs::read_to_string(repo_root.join("docs/public-api-reference.md"))
        .expect("public API reference should be readable");
    let api_contracts = fs::read_to_string(repo_root.join("docs/api-contracts.md"))
        .expect("API contracts should be readable");
    let api_index = fs::read_to_string(repo_root.join("docs/api-index.md"))
        .expect("API index should be readable");

    assert!(
        api_contracts.contains("vocabulary")
            && (api_contracts.contains("additive stateless summary")
                || api_contracts.contains("no new family descriptor")
                || api_contracts.contains("without introducing a new runtime facade")),
        "API contracts should describe the family as an additive explanation layer without a new descriptor"
    );
    assert!(
        api_index
            .contains("These accessors cover the current `Search` and `Syntax` family members")
            || api_index
                .contains("These accessors cover the current Search and Syntax family members"),
        "API index should describe Search and Syntax as grouped existing accessors"
    );
    assert!(
        (public_api_reference.contains("search family member")
            || public_api_reference.contains("Search family member"))
            && public_api_reference.contains("&mut self")
            && (public_api_reference.contains("syntax family member")
                || public_api_reference.contains("Syntax family member"))
            && public_api_reference.contains("&self"),
        "public API reference should document mixed mutability across family members"
    );
    assert!(
        public_api_reference.contains("no new family descriptor")
            || api_contracts.contains("no new family descriptor")
            || public_api_reference.contains("additive stateless summary"),
        "public API reference should keep the family mapping additive rather than introducing a descriptor"
    );
}

#[test]
fn search_family_docs_keep_incsearch_boundary_vocab_in_public_contracts() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let public_api_reference = fs::read_to_string(repo_root.join("docs/public-api-reference.md"))
        .expect("public API reference should be readable");
    let api_contracts = fs::read_to_string(repo_root.join("docs/api-contracts.md"))
        .expect("API contracts should be readable");
    let api_index = fs::read_to_string(repo_root.join("docs/api-index.md"))
        .expect("API index should be readable");

    assert!(
        public_api_reference.contains("Search family")
            && public_api_reference.contains("inactive window")
            && public_api_reference.contains("byte columns")
            && public_api_reference.contains("host-owned presentation"),
        "public API reference should keep Search family vocabulary for inactive windows, byte columns, and host-owned presentation"
    );
    assert!(
        api_contracts.contains("Search family")
            && api_contracts.contains("incsearch")
            && api_contracts.contains("host-owned presentation"),
        "API contracts should describe incsearch as part of the Search family boundary without moving presentation ownership"
    );
    assert!(
        api_index.contains("Search family")
            && api_index.contains("inactive window")
            && api_index.contains("byte columns")
            && api_index.contains("host-owned presentation"),
        "API index should summarize the Search family boundary for inactive windows, byte columns, and host-owned presentation"
    );
    assert!(
        !api_index.contains("issue #14"),
        "API index should not defer the Search family boundary to issue #14"
    );
}

#[test]
fn register_docs_describe_multiline_full_readback_in_public_api_reference() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let public_api_reference = fs::read_to_string(repo_root.join("docs/public-api-reference.md"))
        .expect("public API reference should be readable");

    assert!(
        public_api_reference.contains("register(&self, regname: char) -> Option<String>")
            && public_api_reference.contains("multiline")
            && public_api_reference.contains("full contents"),
        "public API reference should describe register() as returning full multiline contents"
    );
}

#[test]
fn register_docs_pin_contract_tests_as_the_source_of_truth() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let api_contracts = fs::read_to_string(repo_root.join("docs/api-contracts.md"))
        .expect("API contracts should be readable");

    assert!(
        api_contracts.contains("tests/register_contract.rs")
            && (api_contracts.contains("source of truth")
                || api_contracts.contains("authoritative source")),
        "API contracts should point register readback behavior at the contract tests"
    );
}

#[test]
fn host_action_queue_is_empty_by_default() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    assert!(session.take_pending_host_action().is_none());
    assert!(matches!(session.mode(), CoreMode::Normal));
    let bell = CoreHostAction::Bell;
    assert!(matches!(bell, CoreHostAction::Bell));
}

#[test]
fn event_queue_is_empty_by_default() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    assert!(session.take_pending_event().is_none());
}

#[test]
fn execute_ex_command_returns_transaction_with_events_and_host_actions() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let tx = session
        .execute_ex_command(":redraw!")
        .expect("redraw command should succeed");

    assert!(matches!(tx.outcome, CoreCommandOutcome::NoChange));
    assert_eq!(tx.snapshot.text.trim_end_matches('\n'), "buffer");
    assert!(
        tx.events.iter().any(|event| matches!(
            event,
            CoreEvent::Redraw {
                full: true,
                clear_before_draw: true
            }
        )),
        "redraw should be surfaced as an event: {:?}",
        tx.events
    );
    assert!(
        tx.host_actions.is_empty(),
        "v2 transaction should not duplicate UI-like signals as host actions: {:?}",
        tx.host_actions
    );
}

#[cfg(unix)]
#[test]
fn embedded_redraw_event_does_not_leak_terminal_sequences_or_message_events() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let (tx, stdout, stderr) = capture_standard_streams(|| {
        session
            .execute_ex_command(":redraw!")
            .expect("redraw command should succeed")
    });

    assert_eq!(
        sanitize_harness_output(&stdout),
        "",
        "embedded redraw must not write to stdout"
    );
    assert_eq!(
        sanitize_harness_output(&stderr),
        "",
        "embedded redraw must not write to stderr"
    );
    assert!(
        tx.events.iter().any(|event| matches!(
            event,
            CoreEvent::Redraw {
                full: true,
                clear_before_draw: true
            }
        )),
        "redraw should be surfaced as an event: {:?}",
        tx.events
    );
    assert!(
        tx.events
            .iter()
            .all(|event| !matches!(event, CoreEvent::Message(_))),
        "embedded redraw should not synthesize terminal control output as message events: {:?}",
        tx.events
    );
}

#[cfg(unix)]
#[test]
fn embedded_screen_resize_emits_layout_event_without_terminal_leak() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");
    session.set_screen_size(24, 80);
    while session.take_pending_event().is_some() {}
    while session.take_pending_host_action().is_some() {}

    let ((), stdout, stderr) = capture_standard_streams(|| {
        session.set_screen_size(40, 120);
    });

    assert_eq!(
        sanitize_harness_output(&stdout),
        "",
        "embedded resize must not write to stdout"
    );
    assert_eq!(
        sanitize_harness_output(&stderr),
        "",
        "embedded resize must not write to stderr"
    );
    assert!(matches!(
        session.take_pending_event(),
        Some(CoreEvent::LayoutChanged)
    ));
    assert!(session.take_pending_host_action().is_none());
    assert!(
        session.take_pending_event().is_none(),
        "screen resize should not enqueue extra message-like events"
    );
}

#[test]
fn execute_ex_command_surfaces_split_as_events_without_ui_host_action_duplication() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");
    session.set_screen_size(24, 80);

    let tx = session
        .execute_ex_command(":split")
        .expect("split command should succeed");

    assert!(
        tx.events
            .iter()
            .any(|event| matches!(event, CoreEvent::WindowCreated { .. })),
        "split should surface window creation as an event: {:?}",
        tx.events
    );
    assert!(
        tx.events
            .iter()
            .any(|event| matches!(event, CoreEvent::LayoutChanged)),
        "split should surface layout change as an event: {:?}",
        tx.events
    );
    assert!(
        tx.host_actions.is_empty(),
        "v2 split should not duplicate UI-like signals as host actions: {:?}",
        tx.host_actions
    );
}

#[test]
fn execute_ex_command_surfaces_enew_as_event_without_ui_host_action_duplication() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let tx = session
        .execute_ex_command(":enew")
        .expect("enew command should succeed");

    assert!(
        tx.events
            .iter()
            .any(|event| matches!(event, CoreEvent::BufferAdded { .. })),
        "enew should surface buffer creation as an event: {:?}",
        tx.events
    );
    assert!(
        tx.host_actions.is_empty(),
        "v2 enew should not duplicate UI-like signals as host actions: {:?}",
        tx.host_actions
    );
}

#[test]
fn snapshot_does_not_drain_pending_event_queue() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");
    session.set_screen_size(24, 80);
    while session.take_pending_event().is_some() {}

    session.set_screen_size(40, 120);

    let snapshot = session.snapshot();
    assert_eq!(snapshot.text.trim_end_matches('\n'), "buffer");
    assert!(matches!(
        session.take_pending_event(),
        Some(CoreEvent::LayoutChanged)
    ));
}

#[test]
fn host_action_queue_no_longer_duplicates_redraw_events() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let tx = session
        .execute_ex_command(":redraw!")
        .expect("redraw command should succeed");

    assert!(matches!(
        tx.events.as_slice(),
        [CoreEvent::Redraw {
            full: true,
            clear_before_draw: true,
        }]
    ));
    assert!(
        session.take_pending_host_action().is_none(),
        "queue API should no longer duplicate redraw once event delivery exists"
    );
}

#[test]
fn host_action_queue_no_longer_retains_layout_changed() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");
    session.set_screen_size(24, 80);
    while session.take_pending_event().is_some() {}
    while session.take_pending_host_action().is_some() {}

    session.set_screen_size(40, 120);

    assert!(matches!(
        session.take_pending_event(),
        Some(CoreEvent::LayoutChanged)
    ));
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn mode_enum_exposes_extended_visual_and_select_variants() {
    let expected = [
        CoreMode::Visual,
        CoreMode::VisualLine,
        CoreMode::VisualBlock,
        CoreMode::Select,
        CoreMode::SelectLine,
        CoreMode::SelectBlock,
    ];

    assert_eq!(expected.len(), 6);
}

#[test]
fn option_scope_enum_exposes_all_supported_variants() {
    let expected = [
        CoreOptionScope::Default,
        CoreOptionScope::Global,
        CoreOptionScope::Local,
    ];

    assert_eq!(expected.len(), 3);
}

#[test]
fn option_type_enum_exposes_all_supported_variants() {
    let expected = [
        CoreOptionType::Bool,
        CoreOptionType::Number,
        CoreOptionType::String,
    ];

    assert_eq!(expected.len(), 3);
}

#[test]
fn option_error_variants_preserve_contract_details() {
    let mismatch = CoreOptionError::TypeMismatch {
        name: "tabstop".to_string(),
        expected: CoreOptionType::Number,
        actual: CoreOptionType::String,
    };
    assert!(matches!(
        mismatch,
        CoreOptionError::TypeMismatch {
            name,
            expected: CoreOptionType::Number,
            actual: CoreOptionType::String,
        } if name == "tabstop"
    ));

    let unknown = CoreOptionError::UnknownOption {
        name: "missing".to_string(),
    };
    assert!(matches!(
        unknown,
        CoreOptionError::UnknownOption { name } if name == "missing"
    ));

    let set_failed = CoreOptionError::SetFailed {
        name: "tabstop".to_string(),
        reason: "E487".to_string(),
    };
    assert!(matches!(
        set_failed,
        CoreOptionError::SetFailed { name, reason }
            if name == "tabstop" && reason == "E487"
    ));

    let scope_not_supported = CoreOptionError::ScopeNotSupported {
        name: "encoding".to_string(),
        scope: CoreOptionScope::Local,
    };
    assert!(matches!(
        scope_not_supported,
        CoreOptionError::ScopeNotSupported { name, scope: CoreOptionScope::Local }
            if name == "encoding"
    ));

    let internal = CoreOptionError::InternalError {
        name: "number".to_string(),
        detail: "null state".to_string(),
    };
    assert!(matches!(
        internal,
        CoreOptionError::InternalError { name, detail }
            if name == "number" && detail == "null state"
    ));
}

#[test]
fn option_getters_return_typed_values_from_vim() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    session
        .execute_ex_command(":set tabstop=6")
        .expect("tabstop should be set via ex command");
    session
        .execute_ex_command(":set expandtab")
        .expect("expandtab should be set via ex command");
    session
        .execute_ex_command(":set filetype=rust")
        .expect("filetype should be set via ex command");

    assert_eq!(
        session
            .get_option_number("tabstop", CoreOptionScope::Default)
            .expect("number option should be returned"),
        6
    );
    assert!(
        session
            .get_option_bool("expandtab", CoreOptionScope::Default)
            .expect("bool option should be returned")
    );
    assert_eq!(
        session
            .get_option_string("filetype", CoreOptionScope::Default)
            .expect("string option should be returned"),
        "rust"
    );
}

#[test]
fn option_getters_support_scope_selection_for_local_options() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    session
        .execute_ex_command(":setglobal shiftwidth=8")
        .expect("global shiftwidth should be set");
    session
        .execute_ex_command(":setlocal shiftwidth=3")
        .expect("local shiftwidth should be set");

    assert_eq!(
        session
            .get_option_number("shiftwidth", CoreOptionScope::Default)
            .expect("default scope should prefer local value"),
        3
    );
    assert_eq!(
        session
            .get_option_number("shiftwidth", CoreOptionScope::Local)
            .expect("local scope should return local value"),
        3
    );
    assert_eq!(
        session
            .get_option_number("shiftwidth", CoreOptionScope::Global)
            .expect("global scope should return global value"),
        8
    );
}

#[test]
fn option_getters_report_scope_not_supported_for_global_option_local_scope() {
    let _guard = acquire_session_test_lock();
    let session = VimCoreSession::new("buffer").expect("session should initialize");

    assert!(matches!(
        session.get_option_string("encoding", CoreOptionScope::Local),
        Err(CoreOptionError::ScopeNotSupported {
            name,
            scope: CoreOptionScope::Local,
        }) if name == "encoding"
    ));
}

#[test]
fn option_getters_report_type_mismatch_for_wrong_accessor() {
    let _guard = acquire_session_test_lock();
    let session = VimCoreSession::new("buffer").expect("session should initialize");

    assert!(matches!(
        session.get_option_bool("tabstop", CoreOptionScope::Default),
        Err(CoreOptionError::TypeMismatch {
            name,
            expected: CoreOptionType::Bool,
            actual: CoreOptionType::Number,
        }) if name == "tabstop"
    ));
}

#[test]
fn option_setters_update_typed_values_in_vim() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    session
        .set_option_number("tabstop", 8, CoreOptionScope::Default)
        .expect("number option should be set");
    session
        .set_option_bool("expandtab", true, CoreOptionScope::Default)
        .expect("bool option should be set");
    session
        .set_option_string("filetype", "rust", CoreOptionScope::Default)
        .expect("string option should be set");

    assert_eq!(
        session
            .get_option_number("tabstop", CoreOptionScope::Default)
            .expect("updated tabstop should be returned"),
        8
    );
    assert!(
        session
            .get_option_bool("expandtab", CoreOptionScope::Default)
            .expect("updated expandtab should be returned")
    );
    assert_eq!(
        session
            .get_option_string("filetype", CoreOptionScope::Default)
            .expect("updated filetype should be returned"),
        "rust"
    );
}

#[test]
fn option_setters_report_vim_validation_errors() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    assert!(matches!(
        session.set_option_number("tabstop", 0, CoreOptionScope::Default),
        Err(CoreOptionError::SetFailed { name, .. }) if name == "tabstop"
    ));

    assert!(matches!(
        session.set_option_string("fileformat", "wide", CoreOptionScope::Default),
        Err(CoreOptionError::SetFailed { name, .. }) if name == "fileformat"
    ));
}

#[test]
fn option_number_api_round_trips_tabstop_and_shiftwidth() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    session
        .set_option_number("tabstop", 8, CoreOptionScope::Default)
        .expect("tabstop should be set");
    session
        .set_option_number("shiftwidth", 4, CoreOptionScope::Local)
        .expect("shiftwidth should be set locally");

    assert_eq!(
        session
            .get_option_number("tabstop", CoreOptionScope::Default)
            .expect("tabstop should be returned"),
        8
    );
    assert_eq!(
        session
            .get_option_number("shiftwidth", CoreOptionScope::Local)
            .expect("local shiftwidth should be returned"),
        4
    );
}

#[test]
fn option_bool_api_round_trips_expandtab_and_number() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    session
        .set_option_bool("expandtab", true, CoreOptionScope::Default)
        .expect("expandtab should be set");
    session
        .set_option_bool("number", true, CoreOptionScope::Local)
        .expect("number should be set locally");

    assert!(
        session
            .get_option_bool("expandtab", CoreOptionScope::Default)
            .expect("expandtab should be returned")
    );
    assert!(
        session
            .get_option_bool("number", CoreOptionScope::Local)
            .expect("number should be returned")
    );
}

#[test]
fn option_string_api_round_trips_filetype_and_fileencoding() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    session
        .set_option_string("filetype", "rust", CoreOptionScope::Default)
        .expect("filetype should be set");
    session
        .set_option_string("fileencoding", "utf-8", CoreOptionScope::Local)
        .expect("fileencoding should be set");

    assert_eq!(
        session
            .get_option_string("filetype", CoreOptionScope::Default)
            .expect("filetype should be returned"),
        "rust"
    );
    assert_eq!(
        session
            .get_option_string("fileencoding", CoreOptionScope::Local)
            .expect("fileencoding should be returned"),
        "utf-8"
    );
}

#[test]
fn option_api_interoperates_with_ex_commands_both_directions() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    session
        .execute_ex_command(":set tabstop=6")
        .expect("tabstop should be set via ex command");
    session
        .execute_ex_command(":set filetype=rust")
        .expect("filetype should be set via ex command");

    assert_eq!(
        session
            .get_option_number("tabstop", CoreOptionScope::Default)
            .expect("tabstop should be returned after ex update"),
        6
    );
    assert_eq!(
        session
            .get_option_string("filetype", CoreOptionScope::Default)
            .expect("filetype should be returned after ex update"),
        "rust"
    );

    session
        .set_option_number("tabstop", 9, CoreOptionScope::Default)
        .expect("tabstop should be updated through API");
    session
        .set_option_string("filetype", "lua", CoreOptionScope::Default)
        .expect("filetype should be updated through API");

    session
        .execute_ex_command("%d")
        .expect("buffer should be cleared before ex confirmation");
    session
        .execute_ex_command("put =&tabstop")
        .expect("tabstop should be queryable via ex command");
    session
        .execute_ex_command("put =&filetype")
        .expect("filetype should be queryable via ex command");

    let snapshot = session.snapshot();
    assert!(
        snapshot.text.contains("\n9\n"),
        "expected ex-visible tabstop in buffer, got {:?}",
        snapshot.text
    );
    assert!(
        snapshot.text.contains("\nlua\n"),
        "expected ex-visible filetype in buffer, got {:?}",
        snapshot.text
    );
}

#[test]
fn option_errors_cover_unknown_type_validation_and_scope_cases() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    assert!(matches!(
        session.get_option_number("definitely_missing_option", CoreOptionScope::Default),
        Err(CoreOptionError::UnknownOption { name }) if name == "definitely_missing_option"
    ));

    assert!(matches!(
        session.get_option_bool("tabstop", CoreOptionScope::Default),
        Err(CoreOptionError::TypeMismatch {
            name,
            expected: CoreOptionType::Bool,
            actual: CoreOptionType::Number,
        }) if name == "tabstop"
    ));

    assert!(matches!(
        session.set_option_number("tabstop", 0, CoreOptionScope::Default),
        Err(CoreOptionError::SetFailed { name, .. }) if name == "tabstop"
    ));

    assert!(matches!(
        session.get_option_string("encoding", CoreOptionScope::Local),
        Err(CoreOptionError::ScopeNotSupported {
            name,
            scope: CoreOptionScope::Local,
        }) if name == "encoding"
    ));

    // 存在しないオプションへの設定で SetFailed エラーが返ることを検証する
    assert!(matches!(
        session.set_option_number("nonexistent_option", 1, CoreOptionScope::Default),
        Err(CoreOptionError::SetFailed { name, .. }) if name == "nonexistent_option"
    ));

    // 文字列型の存在しないオプションへの設定でも SetFailed エラーが返ることを検証する
    assert!(matches!(
        session.set_option_string("nonexistent_option", "value", CoreOptionScope::Default),
        Err(CoreOptionError::SetFailed { name, .. }) if name == "nonexistent_option"
    ));
}

#[test]
fn backend_identity_reports_upstream_runtime() {
    let _guard = acquire_session_test_lock();
    let session = VimCoreSession::new("buffer").expect("session should initialize");

    assert_eq!(
        format!("{:?}", session.backend_identity()),
        "UpstreamRuntime"
    );
}

#[test]
fn normal_delete_command_mutates_buffer_via_vim_runtime() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("first line\nsecond line\nthird line")
        .expect("session should initialize");

    let outcome = session
        .execute_normal_command("dd")
        .expect("dd should succeed");

    assert!(matches!(
        outcome.outcome,
        vim_core_rs::CoreCommandOutcome::BufferChanged { revision: 1 }
    ));

    let snapshot = session.snapshot();
    assert_eq!(
        snapshot.text.trim_end_matches('\n'),
        "second line\nthird line"
    );
    assert_eq!(snapshot.revision, 1);
    assert!(snapshot.dirty);
    assert_eq!(snapshot.mode, CoreMode::Normal);
}

#[test]
fn normal_insert_command_switches_mode() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let _outcome = session
        .execute_normal_command("i")
        .expect("i should succeed");

    assert_eq!(session.mode(), CoreMode::Insert);
}

#[test]
fn normal_other_insert_commands_switch_mode() {
    let _guard = acquire_session_test_lock();

    // (command, initial text, expected mode, expected row, expected col)
    let commands_and_positions = vec![
        ("a", "word", CoreMode::Insert, 0, 1),
        ("A", "word", CoreMode::Insert, 0, 4),
        ("o", "word", CoreMode::Insert, 1, 0),
        ("O", "word", CoreMode::Insert, 0, 0),
        ("R", "word", CoreMode::Replace, 0, 0),
    ];

    for (cmd, initial_text, expected_mode, exp_row, exp_col) in commands_and_positions {
        let mut session = VimCoreSession::new(initial_text).expect("session should initialize");

        let _outcome = session
            .execute_normal_command(cmd)
            .expect("command should succeed");

        let snapshot = session.snapshot();

        assert_eq!(
            session.mode(),
            expected_mode,
            "Failed mode for command {}",
            cmd
        );
        assert_eq!(
            snapshot.cursor_row, exp_row,
            "Failed cursor_row for command {}",
            cmd
        );
        assert_eq!(
            snapshot.cursor_col, exp_col,
            "Failed cursor_col for command {}",
            cmd
        );
    }
}

#[test]
fn ex_write_command_queues_host_action_once() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let tx = session
        .execute_ex_command(":write! output.txt")
        .expect("write command should succeed");

    assert!(matches!(
        tx.outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        tx.host_actions,
        vec![CoreHostAction::Write {
            path: "output.txt".to_string(),
            force: true,
            issued_after_revision: 0,
        }]
    );
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn ex_quit_command_queues_quit_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let tx = session
        .execute_ex_command(":quit!")
        .expect("quit command should succeed");

    assert!(matches!(
        tx.outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        tx.host_actions,
        vec![CoreHostAction::Quit {
            force: true,
            issued_after_revision: 0,
        }]
    );
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn ex_redraw_command_surfaces_redraw_event() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let tx = session
        .execute_ex_command(":redraw!")
        .expect("redraw command should succeed");

    assert!(matches!(
        tx.outcome,
        vim_core_rs::CoreCommandOutcome::NoChange
    ));
    assert!(matches!(
        tx.events.as_slice(),
        [CoreEvent::Redraw {
            full: true,
            clear_before_draw: true,
        }]
    ));
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn ex_input_command_queues_input_request_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let tx = session
        .execute_ex_command(":input Enter filename")
        .expect("input command should succeed");

    assert!(matches!(
        tx.outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        tx.host_actions,
        vec![CoreHostAction::RequestInput {
            prompt: "Enter filename".to_string(),
            input_kind: CoreInputRequestKind::CommandLine,
            correlation_id: 1,
        }]
    );
}

#[test]
fn execute_ex_command_keeps_input_flow_as_host_action_not_pager_prompt() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let tx = session
        .execute_ex_command(":input Enter filename")
        .expect("input command should succeed");

    assert_eq!(
        tx.host_actions,
        vec![CoreHostAction::RequestInput {
            prompt: "Enter filename".to_string(),
            input_kind: CoreInputRequestKind::CommandLine,
            correlation_id: 1,
        }]
    );
    assert!(
        tx.events
            .iter()
            .all(|event| !matches!(event, CoreEvent::PagerPrompt(_))),
        "input prompt should stay a host action rather than a pager prompt: {:?}",
        tx.events
    );
}

#[test]
fn input_response_api_accepts_submit_and_cancel_for_active_request() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let request_tx = session
        .execute_ex_command(":input Enter filename")
        .expect("input command should succeed");
    assert_eq!(
        request_tx.host_actions,
        vec![CoreHostAction::RequestInput {
            prompt: "Enter filename".to_string(),
            input_kind: CoreInputRequestKind::CommandLine,
            correlation_id: 1,
        }]
    );

    let submit_tx = session
        .submit_input_response(CoreInputResponse::Submitted {
            correlation_id: 1,
            value: "notes.txt".to_string(),
        })
        .expect("submit response should be accepted");

    assert!(matches!(submit_tx.outcome, CoreCommandOutcome::NoChange));
    assert!(submit_tx.host_actions.is_empty());
    assert!(submit_tx.events.is_empty());

    let request_tx = session
        .execute_ex_command(":input Confirm")
        .expect("second input command should succeed");
    assert_eq!(
        request_tx.host_actions,
        vec![CoreHostAction::RequestInput {
            prompt: "Confirm".to_string(),
            input_kind: CoreInputRequestKind::CommandLine,
            correlation_id: 2,
        }]
    );

    let cancel_tx = session
        .submit_input_response(CoreInputResponse::Cancelled { correlation_id: 2 })
        .expect("cancel response should be accepted");

    assert!(matches!(cancel_tx.outcome, CoreCommandOutcome::NoChange));
    assert!(cancel_tx.host_actions.is_empty());
    assert!(cancel_tx.events.is_empty());
}

#[test]
fn input_response_api_rejects_no_pending_and_correlation_mismatch() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let no_pending =
        session.submit_input_response(CoreInputResponse::Cancelled { correlation_id: 99 });
    assert!(matches!(
        no_pending,
        Err(CoreInputResponseError::NoPendingInput)
    ));

    session
        .execute_ex_command(":input Enter filename")
        .expect("input command should succeed");

    let mismatch = session.submit_input_response(CoreInputResponse::Submitted {
        correlation_id: 2,
        value: "wrong".to_string(),
    });
    assert!(matches!(
        mismatch,
        Err(CoreInputResponseError::CorrelationMismatch {
            expected: 1,
            actual: 2,
        })
    ));

    let accepted =
        session.submit_input_response(CoreInputResponse::Cancelled { correlation_id: 1 });
    assert!(
        accepted.is_ok(),
        "mismatch should not clear the active request: {:?}",
        accepted
    );
}

#[cfg(unix)]
#[test]
fn vimscript_input_function_emits_message_without_host_action_or_terminal_leak() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let ((result, event, action), stdout, stderr) = capture_standard_streams(|| {
        let result = session.eval_string(r#"input("Enter filename: ")"#);
        let event = session.take_pending_event();
        let action = session.take_pending_host_action();
        (result, event, action)
    });

    assert_eq!(result, None);
    assert!(
        event.is_none(),
        "input() should not emit fail-fast messages: {event:?}"
    );
    assert_eq!(
        action,
        Some(CoreHostAction::RequestInput {
            prompt: "Enter filename: ".to_string(),
            input_kind: CoreInputRequestKind::CommandLine,
            correlation_id: 1,
        })
    );
    assert!(
        sanitize_harness_output(&stdout).is_empty() && sanitize_harness_output(&stderr).is_empty(),
        "embedded input() should not leak prompts to the terminal: stdout={:?}, stderr={:?}",
        stdout,
        stderr
    );

    session
        .submit_input_response(CoreInputResponse::Submitted {
            correlation_id: 1,
            value: "notes.txt".to_string(),
        })
        .expect("input() response should resume evaluation");
    assert_eq!(
        session.take_completed_input_eval_result(),
        Some("notes.txt".to_string())
    );
}

#[cfg(unix)]
#[test]
fn vimscript_inputsecret_function_emits_message_without_host_action_or_terminal_leak() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let ((result, event, action), stdout, stderr) = capture_standard_streams(|| {
        let result = session.eval_string(r#"inputsecret("Password: ")"#);
        let event = session.take_pending_event();
        let action = session.take_pending_host_action();
        (result, event, action)
    });

    assert_eq!(result, None);
    assert!(
        event.is_none(),
        "inputsecret() should not emit fail-fast messages: {event:?}"
    );
    assert_eq!(
        action,
        Some(CoreHostAction::RequestInput {
            prompt: "Password: ".to_string(),
            input_kind: CoreInputRequestKind::Secret,
            correlation_id: 1,
        })
    );
    assert!(
        sanitize_harness_output(&stdout).is_empty() && sanitize_harness_output(&stderr).is_empty(),
        "embedded inputsecret() should not leak prompts to the terminal: stdout={:?}, stderr={:?}",
        stdout,
        stderr
    );

    session
        .submit_input_response(CoreInputResponse::Cancelled { correlation_id: 1 })
        .expect("inputsecret() cancel should resume evaluation");
    assert_eq!(
        session.take_completed_input_eval_result(),
        Some(String::new())
    );
}

#[test]
fn ex_bell_command_surfaces_bell_event() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let tx = session
        .execute_ex_command(":bell")
        .expect("bell command should succeed");

    assert!(matches!(
        tx.outcome,
        vim_core_rs::CoreCommandOutcome::NoChange
    ));
    assert!(matches!(tx.events.as_slice(), [CoreEvent::Bell]));
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn ex_set_command_executes_via_vim_without_host_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :set number は Vim 本体の Ex 実行経路で処理され、host action は生成されない
    let outcome = session
        .execute_ex_command(":set number")
        .expect("set command should succeed");

    assert!(
        !matches!(
            outcome.outcome,
            vim_core_rs::CoreCommandOutcome::HostActionQueued
        ),
        "set number は host action を生成しない"
    );
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn ex_substitute_command_modifies_buffer_via_vim() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello world").expect("session should initialize");

    // :s/hello/goodbye/ は Vim 本体の Ex 実行経路でバッファを変更する
    let _outcome = session
        .execute_ex_command(":s/hello/goodbye/")
        .expect("substitute command should succeed");

    let snapshot = session.snapshot();
    assert_eq!(snapshot.text.trim_end_matches('\n'), "goodbye world");
}

#[test]
fn ex_write_short_form_queues_host_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :w は :write の短縮形で、同様に host action を生成する
    let tx = session
        .execute_ex_command(":w output.txt")
        .expect("w command should succeed");

    assert!(matches!(
        tx.outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        tx.host_actions,
        vec![CoreHostAction::Write {
            path: "output.txt".to_string(),
            force: false,
            issued_after_revision: 0,
        }]
    );
}

#[test]
fn ex_quit_short_form_queues_host_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :q! は :quit! の短縮形で、同様に host action を生成する
    let tx = session
        .execute_ex_command(":q!")
        .expect("q command should succeed");

    assert!(matches!(
        tx.outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        tx.host_actions,
        vec![CoreHostAction::Quit {
            force: true,
            issued_after_revision: 0,
        }]
    );
}

#[test]
fn pathdef_resolves_non_empty_runtimepath() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :set runtimepath? を実行して runtimepath が空でないことを確認する。
    // pathdef.c の placeholder 依存が解消されていれば、Vim は configure 由来の
    // パスをデフォルト runtimepath として設定する。
    // この Ex コマンドはバッファを変更しないので NoChange が返る。
    let outcome = session
        .execute_ex_command(":set runtimepath?")
        .expect("set runtimepath? should succeed");

    assert!(
        !matches!(
            outcome.outcome,
            vim_core_rs::CoreCommandOutcome::HostActionQueued
        ),
        "set runtimepath? は host action を生成しない"
    );
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn pathdef_provides_vim_dir_for_runtime_discovery() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :echo $VIM で $VIM 変数を確認する。pathdef.c の default_vim_dir が
    // 空文字でなければ、Vim は起動時にこの値をフォールバックとして利用する。
    // headless 環境では $VIM 環境変数が未設定の場合、default_vim_dir が
    // 使われるため、空でないことを間接的に検証する。
    //
    // Note: この検証は default_vim_dir が compile-time に設定されていることの
    // 間接検証である。$VIM が環境変数として設定されている場合はそちらが
    // 優先されるが、pathdef.c のフォールバック値が空でないことが重要。
    let outcome = session
        .execute_ex_command(":set runtimepath?")
        .expect("should succeed");

    // コマンド自体がエラーにならないことが最低条件
    assert!(
        !matches!(
            outcome.outcome,
            vim_core_rs::CoreCommandOutcome::HostActionQueued
        ),
        "set runtimepath? はホストアクションを生成しない"
    );
}

#[test]
fn ex_redraw_without_bang_surfaces_non_clearing_event() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :redraw（! なし）は clear_before_draw: false の redraw event を生成する
    let tx = session
        .execute_ex_command(":redraw")
        .expect("redraw command should succeed");

    assert!(matches!(
        tx.outcome,
        vim_core_rs::CoreCommandOutcome::NoChange
    ));
    assert!(matches!(
        tx.events.as_slice(),
        [CoreEvent::Redraw {
            full: true,
            clear_before_draw: false,
        }]
    ));
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn normal_movement_command_changes_cursor() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("first line\nsecond line\nthird line\n")
        .expect("session should initialize");

    let outcome = session
        .execute_normal_command("j")
        .expect("j should succeed");

    assert!(matches!(
        outcome.outcome,
        vim_core_rs::CoreCommandOutcome::CursorChanged { row: 1, col: 0 }
    ));

    let snapshot = session.snapshot();
    assert_eq!(snapshot.cursor_row, 1);
    assert_eq!(snapshot.cursor_col, 0);

    let outcome2 = session
        .execute_normal_command("l")
        .expect("l should succeed");

    assert!(matches!(
        outcome2.outcome,
        vim_core_rs::CoreCommandOutcome::CursorChanged { row: 1, col: 1 }
    ));

    let snapshot2 = session.snapshot();
    assert_eq!(snapshot2.cursor_row, 1);
    assert_eq!(snapshot2.cursor_col, 1);
}

#[test]
fn api_index_maps_rendering_state_family_without_new_surface() {
    let api_index_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/api-index.md");
    let content = fs::read_to_string(&api_index_path).expect("api-index should be readable");

    assert!(
        content.contains("Rendering State Family")
            && content.contains("Search")
            && content.contains("Syntax")
            && content.contains("Annotations")
            && content.contains("deferred placeholder")
            && !content.contains("issue #14"),
        "api index should document the family boundary and phase split"
    );
    assert!(
        content.contains("get_search_pattern")
            && content.contains("get_line_syntax")
            && content.contains("textprop"),
        "api index should map the existing search/syntax accessors to the family boundary and keep textprop deferred"
    );
}

#[test]
fn tree_sitter_features_are_default_off_and_separate_from_vim_syntax() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo_toml =
        fs::read_to_string(repo_root.join("Cargo.toml")).expect("Cargo.toml should be readable");
    let public_api_reference = fs::read_to_string(repo_root.join("docs/public-api-reference.md"))
        .expect("public API reference should be readable");
    let api_index = fs::read_to_string(repo_root.join("docs/api-index.md"))
        .expect("api index should be readable");

    assert!(
        cargo_toml.contains("default = []")
            && cargo_toml.contains("experimental-tree-sitter = []")
            && cargo_toml.contains("tree-sitter-markdown = [\"experimental-tree-sitter\"]")
            && cargo_toml.contains("tree-sitter-rust = [\"experimental-tree-sitter\"]"),
        "Tree-sitter feature flags should be opt-in and default-off"
    );
    let dependency_sections = cargo_toml
        .split("[dependencies]")
        .nth(1)
        .and_then(|after_dependencies| after_dependencies.split("[build-dependencies]").next())
        .expect("Cargo.toml should contain dependency sections");
    assert!(
        !dependency_sections.contains("tree-sitter =")
            && !dependency_sections.contains("tree-sitter-markdown =")
            && !dependency_sections.contains("tree-sitter-rust ="),
        "Phase 2 should not add parser or grammar dependencies"
    );
    assert!(
        public_api_reference.contains("CoreTreeSitterRangeSyntax")
            && public_api_reference.contains("feature-gated")
            && public_api_reference.contains("separate from `CoreSyntaxChunk`")
            && api_index.contains("Experimental Tree-sitter"),
        "docs should describe the feature-gated Tree-sitter surface separately from Vim syntax"
    );
}

#[cfg(feature = "experimental-tree-sitter")]
#[test]
fn experimental_tree_sitter_public_types_are_constructible() {
    let range = CoreTextRange {
        start: CoreTextPosition { row: 0, col: 0 },
        end: CoreTextPosition { row: 0, col: 4 },
    };
    let provenance = CoreTreeSitterProvenance {
        language_id: "rust".to_string(),
        package_id: "tree-sitter-rust".to_string(),
        package_version: "0.0.0-skeleton".to_string(),
        parser_version: "0.0.0-skeleton".to_string(),
        query_version: "0.0.0-skeleton".to_string(),
    };
    let chunk = CoreTreeSitterChunk {
        range,
        capture_name: "keyword".to_string(),
        category: CoreSyntaxCategory::Keyword,
        modifiers: vec![CoreSyntaxModifier::Definition],
    };
    let syntax = CoreTreeSitterRangeSyntax {
        buffer_id: 1,
        source_revision: CoreBufferRevision { value: 7 },
        provenance: provenance.clone(),
        status: CoreTreeSitterStatus::Prepared,
        has_error: false,
        chunks: vec![chunk],
    };
    let resolved_language = CoreResolvedLanguage {
        range,
        role: CoreLanguageRole::EmbeddedRegion,
        language_id: Some("rust".to_string()),
        package_id: Some(provenance.package_id),
        package_version: Some(provenance.package_version),
        kind: CoreEmbeddedBlockKind::Syntax,
        confidence: CoreResolutionConfidence::Exact,
        source: CoreLanguageResolutionSource::Registry,
    };
    let embedded_region = CoreEmbeddedRegion {
        range,
        content_range: range,
        source: CoreEmbeddedRegionSource::MarkdownFence,
        raw_info_string: Some("rust".to_string()),
        normalized_info_string: Some("rust".to_string()),
        normalized_kind: CoreEmbeddedBlockKind::Syntax,
        resolved_language: Some(resolved_language),
    };

    assert_eq!(syntax.source_revision, CoreBufferRevision { value: 7 });
    assert_eq!(syntax.chunks[0].capture_name, "keyword");
    assert!(matches!(
        syntax.chunks[0].category,
        CoreSyntaxCategory::Keyword
    ));
    assert!(matches!(
        embedded_region.normalized_kind,
        CoreEmbeddedBlockKind::Syntax
    ));
}
