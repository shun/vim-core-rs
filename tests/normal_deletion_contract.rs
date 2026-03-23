use std::sync::{Mutex, OnceLock};
use vim_core_rs::{CoreCommandOutcome, CoreMode, VimCoreSession};

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
fn normal_dd_command_deletes_line_and_updates_cursor_and_revision() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("line 1\nline 2\nline 3\n").expect("session should initialize");

    // Move to second line
    session.apply_normal_command("j").expect("j should succeed");

    let snapshot_before = session.snapshot();
    assert_eq!(snapshot_before.cursor_row, 1);
    assert_eq!(snapshot_before.cursor_col, 0);
    assert_eq!(snapshot_before.revision, 0);

    // Delete second line
    let outcome = session
        .apply_normal_command("dd")
        .expect("dd should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::BufferChanged { revision: 1 }
    ));

    let snapshot = session.snapshot();
    // "line 2\n" is deleted, "line 3\n" moves up to row 1.
    // In Vim, when 'dd' is called on middle line, cursor usually stays on the same row (now containing next line)
    assert_eq!(snapshot.text, "line 1\nline 3\n");
    assert_eq!(snapshot.revision, 1);
    assert!(snapshot.dirty);
    assert_eq!(snapshot.mode, CoreMode::Normal);
    assert_eq!(snapshot.cursor_row, 1);
    assert_eq!(snapshot.cursor_col, 0);
}

#[test]
fn normal_dw_command_deletes_word_and_updates_cursor() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello world\n").expect("session should initialize");

    // Delete "hello "
    let outcome = session
        .apply_normal_command("dw")
        .expect("dw should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::BufferChanged { revision: 1 }
    ));

    let snapshot = session.snapshot();
    assert_eq!(snapshot.text, "world\n");
    assert_eq!(snapshot.cursor_row, 0);
    assert_eq!(snapshot.cursor_col, 0);
}

#[test]
fn normal_x_command_deletes_char_under_cursor() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("abc\n").expect("session should initialize");

    // Move to 'b'
    session.apply_normal_command("l").expect("l should succeed");

    let outcome = session.apply_normal_command("x").expect("x should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::BufferChanged { revision: 1 }
    ));

    let snapshot = session.snapshot();
    assert_eq!(snapshot.text, "ac\n");
    assert_eq!(snapshot.cursor_row, 0);
    assert_eq!(snapshot.cursor_col, 1); // cursor should be on 'c'
}

#[test]
fn normal_d_dollar_command_deletes_to_end_of_line() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello world\n").expect("session should initialize");

    // Move to 'w'
    session
        .apply_normal_command("6l")
        .expect("6l should succeed");

    let outcome = session
        .apply_normal_command("d$")
        .expect("d$ should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::BufferChanged { revision: 1 }
    ));

    let snapshot = session.snapshot();
    assert_eq!(snapshot.text, "hello \n");
    assert_eq!(snapshot.cursor_row, 0);
    assert_eq!(snapshot.cursor_col, 5); // cursor should be on space (or last char)
}

#[test]
fn multiple_deletions_increment_revision_monotonically() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("1\n2\n3\n4\n").expect("session should initialize");

    assert_eq!(session.snapshot().revision, 0);

    session.apply_normal_command("dd").expect("1st dd");
    assert_eq!(session.snapshot().revision, 1);
    assert_eq!(session.snapshot().text, "2\n3\n4\n");

    session.apply_normal_command("dd").expect("2nd dd");
    assert_eq!(session.snapshot().revision, 2);
    assert_eq!(session.snapshot().text, "3\n4\n");

    session.apply_normal_command("x").expect("x");
    assert_eq!(session.snapshot().revision, 3);
    assert_eq!(session.snapshot().text, "\n4\n");
}

#[test]
fn deleting_last_line_moves_cursor_up() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("line 1\nline 2\n").expect("session should initialize");

    // Move to last line
    session.apply_normal_command("G").expect("G should succeed");
    assert_eq!(session.snapshot().cursor_row, 1);

    // Delete last line
    session
        .apply_normal_command("dd")
        .expect("dd should succeed");

    let snapshot = session.snapshot();
    assert_eq!(snapshot.text, "line 1\n");
    assert_eq!(snapshot.cursor_row, 0); // Cursor should move up to the new last line
}
