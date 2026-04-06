use std::sync::{Mutex, MutexGuard, OnceLock};
use vim_core_rs::{CoreMatchType, CoreSearchHighlightMode, CoreSearchQueryError, VimCoreSession};

fn session_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn acquire_session_test_lock() -> MutexGuard<'static, ()> {
    session_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn active_window_id(session: &VimCoreSession) -> i32 {
    session
        .snapshot()
        .windows
        .iter()
        .find(|window| window.is_active)
        .map(|window| window.id)
        .expect("active window should exist")
}

#[test]
fn incsearch_active_query_exposes_live_preview_state() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("alpha\nhello world hello\nomega\n")
        .expect("session should initialize");

    session
        .execute_ex_command("set incsearch hlsearch")
        .unwrap();
    session.execute_normal_command("/hello").unwrap();

    let state = session
        .query_visible_search_state(1, 3)
        .expect("live search state should be queryable");

    assert_eq!(state.mode, CoreSearchHighlightMode::IncSearch);
    assert!(state.incsearch_active);
    assert_eq!(state.input_pattern.as_deref(), Some("hello"));
    assert_eq!(state.pattern.as_deref(), Some("hello"));
    assert!(state.hlsearch_enabled);
    assert!(!state.hlsearch_suspended);
    assert_eq!(state.window_id, active_window_id(&session));
    assert_eq!(state.ranges.len(), 2);
    assert!(
        state
            .ranges
            .iter()
            .any(|range| range.match_type == CoreMatchType::CurSearch)
    );
    assert!(
        state
            .ranges
            .iter()
            .any(|range| range.match_type == CoreMatchType::IncSearch)
    );
}

#[test]
fn incsearch_disabled_keeps_input_pattern_but_returns_no_preview_ranges() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("alpha\nhello world hello\nomega\n")
        .expect("session should initialize");

    session.execute_ex_command("let @/ = ''").unwrap();
    session
        .execute_ex_command("set noincsearch nohlsearch")
        .unwrap();
    session.execute_normal_command("/hello").unwrap();

    let state = session
        .query_visible_search_state(1, 3)
        .expect("search state should still be queryable");

    assert_eq!(state.mode, CoreSearchHighlightMode::Disabled);
    assert!(!state.incsearch_active);
    assert_eq!(state.input_pattern.as_deref(), Some("hello"));
    assert!(state.pattern.is_none());
    assert!(state.ranges.is_empty());
}

#[test]
fn escape_clears_incsearch_preview_state() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("alpha\nhello world hello\nomega\n")
        .expect("session should initialize");

    session.execute_ex_command("let @/ = ''").unwrap();
    session
        .execute_ex_command("set incsearch nohlsearch")
        .unwrap();
    session.execute_normal_command("/hello").unwrap();

    let before_cancel = session.query_visible_search_state(1, 3).unwrap();
    assert!(before_cancel.incsearch_active);
    assert!(!before_cancel.ranges.is_empty());

    session.dispatch_key("\u{1b}").unwrap();

    let after_cancel = session.query_visible_search_state(1, 3).unwrap();
    assert_eq!(after_cancel.mode, CoreSearchHighlightMode::Disabled);
    assert!(!after_cancel.incsearch_active);
    assert!(after_cancel.input_pattern.is_none());
    assert!(after_cancel.pattern.is_none());
    assert!(after_cancel.ranges.is_empty());
}

#[test]
fn enter_commits_preview_into_regular_search_state() {
    let _guard = acquire_session_test_lock();
    {
        let mut preview_session = VimCoreSession::new("alpha\nhello world hello\nomega\n")
            .expect("session should initialize");

        preview_session.execute_ex_command("let @/ = ''").unwrap();
        preview_session
            .execute_ex_command("set incsearch hlsearch")
            .unwrap();
        preview_session.execute_normal_command("/hello").unwrap();

        let before_enter = preview_session.query_visible_search_state(1, 3).unwrap();
        assert_eq!(before_enter.mode, CoreSearchHighlightMode::IncSearch);
    }

    let mut committed_session = VimCoreSession::new("alpha\nhello world hello\nomega\n")
        .expect("session should initialize");
    committed_session.execute_ex_command("let @/ = ''").unwrap();
    committed_session
        .execute_ex_command("set incsearch hlsearch")
        .unwrap();
    committed_session
        .execute_normal_command("/hello\r")
        .unwrap();

    let after_enter = committed_session.query_visible_search_state(1, 3).unwrap();
    assert_eq!(after_enter.mode, CoreSearchHighlightMode::HlSearch);
    assert!(!after_enter.incsearch_active);
    assert!(after_enter.input_pattern.is_none());
    assert_eq!(after_enter.pattern.as_deref(), Some("hello"));
    assert!(
        after_enter
            .ranges
            .iter()
            .all(|range| range.match_type != CoreMatchType::IncSearch)
    );
    assert!(
        after_enter
            .ranges
            .iter()
            .any(|range| range.match_type == CoreMatchType::CurSearch)
    );
}

#[test]
fn query_visible_search_state_marks_current_match_and_limits_rows() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("hit\nmiss\nhit hit\n").expect("session should initialize");

    session.execute_ex_command("set hlsearch").unwrap();
    session.execute_ex_command("let @/ = 'hit'").unwrap();
    session.execute_normal_command("2j").unwrap();

    let state = session.query_visible_search_state(3, 3).unwrap();

    assert_eq!(state.mode, CoreSearchHighlightMode::HlSearch);
    assert_eq!(state.ranges.len(), 2);
    assert!(state.ranges.iter().all(|range| range.start_row == 3));
    assert!(
        state
            .ranges
            .iter()
            .any(|range| range.match_type == CoreMatchType::CurSearch)
    );
}

#[test]
fn query_visible_search_state_uses_byte_columns_for_halfwidth_matches() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("abc abc\n").expect("session should initialize");

    session.execute_ex_command("set hlsearch").unwrap();
    session.execute_ex_command("let @/ = 'abc'").unwrap();

    let state = session.query_visible_search_state(1, 1).unwrap();

    assert_eq!(state.ranges.len(), 2);
    assert_eq!(state.ranges[0].start_col, 0);
    assert_eq!(state.ranges[0].end_col, 3);
    assert_eq!(state.ranges[1].start_col, 4);
    assert_eq!(state.ranges[1].end_col, 7);
}

#[test]
fn query_visible_search_state_uses_byte_columns_for_fullwidth_matches() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("あい あい\n").expect("session should initialize");

    session.execute_ex_command("set hlsearch").unwrap();
    session.execute_ex_command("let @/ = 'あい'").unwrap();

    let state = session.query_visible_search_state(1, 1).unwrap();

    assert_eq!(state.ranges.len(), 2);
    assert_eq!(state.ranges[0].start_col, 0);
    assert_eq!(state.ranges[0].end_col, 6);
    assert_eq!(state.ranges[1].start_col, 7);
    assert_eq!(state.ranges[1].end_col, 13);
}

#[test]
fn search_capability_contract_is_available_and_documents_column_semantics() {
    let contract = VimCoreSession::search_capability_contract();

    assert!(contract.live_state_query_available);
    assert!(contract.visible_rows_only);
    assert!(contract.start_col_inclusive);
    assert!(contract.end_col_exclusive);
}

#[test]
fn query_visible_search_state_rejects_invalid_viewport() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello\n").expect("session should initialize");

    let error = session
        .query_visible_search_state(3, 2)
        .expect_err("invalid viewport should fail");

    assert_eq!(
        error,
        CoreSearchQueryError::InvalidViewport {
            start_row: 3,
            end_row: 2,
        }
    );
}

#[test]
fn query_visible_search_state_rejects_unknown_window() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello\n").expect("session should initialize");

    let error = session
        .query_visible_search_state_for_window(999_999, 1, 1)
        .expect_err("unknown window should fail");

    assert_eq!(
        error,
        CoreSearchQueryError::WindowNotFound { window_id: 999_999 }
    );
}
