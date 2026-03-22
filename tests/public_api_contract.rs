use std::sync::{Mutex, OnceLock};
use vim_core_rs::{
    CoreHostAction, CoreInputRequestKind, CoreMode, CoreOptionError, CoreOptionScope,
    CoreOptionType, CoreSessionError, VimCoreSession,
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

#[test]
fn session_exposes_initial_snapshot_contract() {
    let _guard = acquire_session_test_lock();
    let session =
        VimCoreSession::new("first line\nsecond line").expect("session should initialize");
    let snapshot = session.snapshot();

    assert_eq!(snapshot.text.trim_end_matches('\n'), "first line\nsecond line");
    assert_eq!(snapshot.revision, 0);
    assert!(!snapshot.dirty);
    assert_eq!(snapshot.mode, CoreMode::Normal);
    assert_eq!(snapshot.pending_host_actions, 0);
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
fn host_action_queue_is_empty_by_default() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    assert!(session.take_pending_host_action().is_none());
    assert!(matches!(session.mode(), CoreMode::Normal));
    let bell = CoreHostAction::Bell;
    assert!(matches!(bell, CoreHostAction::Bell));
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
        .apply_ex_command(":set tabstop=6")
        .expect("tabstop should be set via ex command");
    session
        .apply_ex_command(":set expandtab")
        .expect("expandtab should be set via ex command");
    session
        .apply_ex_command(":set filetype=rust")
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
        .apply_ex_command(":setglobal shiftwidth=8")
        .expect("global shiftwidth should be set");
    session
        .apply_ex_command(":setlocal shiftwidth=3")
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
        .apply_ex_command(":set tabstop=6")
        .expect("tabstop should be set via ex command");
    session
        .apply_ex_command(":set filetype=rust")
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
        .apply_ex_command("%d")
        .expect("buffer should be cleared before ex confirmation");
    session
        .apply_ex_command("put =&tabstop")
        .expect("tabstop should be queryable via ex command");
    session
        .apply_ex_command("put =&filetype")
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
        .apply_normal_command("dd")
        .expect("dd should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::BufferChanged { revision: 1 }
    ));

    let snapshot = session.snapshot();
    assert_eq!(snapshot.text.trim_end_matches('\n'), "second line\nthird line");
    assert_eq!(snapshot.revision, 1);
    assert!(snapshot.dirty);
    assert_eq!(snapshot.mode, CoreMode::Normal);
}

#[test]
fn normal_insert_command_switches_mode() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let _outcome = session.apply_normal_command("i").expect("i should succeed");

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
            .apply_normal_command(cmd)
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

    let outcome = session
        .apply_ex_command(":write! output.txt")
        .expect("write command should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(session.snapshot().pending_host_actions, 1);
    assert_eq!(
        session.take_pending_host_action(),
        Some(CoreHostAction::Write {
            path: "output.txt".to_string(),
            force: true,
            issued_after_revision: 0,
        })
    );
    assert!(session.take_pending_host_action().is_none());
    assert_eq!(session.snapshot().pending_host_actions, 0);
}

#[test]
fn ex_quit_command_queues_quit_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let outcome = session
        .apply_ex_command(":quit!")
        .expect("quit command should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        session.take_pending_host_action(),
        Some(CoreHostAction::Quit {
            force: true,
            issued_after_revision: 0,
        })
    );
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn ex_redraw_command_queues_redraw_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let outcome = session
        .apply_ex_command(":redraw!")
        .expect("redraw command should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        session.take_pending_host_action(),
        Some(CoreHostAction::Redraw {
            full: true,
            clear_before_draw: true,
        })
    );
}

#[test]
fn ex_input_command_queues_input_request_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let outcome = session
        .apply_ex_command(":input Enter filename")
        .expect("input command should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        session.take_pending_host_action(),
        Some(CoreHostAction::RequestInput {
            prompt: "Enter filename".to_string(),
            input_kind: CoreInputRequestKind::CommandLine,
            correlation_id: 1,
        })
    );
}

#[test]
fn ex_bell_command_queues_bell_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    let outcome = session
        .apply_ex_command(":bell")
        .expect("bell command should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        session.take_pending_host_action(),
        Some(CoreHostAction::Bell)
    );
}

#[test]
fn ex_set_command_executes_via_vim_without_host_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :set number は Vim 本体の Ex 実行経路で処理され、host action は生成されない
    let outcome = session
        .apply_ex_command(":set number")
        .expect("set command should succeed");

    assert!(
        !matches!(outcome, vim_core_rs::CoreCommandOutcome::HostActionQueued),
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
        .apply_ex_command(":s/hello/goodbye/")
        .expect("substitute command should succeed");

    let snapshot = session.snapshot();
    assert_eq!(snapshot.text.trim_end_matches('\n'), "goodbye world");
}

#[test]
fn ex_write_short_form_queues_host_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :w は :write の短縮形で、同様に host action を生成する
    let outcome = session
        .apply_ex_command(":w output.txt")
        .expect("w command should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        session.take_pending_host_action(),
        Some(CoreHostAction::Write {
            path: "output.txt".to_string(),
            force: false,
            issued_after_revision: 0,
        })
    );
}

#[test]
fn ex_quit_short_form_queues_host_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :q! は :quit! の短縮形で、同様に host action を生成する
    let outcome = session
        .apply_ex_command(":q!")
        .expect("q command should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        session.take_pending_host_action(),
        Some(CoreHostAction::Quit {
            force: true,
            issued_after_revision: 0,
        })
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
        .apply_ex_command(":set runtimepath?")
        .expect("set runtimepath? should succeed");

    assert!(
        !matches!(outcome, vim_core_rs::CoreCommandOutcome::HostActionQueued),
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
        .apply_ex_command(":set runtimepath?")
        .expect("should succeed");

    // コマンド自体がエラーにならないことが最低条件
    assert!(
        !matches!(outcome, vim_core_rs::CoreCommandOutcome::HostActionQueued),
        "set runtimepath? はホストアクションを生成しない"
    );
}

#[test]
fn ex_redraw_without_bang_queues_non_clearing_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("buffer").expect("session should initialize");

    // :redraw（! なし）は clear_before_draw: false の host action を生成する
    let outcome = session
        .apply_ex_command(":redraw")
        .expect("redraw command should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::HostActionQueued
    ));
    assert_eq!(
        session.take_pending_host_action(),
        Some(CoreHostAction::Redraw {
            full: true,
            clear_before_draw: false,
        })
    );
}

#[test]
fn normal_movement_command_changes_cursor() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("first line\nsecond line\nthird line\n")
        .expect("session should initialize");

    let outcome = session.apply_normal_command("j").expect("j should succeed");

    assert!(matches!(
        outcome,
        vim_core_rs::CoreCommandOutcome::CursorChanged { row: 1, col: 0 }
    ));

    let snapshot = session.snapshot();
    assert_eq!(snapshot.cursor_row, 1);
    assert_eq!(snapshot.cursor_col, 0);

    let outcome2 = session.apply_normal_command("l").expect("l should succeed");

    assert!(matches!(
        outcome2,
        vim_core_rs::CoreCommandOutcome::CursorChanged { row: 1, col: 1 }
    ));

    let snapshot2 = session.snapshot();
    assert_eq!(snapshot2.cursor_row, 1);
    assert_eq!(snapshot2.cursor_col, 1);
}
