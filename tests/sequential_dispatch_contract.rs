use std::sync::{Mutex, MutexGuard, OnceLock};
use vim_core_rs::{CorePendingArgumentKind, CorePendingInput, VimCoreSession};

fn session_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn acquire_session_test_lock() -> MutexGuard<'static, ()> {
    session_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn pending(keys: &str, awaited_argument: Option<CorePendingArgumentKind>) -> CorePendingInput {
    CorePendingInput {
        pending_keys: keys.to_string(),
        awaited_argument,
    }
}

#[test]
fn sequential_dispatch_handles_gg_and_reports_pending_prefix() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("one\ntwo\nthree\n").expect("session should initialize");

    session.dispatch_key("g").expect("first g should succeed");
    assert_eq!(session.pending_input(), pending("g", None));
    assert_eq!(session.snapshot().pending_input, pending("g", None));

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
    assert_eq!(
        session.pending_input(),
        pending("d", Some(CorePendingArgumentKind::MotionOrTextObject))
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
    assert_eq!(
        session.pending_input(),
        pending("c", Some(CorePendingArgumentKind::MotionOrTextObject))
    );

    session.dispatch_key("i").expect("i should succeed");
    assert_eq!(
        session.pending_input(),
        pending("ci", Some(CorePendingArgumentKind::MotionOrTextObject))
    );

    session.dispatch_key("w").expect("w should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::none());
}

#[test]
fn sequential_dispatch_tracks_register_prefix_until_command_executes() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("paste target\n").expect("session should initialize");
    session.set_register('a', "from-a");

    session.dispatch_key("\"").expect("quote should succeed");
    assert_eq!(
        session.pending_input(),
        pending("\"", Some(CorePendingArgumentKind::Register))
    );

    session
        .dispatch_key("a")
        .expect("register name should succeed");
    assert_eq!(
        session.pending_input(),
        pending("\"a", Some(CorePendingArgumentKind::NormalCommand))
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
    assert_eq!(
        session.pending_input(),
        pending("'", Some(CorePendingArgumentKind::MarkJump))
    );

    session
        .dispatch_key("a")
        .expect("mark jump target should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert_eq!(session.snapshot().cursor_row, 1);
}
