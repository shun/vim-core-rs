use std::sync::{Mutex, MutexGuard, OnceLock};
use vim_core_rs::{CoreMode, CorePendingArgumentKind, CorePendingInput, VimCoreSession};

fn session_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn acquire_session_test_lock() -> MutexGuard<'static, ()> {
    session_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn pending(
    keys: &str,
    count: Option<usize>,
    awaited_argument: Option<CorePendingArgumentKind>,
) -> CorePendingInput {
    CorePendingInput {
        pending_keys: keys.to_string(),
        count,
        awaited_argument,
    }
}

fn assert_pending_state(session: &VimCoreSession, expected: CorePendingInput) {
    assert_eq!(session.pending_input(), expected);
    assert_eq!(session.snapshot().pending_input, expected);
}

#[test]
fn sequential_dispatch_handles_gg_and_reports_pending_prefix() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("one\ntwo\nthree\n").expect("session should initialize");

    session.dispatch_key("g").expect("first g should succeed");
    assert_pending_state(&session, pending("g", None, None));

    session.dispatch_key("g").expect("second g should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert_eq!(session.snapshot().cursor_row, 0);
}

#[test]
fn sequential_dispatch_handles_dd_and_reports_operator_pending() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("first line\nsecond line\nthird line\n")
        .expect("session should initialize");

    session.dispatch_key("d").expect("d should succeed");
    assert_pending_state(
        &session,
        pending("d", None, Some(CorePendingArgumentKind::MotionOrTextObject)),
    );

    session.dispatch_key("d").expect("second d should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert_eq!(
        session.snapshot().text.trim_end_matches('\n'),
        "second line\nthird line"
    );
}

#[test]
fn sequential_dispatch_handles_ciw_and_reports_each_pending_transition() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("alpha beta gamma\n").expect("session should initialize");

    session.dispatch_key("c").expect("c should succeed");
    assert_pending_state(
        &session,
        pending("c", None, Some(CorePendingArgumentKind::MotionOrTextObject)),
    );

    session.dispatch_key("i").expect("i should succeed");
    assert_pending_state(
        &session,
        pending(
            "ci",
            None,
            Some(CorePendingArgumentKind::MotionOrTextObject),
        ),
    );

    session.dispatch_key("w").expect("w should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert_eq!(session.snapshot().mode, CoreMode::Insert);
}

#[test]
fn sequential_dispatch_tracks_register_prefix_until_command_executes() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("paste target\n").expect("session should initialize");
    session.set_register('a', "from-a");

    session.dispatch_key("\"").expect("quote should succeed");
    assert_pending_state(
        &session,
        pending("\"", None, Some(CorePendingArgumentKind::Register)),
    );

    session
        .dispatch_key("a")
        .expect("register name should succeed");
    assert_pending_state(
        &session,
        pending("\"a", None, Some(CorePendingArgumentKind::NormalCommand)),
    );

    session.dispatch_key("p").expect("paste should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert!(session.snapshot().text.contains("from-a"));
}

#[test]
fn sequential_dispatch_tracks_mark_jump_prefix_until_mark_name_arrives() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("one\ntwo\nthree\n").expect("session should initialize");
    let current_buf_id = session.buffers()[0].id;
    session
        .set_mark('a', current_buf_id, 1, 0)
        .expect("mark setup should succeed");
    session
        .execute_normal_command("gg")
        .expect("cursor reset should succeed");

    session
        .dispatch_key("'")
        .expect("mark jump prefix should succeed");
    assert_pending_state(
        &session,
        pending("'", None, Some(CorePendingArgumentKind::MarkJump)),
    );

    session
        .dispatch_key("a")
        .expect("mark jump target should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert_eq!(session.snapshot().cursor_row, 1);
}

#[test]
fn sequential_dispatch_respects_insert_mode_literal_prefix_keys() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("session should initialize");

    session
        .dispatch_key("i")
        .expect("i should enter insert mode");
    assert_eq!(session.snapshot().mode, CoreMode::Insert);
    assert_eq!(session.pending_input(), CorePendingInput::none());

    session
        .dispatch_key("g")
        .expect("g should insert literally");
    assert_eq!(session.snapshot().mode, CoreMode::Insert);
    assert_eq!(session.snapshot().text, "g\n");
    assert_eq!(session.pending_input(), CorePendingInput::none());

    session
        .dispatch_key("\u{1b}")
        .expect("escape should succeed");
    assert_eq!(session.snapshot().mode, CoreMode::Normal);
    assert_eq!(session.snapshot().text, "g\n");
    assert_eq!(session.pending_input(), CorePendingInput::none());
}

#[test]
fn sequential_dispatch_respects_insert_mode_literal_digits() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("session should initialize");

    session
        .dispatch_key("i")
        .expect("i should enter insert mode");
    session
        .dispatch_key("2")
        .expect("2 should insert literally");

    assert_eq!(session.snapshot().mode, CoreMode::Insert);
    assert_eq!(session.snapshot().text, "2\n");
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert_eq!(session.snapshot().pending_input, CorePendingInput::none());
}

#[test]
fn sequential_dispatch_supports_counted_line_motion() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("one\ntwo\nthree\nfour\n").expect("session should initialize");

    session.dispatch_key("2").expect("count should succeed");
    assert_pending_state(&session, pending("2", Some(2), None));

    session.dispatch_key("j").expect("motion should succeed");
    assert_eq!(session.snapshot().cursor_row, 2);
    assert_eq!(session.pending_input(), CorePendingInput::none());
}

#[test]
fn sequential_dispatch_supports_counted_goto_line() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("one\ntwo\nthree\nfour\n").expect("session should initialize");

    session.dispatch_key("3").expect("count should succeed");
    assert_pending_state(&session, pending("3", Some(3), None));

    session.dispatch_key("G").expect("G should succeed");
    assert_eq!(session.snapshot().cursor_row, 2);
    assert_eq!(session.pending_input(), CorePendingInput::none());
}

#[test]
fn sequential_dispatch_supports_counted_gg_prefix_sequences() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("one\ntwo\nthree\nfour\n").expect("session should initialize");
    session
        .execute_normal_command("G")
        .expect("move to last line");
    assert_eq!(session.snapshot().cursor_row, 3);

    session.dispatch_key("2").expect("count should succeed");
    assert_pending_state(&session, pending("2", Some(2), None));

    session
        .dispatch_key("g")
        .expect("prefix should stay pending");
    assert_pending_state(&session, pending("2g", Some(2), None));

    session.dispatch_key("g").expect("sequence should execute");
    assert_eq!(session.snapshot().cursor_row, 1);
    assert_eq!(session.pending_input(), CorePendingInput::none());
}

#[test]
fn sequential_dispatch_supports_counted_operator_sequences() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("one two three four\n").expect("session should initialize");

    session.dispatch_key("2").expect("count should succeed");
    assert_pending_state(&session, pending("2", Some(2), None));

    session
        .dispatch_key("d")
        .expect("operator should stay pending");
    assert_pending_state(
        &session,
        pending(
            "2d",
            Some(2),
            Some(CorePendingArgumentKind::MotionOrTextObject),
        ),
    );

    session.dispatch_key("w").expect("motion should execute");
    assert_eq!(session.snapshot().text, "three four\n");
    assert_eq!(session.pending_input(), CorePendingInput::none());
}
