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
fn single_line_motion_keeps_topline_stable_after_screen_resize() {
    let _guard = acquire_session_test_lock();

    let text = (1..=20)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = VimCoreSession::new(&text).expect("session should initialize");
    session.set_screen_size(12, 80);

    let initial_snapshot = session.snapshot();
    let initial_win = &initial_snapshot.windows[0];
    assert_eq!(initial_win.topline, 1, "Initial topline should be 1");
    assert!(
        initial_win.botline > initial_win.topline,
        "Initial viewport should span multiple lines"
    );

    session.dispatch_key("j").expect("j should succeed");

    let moved_snapshot = session.snapshot();
    let moved_win = &moved_snapshot.windows[0];
    assert_eq!(
        moved_snapshot.cursor_row, 1,
        "Cursor should move down one line"
    );
    assert_eq!(
        moved_win.topline, 1,
        "Single-line motion should not scroll the viewport immediately"
    );
    assert!(
        moved_win.botline > moved_win.topline,
        "Viewport height should remain valid after single-line motion"
    );
}

#[test]
fn repeated_resize_keeps_topline_stable_after_single_line_motion() {
    let _guard = acquire_session_test_lock();

    let text = (1..=20)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = VimCoreSession::new(&text).expect("session should initialize");
    session.set_screen_size(24, 80);
    session.set_screen_size(18, 80);
    session.set_screen_size(12, 80);

    let initial_snapshot = session.snapshot();
    let initial_win = &initial_snapshot.windows[0];
    assert_eq!(initial_win.topline, 1, "Initial topline should be 1");

    session.dispatch_key("j").expect("j should succeed");

    let moved_snapshot = session.snapshot();
    let moved_win = &moved_snapshot.windows[0];
    assert_eq!(
        moved_snapshot.cursor_row, 1,
        "Cursor should move down one line"
    );
    assert_eq!(
        moved_win.topline, 1,
        "Repeated resize should not make single-line motion scroll the viewport"
    );
}

#[test]
fn split_window_keeps_topline_stable_after_single_line_motion() {
    let _guard = acquire_session_test_lock();

    let text = (1..=20)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = VimCoreSession::new(&text).expect("session should initialize");
    session.set_screen_size(12, 80);
    session
        .execute_ex_command(":split")
        .expect("split should succeed");

    let windows = session.windows();
    assert!(
        windows.iter().any(|window| window.is_active),
        "Split should leave an active window"
    );
    let active_before = windows
        .iter()
        .find(|window| window.is_active)
        .cloned()
        .expect("split should leave an active window");
    let snapshot_before = session.snapshot();
    let cursor_before = snapshot_before.cursor_row;

    session.dispatch_key("j").expect("j should succeed");

    let snapshot_after = session.snapshot();
    let active_after = session
        .windows()
        .iter()
        .find(|window| window.is_active)
        .cloned()
        .expect("split should still leave an active window");
    assert_eq!(
        snapshot_after.cursor_row,
        cursor_before + 1,
        "Single-line motion after split should move the cursor down one line"
    );
    assert_eq!(
        active_after.topline, active_before.topline,
        "Single-line motion after split should not scroll the active window"
    );
}

#[test]
fn vertical_and_horizontal_resize_keep_topline_stable_separately() {
    let _guard = acquire_session_test_lock();

    let text = (1..=20)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");

    {
        let mut session = VimCoreSession::new(&text).expect("session should initialize");
        session.set_screen_size(24, 80);
        session.set_screen_size(12, 80);

        session.dispatch_key("j").expect("j should succeed");

        let snapshot = session.snapshot();
        assert_eq!(
            snapshot.windows[0].topline, 1,
            "Vertical resize should not make single-line motion scroll the viewport"
        );
    }

    {
        let mut session = VimCoreSession::new(&text).expect("session should initialize");
        session.set_screen_size(24, 80);
        session.set_screen_size(24, 120);

        session.dispatch_key("j").expect("j should succeed");

        let snapshot = session.snapshot();
        assert_eq!(
            snapshot.windows[0].topline, 1,
            "Horizontal resize should not make single-line motion scroll the viewport"
        );
    }
}

#[test]
fn recreated_session_keeps_topline_stable_after_single_line_motion() {
    let _guard = acquire_session_test_lock();

    let text = (1..=20)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");

    {
        let mut session = VimCoreSession::new(&text).expect("session should initialize");
        session.set_screen_size(12, 80);
        session.set_screen_size(18, 80);
        session
            .execute_ex_command(":split")
            .expect("split should succeed");

        session.dispatch_key("j").expect("j should succeed");

        let snapshot = session.snapshot();
        assert_eq!(snapshot.cursor_row, 1);
        assert_eq!(
            snapshot
                .windows
                .iter()
                .find(|w| w.is_active)
                .unwrap()
                .topline,
            1
        );
    }

    {
        let mut session = VimCoreSession::new(&text).expect("session should initialize");
        session.set_screen_size(12, 80);
        let snapshot = session.snapshot();
        assert_eq!(
            snapshot
                .windows
                .iter()
                .find(|w| w.is_active)
                .unwrap()
                .topline,
            1,
            "A recreated session should start with a stable topline even after the previous session resized and split"
        );

        session.dispatch_key("j").expect("j should succeed");

        let snapshot = session.snapshot();
        assert_eq!(
            snapshot.cursor_row, 1,
            "Recreated session should still move the cursor down one line"
        );
        assert_eq!(
            snapshot
                .windows
                .iter()
                .find(|w| w.is_active)
                .unwrap()
                .topline,
            1,
            "Recreated session should keep the viewport stable"
        );
    }
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
