use std::sync::{Mutex, OnceLock};
use vim_core_rs::VimCoreSession;

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
fn vertical_scroll_updates_viewport_boundaries() {
    let _guard = acquire_session_test_lock();

    // Create 100 lines
    let text = (1..=100)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = VimCoreSession::new(&text).expect("session should initialize");

    // Update layout automatically upon snapshot

    let initial_snapshot = session.snapshot();
    assert!(
        !initial_snapshot.windows.is_empty(),
        "Window list should not be empty"
    );
    let initial_win = &initial_snapshot.windows[0];

    assert_eq!(initial_win.topline, 1, "Initial topline should be 1");
    let initial_botline = initial_win.botline;
    assert!(
        initial_botline > 1 && initial_botline <= 100,
        "botline should be > 1"
    );

    // Move cursor down by 50 lines which forces a scroll
    session
        .apply_normal_command("50j")
        .expect("50j should succeed");

    // Update layout automatically upon snapshot

    let scrolled_snapshot = session.snapshot();
    let scrolled_win = &scrolled_snapshot.windows[0];

    assert!(
        scrolled_win.topline > 1,
        "topline should be > 1 after scrolling down"
    );
    assert!(
        scrolled_win.botline > initial_botline,
        "botline should be advanced"
    );
}

#[test]
fn horizontal_scroll_updates_viewport_boundaries() {
    let _guard = acquire_session_test_lock();

    // Create 1 line with 1000 characters
    let long_line = "a".repeat(1000);
    let mut session = VimCoreSession::new(&long_line).expect("session should initialize");

    // Turn off wrap so horizontal scroll occurs
    session
        .apply_ex_command("set nowrap")
        .expect("set nowrap should succeed");

    // Update layout automatically upon snapshot

    let initial_snapshot = session.snapshot();
    assert!(
        !initial_snapshot.windows.is_empty(),
        "Window list should not be empty"
    );
    let initial_win = &initial_snapshot.windows[0];

    assert_eq!(initial_win.leftcol, 0, "Initial leftcol should be 0");

    // Move cursor right by 200 columns
    session
        .apply_normal_command("200l")
        .expect("200l should succeed");

    // Update layout automatically upon snapshot

    let far_scrolled_snapshot = session.snapshot();
    let far_scrolled_win = &far_scrolled_snapshot.windows[0];

    assert!(
        far_scrolled_win.leftcol > 0,
        "leftcol should increase after moving cursor far right with nowrap"
    );
}
