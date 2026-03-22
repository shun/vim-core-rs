use std::fs;
use std::sync::{Mutex, OnceLock};
use vim_core_rs::{
    CoreHostAction, VimCoreSession, CoreCommandOutcome,
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
fn ex_write_does_not_create_file_on_disk() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    let test_file = "Xtest_write_not_created.txt";
    if fs::metadata(test_file).is_ok() {
        fs::remove_file(test_file).ok();
    }

    let outcome = session
        .apply_ex_command(&format!(":write {}", test_file))
        .expect("write command should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::HostActionQueued
    ));

    // Verify host action is queued with correct path
    let action = session.take_pending_host_action().expect("host action should be queued");
    if let CoreHostAction::Write { path, force, .. } = action {
        assert_eq!(path, test_file);
        assert!(!force);
    } else {
        panic!("Expected Write action, got {:?}", action);
    }

    // Verify file was NOT created on disk
    assert!(!fs::metadata(test_file).is_ok(), "File should NOT be created on disk by Vim runtime");
}

#[test]
fn ex_write_bang_queues_force_write_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    let test_file = "Xtest_write_bang.txt";
    let outcome = session
        .apply_ex_command(&format!(":write! {}", test_file))
        .expect("write! command should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::HostActionQueued
    ));

    let action = session.take_pending_host_action().expect("host action should be queued");
    if let CoreHostAction::Write { path, force, .. } = action {
        assert_eq!(path, test_file);
        assert!(force);
    } else {
        panic!("Expected Write action, got {:?}", action);
    }
}

#[test]
fn ex_write_no_filename_queues_empty_path_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    let outcome = session
        .apply_ex_command(":write")
        .expect("write command should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::HostActionQueued
    ));

    let action = session.take_pending_host_action().expect("host action should be queued");
    if let CoreHostAction::Write { path, .. } = action {
        assert_eq!(path, "");
    } else {
        panic!("Expected Write action, got {:?}", action);
    }
}

#[test]
fn ex_update_is_intercepted_as_write_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    // :update はブリッジ層で常に write としてインターセプトされる
    // dirty 判定はホスト（Rust）側の責務
    let test_file = "Xtest_update_intercepted.txt";
    let outcome = session
        .apply_ex_command(&format!(":update {}", test_file))
        .expect("update command should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::HostActionQueued
    ));

    let action = session.take_pending_host_action().expect("host action should be queued");
    if let CoreHostAction::Write { path, .. } = action {
        assert_eq!(path, test_file);
    } else {
        panic!("Expected Write action (from update), got {:?}", action);
    }
}

#[test]
fn ex_quit_bang_queues_force_quit_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    let outcome = session
        .apply_ex_command(":quit!")
        .expect("quit! command should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::HostActionQueued
    ));

    let action = session.take_pending_host_action().expect("host action should be queued");
    if let CoreHostAction::Quit { force, .. } = action {
        assert!(force);
    } else {
        panic!("Expected Quit action, got {:?}", action);
    }
}

#[test]
fn ex_wq_is_intercepted_as_quit_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    let outcome = session
        .apply_ex_command(":wq")
        .expect("wq command should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::HostActionQueued
    ));

    let action = session.take_pending_host_action().expect("host action should be queued");
    assert!(matches!(action, CoreHostAction::Quit { .. }));
}

#[test]
fn ex_xit_is_intercepted_as_quit_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    let outcome = session
        .apply_ex_command(":x")
        .expect("xit command should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::HostActionQueued
    ));

    let action = session.take_pending_host_action().expect("host action should be queued");
    assert!(matches!(action, CoreHostAction::Quit { .. }));
}

#[test]
fn ex_redraw_bang_queues_correct_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    let outcome = session
        .apply_ex_command(":redraw!")
        .expect("redraw! command should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::HostActionQueued
    ));

    let action = session.take_pending_host_action().expect("host action should be queued");
    assert_eq!(
        action,
        CoreHostAction::Redraw {
            full: true,
            clear_before_draw: true,
        }
    );
}

#[test]
fn ex_redraw_short_form_queues_action() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    let outcome = session
        .apply_ex_command(":redr")
        .expect("redr command should succeed");

    assert!(matches!(
        outcome,
        CoreCommandOutcome::HostActionQueued
    ));

    let action = session.take_pending_host_action().expect("host action should be queued");
    assert_eq!(
        action,
        CoreHostAction::Redraw {
            full: true,
            clear_before_draw: false,
        }
    );
}

#[test]
fn multiple_ex_commands_can_queue_multiple_actions() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    session.apply_ex_command(":write file1.txt").unwrap();
    session.apply_ex_command(":redraw").unwrap();
    session.apply_ex_command(":quit").unwrap();

    assert_eq!(session.snapshot().pending_host_actions, 3);

    assert!(matches!(session.take_pending_host_action(), Some(CoreHostAction::Write { .. })));
    assert!(matches!(session.take_pending_host_action(), Some(CoreHostAction::Redraw { .. })));
    assert!(matches!(session.take_pending_host_action(), Some(CoreHostAction::Quit { .. })));
    assert!(session.take_pending_host_action().is_none());
}

#[test]
fn ex_compound_command_with_write_is_not_intercepted_yet() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("some content").expect("session should initialize");
    
    let test_file = "Xtest_compound_write.txt";
    if fs::metadata(test_file).is_ok() {
        fs::remove_file(test_file).ok();
    }

    // CURRENT LIMITATION: Compound commands starting with non-intercepted keywords
    // will bypass the bridge interception and might perform actual I/O.
    let _outcome = session
        .apply_ex_command(&format!(":set number | write! {}", test_file))
        .expect("compound command should succeed");

    let file_exists = fs::metadata(test_file).is_ok();

    if file_exists {
        fs::remove_file(test_file).ok();
        // compound コマンド内の write は bridge interception を bypass する。
        // Write ホストアクションは出ないが、autocommand 由来のイベント
        // (BufAdd 等) がキューされることはある。
        while let Some(action) = session.take_pending_host_action() {
            assert!(
                !matches!(action, CoreHostAction::Write { .. }),
                "Write action should have bypassed bridge interception, got {:?}",
                action
            );
        }
    }
}
