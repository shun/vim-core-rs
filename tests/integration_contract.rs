use std::sync::{Mutex, OnceLock};
use vim_core_rs::{
    CoreCommandOutcome, CoreEvent, CoreHostAction, CoreMode, CorePendingArgumentKind,
    CorePendingInput, VimCoreSession,
};

fn pending(keys: &str, awaited_argument: Option<CorePendingArgumentKind>) -> CorePendingInput {
    CorePendingInput {
        pending_keys: keys.to_string(),
        awaited_argument,
    }
}

fn session_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn acquire_session_test_lock() -> std::sync::MutexGuard<'static, ()> {
    session_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn assert_host_action<F>(session: &mut VimCoreSession, command: &str, predicate: F)
where
    F: FnOnce(&CoreHostAction),
{
    let outcome = session
        .execute_ex_command(command)
        .unwrap_or_else(|_| panic!("Failed to apply command: {}", command));

    match outcome.outcome {
        CoreCommandOutcome::HostActionQueued => {
            let action = outcome
                .host_actions
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("Expected host action for command: {}", command));
            predicate(&action);
        }
        _ => panic!(
            "Expected HostActionQueued for command: {}, got {:?}",
            command, outcome.outcome
        ),
    }
}

fn assert_write_action(session: &mut VimCoreSession, command: &str, force: bool) {
    assert_host_action(session, command, |action| {
        if let CoreHostAction::Write {
            force: actual_force,
            ..
        } = action
        {
            assert_eq!(
                *actual_force, force,
                "unexpected force flag for command {command}"
            );
        } else {
            panic!("Expected Write action for {command}, got {:?}", action);
        }
    });
}

fn assert_quit_action(session: &mut VimCoreSession, command: &str, force: bool) {
    assert_host_action(session, command, |action| {
        if let CoreHostAction::Quit {
            force: actual_force,
            ..
        } = action
        {
            assert_eq!(
                *actual_force, force,
                "unexpected force flag for command {command}"
            );
        } else {
            panic!("Expected Quit action for {command}, got {:?}", action);
        }
    });
}

#[test]
fn side_effect_convergence_matrix() {
    let _guard = acquire_session_test_lock();

    let mut session = VimCoreSession::new("Original text").expect("Failed to initialize session");

    for command in ["write", "write!", "update", "up!"] {
        assert_write_action(&mut session, command, command.ends_with('!'));
    }

    for command in ["quit", "q", "quit!", "qa", "qa!"] {
        assert_quit_action(&mut session, command, command.ends_with('!'));
    }

    for command in ["wq", "wq!", "x", "xit"] {
        assert_quit_action(&mut session, command, command.ends_with('!'));
    }

    let redraw_tx = session
        .execute_ex_command("redraw")
        .expect("redraw should succeed");
    assert!(matches!(
        redraw_tx.events.as_slice(),
        [CoreEvent::Redraw {
            clear_before_draw: false,
            ..
        }]
    ));
    assert!(redraw_tx.host_actions.is_empty());

    let redraw_bang_tx = session
        .execute_ex_command("redraw!")
        .expect("redraw! should succeed");
    assert!(matches!(
        redraw_bang_tx.events.as_slice(),
        [CoreEvent::Redraw {
            clear_before_draw: true,
            ..
        }]
    ));
    assert!(redraw_bang_tx.host_actions.is_empty());

    let bell_tx = session
        .execute_ex_command("bell")
        .expect("bell should succeed");
    assert!(matches!(bell_tx.events.as_slice(), [CoreEvent::Bell]));
    assert!(bell_tx.host_actions.is_empty());

    assert_host_action(&mut session, "input Hello", |action| {
        if let CoreHostAction::RequestInput { prompt, .. } = action {
            assert_eq!(prompt, "Hello");
        } else {
            panic!("Expected RequestInput action, got {:?}", action);
        }
    });
}

#[test]
fn normal_mode_side_effects() {
    let _guard = acquire_session_test_lock();

    let mut session = VimCoreSession::new("Line 1\nLine 2").expect("Failed to initialize session");

    // ZZ is a normal command that saves and exits
    let outcome = session
        .execute_normal_command("ZZ")
        .expect("Failed to apply ZZ");

    assert!(matches!(
        outcome.outcome,
        CoreCommandOutcome::HostActionQueued
    ));
    let action = outcome.host_actions.first().expect("Expected action");
    assert!(matches!(action, CoreHostAction::Quit { .. }));

    let outcome = session
        .execute_normal_command("ZQ")
        .expect("Failed to apply ZQ");
    assert!(matches!(
        outcome.outcome,
        CoreCommandOutcome::HostActionQueued
    ));
    let action = outcome.host_actions.first().expect("Expected action");
    assert!(matches!(action, CoreHostAction::Quit { force: true, .. }));
}

#[test]
fn snapshot_and_session_state_apis_stay_consistent_within_one_session() {
    let _guard = acquire_session_test_lock();

    let mut session = VimCoreSession::new("alpha\nbravo\ncharlie\ndelta\n")
        .expect("Failed to initialize session");
    let current_buf_id = session
        .buffers()
        .into_iter()
        .find(|buffer| buffer.is_active)
        .expect("active buffer should exist")
        .id;

    session
        .execute_ex_command(":clearjumps")
        .expect("clearjumps should succeed");
    session
        .set_mark('a', current_buf_id, 3, 0)
        .expect("mark should be set from rust");

    let initial_snapshot = session.snapshot();
    assert_eq!(initial_snapshot.mode, CoreMode::Normal);
    assert_eq!(initial_snapshot.pending_input, CorePendingInput::none());
    assert_eq!(session.mode(), CoreMode::Normal);
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert_eq!(session.mark('a').expect("mark should be readable").row, 3);
    assert!(session.jumplist().entries.is_empty());

    session
        .execute_normal_command("'a")
        .expect("mark jump should succeed within one injection");
    let jumped_snapshot = session.snapshot();
    assert_eq!(jumped_snapshot.mode, CoreMode::Normal);
    assert_eq!(jumped_snapshot.pending_input, CorePendingInput::none());
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert_eq!(jumped_snapshot.cursor_row, 3);

    let jumplist_after_jump = session.jumplist();
    assert!(
        !jumplist_after_jump.entries.is_empty(),
        "mark jump should leave a navigable history entry: {jumplist_after_jump:?}"
    );
    assert_eq!(
        jumplist_after_jump.current_index,
        jumplist_after_jump.entries.len()
    );

    session
        .execute_normal_command("v")
        .expect("v should enter visual mode");
    let visual_snapshot = session.snapshot();
    assert_eq!(visual_snapshot.mode, CoreMode::Visual);
    assert_eq!(session.mode(), CoreMode::Visual);
    assert_eq!(visual_snapshot.pending_input, CorePendingInput::none());
    assert_eq!(session.pending_input(), CorePendingInput::none());
    assert_eq!(
        session.mark('a').expect("mark should stay available").row,
        3
    );

    session
        .execute_normal_command("\x1bR")
        .expect("escape then R should enter replace mode");
    assert_eq!(session.mode(), CoreMode::Replace);
    assert_eq!(session.snapshot().mode, CoreMode::Replace);
    assert_eq!(session.pending_input(), CorePendingInput::none());

    session
        .execute_normal_command("\x1bf")
        .expect("escape then f should enter char-pending state");
    assert_eq!(session.mode(), CoreMode::Normal);
    assert_eq!(
        session.pending_input(),
        pending("f", Some(CorePendingArgumentKind::Char))
    );
    assert_eq!(
        session.snapshot().pending_input,
        pending("f", Some(CorePendingArgumentKind::Char))
    );
    assert_eq!(
        session
            .mark('a')
            .expect("mark should survive pending state")
            .row,
        3
    );
    assert_eq!(session.jumplist(), jumplist_after_jump);
}
