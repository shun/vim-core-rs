use std::sync::{Mutex, MutexGuard, OnceLock};
use vim_core_rs::{CorePendingInput, VimCoreSession};

fn session_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn acquire_session_test_lock() -> MutexGuard<'static, ()> {
    session_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn snapshot_exposes_pending_input_state() {
    let _guard = acquire_session_test_lock();
    let session = VimCoreSession::new("alpha beta gamma\n").expect("session should initialize");

    assert_eq!(session.pending_input(), CorePendingInput::None);
    assert_eq!(session.snapshot().pending_input, CorePendingInput::None);
}

#[test]
fn find_and_till_commands_report_char_pending_until_completed() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("alpha beta gamma\n").expect("session should initialize");

    session.apply_normal_command("f").expect("f should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::Char);
    assert_eq!(session.snapshot().pending_input, CorePendingInput::Char);

    session
        .apply_normal_command("b")
        .expect("pending find target should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::None);
    assert_eq!(session.snapshot().pending_input, CorePendingInput::None);

    session.apply_normal_command("0").expect("0 should succeed");
    session.apply_normal_command("t").expect("t should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::Char);

    session
        .apply_normal_command("g")
        .expect("pending till target should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::None);
}

#[test]
fn replace_mark_and_register_commands_report_specialized_pending_kinds() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("alpha beta gamma\n").expect("session should initialize");

    session.apply_normal_command("r").expect("r should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::Replace);
    assert_eq!(session.snapshot().pending_input, CorePendingInput::Replace);

    session
        .apply_normal_command("Z")
        .expect("replace target should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::None);

    session.apply_normal_command("m").expect("m should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::MarkSet);
    session
        .apply_normal_command("a")
        .expect("mark name should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::None);

    session
        .apply_normal_command("\"")
        .expect("\" should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::Register);
    session
        .apply_normal_command("a")
        .expect("register name should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::None);
}

#[test]
fn mark_jump_commands_report_pending_when_current_input_model_can_observe_them() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("one\ntwo\nthree\n").expect("session should initialize");

    session.apply_normal_command("'").expect("' should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::MarkJump);
    assert_eq!(session.snapshot().pending_input, CorePendingInput::MarkJump);

    session
        .apply_normal_command("a")
        .expect("mark jump target should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::None);

    session.apply_normal_command("`").expect("` should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::MarkJump);
    session
        .apply_normal_command("a")
        .expect("mark jump target should succeed");
    assert_eq!(session.pending_input(), CorePendingInput::None);
}

#[test]
fn fully_satisfied_command_does_not_leave_pending_input() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("alpha beta gamma\n").expect("session should initialize");

    session
        .apply_normal_command("fb")
        .expect("find command with target should succeed");

    assert_eq!(session.pending_input(), CorePendingInput::None);
    assert_eq!(session.snapshot().pending_input, CorePendingInput::None);
}
