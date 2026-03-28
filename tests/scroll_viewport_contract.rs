use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use vim_core_rs::{CoreSessionOptions, VimCoreSession};

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
        .execute_normal_command("50j")
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
fn page_scroll_back_restores_previous_topline() {
    let _guard = acquire_session_test_lock();

    let text = (1..=40)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = VimCoreSession::new(&text).expect("session should initialize");
    session.set_screen_size(12, 80);

    let initial_snapshot = session.snapshot();
    let initial_win = &initial_snapshot.windows[0];
    assert_eq!(initial_win.topline, 1, "Initial topline should be 1");

    session
        .dispatch_key("\u{6}")
        .expect("Ctrl-F should succeed");
    let forward_snapshot = session.snapshot();
    let forward_win = &forward_snapshot.windows[0];
    assert!(
        forward_win.topline > initial_win.topline,
        "Ctrl-F should advance topline"
    );

    session
        .dispatch_key("\u{2}")
        .expect("Ctrl-B should succeed");
    let backward_snapshot = session.snapshot();
    let backward_win = &backward_snapshot.windows[0];
    assert_eq!(
        backward_win.topline, initial_win.topline,
        "Ctrl-B should restore the previous topline"
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
        .execute_ex_command("set nowrap")
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
        .execute_normal_command("200l")
        .expect("200l should succeed");

    // Update layout automatically upon snapshot

    let far_scrolled_snapshot = session.snapshot();
    let far_scrolled_win = &far_scrolled_snapshot.windows[0];

    assert!(
        far_scrolled_win.leftcol > 0,
        "leftcol should increase after moving cursor far right with nowrap"
    );
}

#[test]
fn ctrl_f_and_ctrl_b_restore_the_previous_page_viewport() {
    let _guard = acquire_session_test_lock();

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let debug_log_path =
        std::env::temp_dir().join(format!("vim-core-rs-scroll-viewport-{nanos}.log"));

    let text = (1..=100)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = VimCoreSession::new_with_options(
        &text,
        CoreSessionOptions {
            debug_log_path: Some(debug_log_path.clone()),
            ..Default::default()
        },
    )
    .expect("session should initialize");

    let initial_snapshot = session.snapshot();
    let initial_window = &initial_snapshot.windows[0];
    let initial_topline = initial_window.topline;

    session
        .execute_normal_command("\x06")
        .expect("Ctrl+F should succeed");

    let forward_snapshot = session.snapshot();
    let forward_window = &forward_snapshot.windows[0];
    assert!(
        forward_window.topline > initial_topline,
        "Ctrl+F should advance topline: initial={}, forward={}",
        initial_topline,
        forward_window.topline
    );

    session
        .execute_normal_command("\x02")
        .expect("Ctrl+B should succeed");

    let backward_snapshot = session.snapshot();
    let backward_window = &backward_snapshot.windows[0];

    let debug_log = std::fs::read_to_string(&debug_log_path)
        .unwrap_or_else(|error| panic!("debug log should be readable: {error}"));
    eprintln!("[test] native log:\n{debug_log}");

    assert_eq!(
        backward_window.topline, initial_topline,
        "Ctrl+B should restore the original topline after a page forward"
    );
}
