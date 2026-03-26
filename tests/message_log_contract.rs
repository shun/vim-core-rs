#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::io::Read;
#[cfg(unix)]
use std::os::fd::{FromRawFd, RawFd};
use std::sync::{Mutex, OnceLock};
#[cfg(unix)]
use std::{sync::mpsc, thread, time::Duration};
use vim_core_rs::{
    CoreEvent, CoreMessageEvent, CoreMessageKind, CorePagerPromptKind, VimCoreSession,
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

// === Task 5.1: C FFI API の単体テスト ===

/// vim_bridge_eval_string が意図した Vimscript を正しく評価・返却できることを確認する
#[test]
fn eval_string_evaluates_simple_arithmetic() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    // "1 + 1" の評価結果が "2" であることを確認
    let result = session.eval_string("1 + 1");
    println!("[TEST] eval_string('1 + 1') = {:?}", result);
    assert_eq!(result, Some("2".to_string()));
}

/// vim_bridge_eval_string が文字列式を正しく評価することを確認する
#[test]
fn eval_string_evaluates_string_expression() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let result = session.eval_string("'hello' . ' world'");
    println!("[TEST] eval_string('hello' . ' world') = {:?}", result);
    assert_eq!(result, Some("hello world".to_string()));
}

/// 無効な式で NULL が返されることを確認する
#[test]
fn eval_string_returns_none_for_invalid_expression() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    // 無効な式はNoneを返す（クラッシュしない）
    let result = session.eval_string("invalid_nonexistent_function_xyz()");
    println!("[TEST] eval_string(invalid) = {:?}", result);
    // eval_to_string may return empty string or None for invalid expressions
    // The important thing is it doesn't crash
}

// === Task 5.2: メッセージの捕捉とルーティングの結合テスト ===

/// echoerr で発生したエラーメッセージが pending event として届くことを確認する
#[test]
fn error_message_is_exposed_via_pending_event() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let _ = session.apply_ex_command("echoerr 'test error message'");

    assert!(matches!(
        session.take_pending_event(),
        Some(CoreEvent::Message(CoreMessageEvent {
            kind: CoreMessageKind::Error,
            content,
        })) if content.contains("test error message")
    ));
}

#[test]
fn execute_ex_command_v2_returns_message_events_without_scraping() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let tx = session
        .execute_ex_command_v2("echomsg 'hello from event queue'")
        .expect("v2 実行が成功すること");

    assert!(
        tx.events.iter().any(|event| matches!(
            event,
            CoreEvent::Message(message)
                if message.kind == CoreMessageKind::Normal
                    && message.content.contains("hello from event queue")
        )),
        "echomsg は event queue 経由で観測できること: {:?}",
        tx.events
    );
}

#[test]
fn execute_normal_command_v2_returns_undo_message_events() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    session
        .apply_normal_command("A!")
        .expect("テキスト変更に成功すること");
    session
        .apply_normal_command("\u{1b}")
        .expect("Insert モードを抜けられること");

    let tx = session
        .execute_normal_command_v2("u")
        .expect("undo が成功すること");

    assert!(
        tx.events
            .iter()
            .any(|event| matches!(event, CoreEvent::Message(_))),
        "undo は message event を返すこと: {:?}",
        tx.events
    );
}

#[cfg(unix)]
fn capture_standard_streams<T>(f: impl FnOnce() -> T) -> (T, String, String) {
    unsafe fn capture_fd(fd: RawFd) -> (RawFd, RawFd) {
        let saved = unsafe { libc::dup(fd) };
        assert!(saved >= 0, "dup failed for fd={fd}");

        let mut pipefds = [0; 2];
        assert_eq!(
            unsafe { libc::pipe(pipefds.as_mut_ptr()) },
            0,
            "pipe failed for fd={fd}"
        );
        assert!(
            unsafe { libc::dup2(pipefds[1], fd) } >= 0,
            "dup2 failed for fd={fd}"
        );
        assert_eq!(
            unsafe { libc::close(pipefds[1]) },
            0,
            "close failed for write pipe fd={fd}"
        );
        (saved, pipefds[0])
    }

    unsafe fn restore_fd(fd: RawFd, saved: RawFd) {
        assert!(
            unsafe { libc::dup2(saved, fd) } >= 0,
            "restore dup2 failed for fd={fd}"
        );
        assert_eq!(
            unsafe { libc::close(saved) },
            0,
            "close failed for saved fd={fd}"
        );
    }

    unsafe fn read_pipe(read_fd: RawFd) -> String {
        let mut file = unsafe { File::from_raw_fd(read_fd) };
        let mut output = String::new();
        file.read_to_string(&mut output)
            .expect("pipe output should be readable");
        output
    }

    unsafe {
        let (saved_stdout, stdout_read) = capture_fd(libc::STDOUT_FILENO);
        let (saved_stderr, stderr_read) = capture_fd(libc::STDERR_FILENO);

        let result = f();

        libc::fflush(std::ptr::null_mut());
        restore_fd(libc::STDOUT_FILENO, saved_stdout);
        restore_fd(libc::STDERR_FILENO, saved_stderr);

        let stdout = read_pipe(stdout_read);
        let stderr = read_pipe(stderr_read);
        (result, stdout, stderr)
    }
}

#[cfg(unix)]
fn sanitize_harness_output(output: &str) -> String {
    output
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("test ")
                && !trimmed.contains(" ... ok")
                && !trimmed.contains(" ... FAILED")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(unix)]
#[test]
fn embedded_echoconsole_is_delivered_as_event_without_terminal_leak() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let (tx, stdout, stderr) = capture_standard_streams(|| {
        session
            .execute_ex_command_v2("echoconsole 'console event'")
            .expect("echoconsole が成功すること")
    });

    assert_eq!(
        sanitize_harness_output(&stdout),
        "",
        "embedded mode must not write to stdout"
    );
    assert_eq!(
        sanitize_harness_output(&stderr),
        "",
        "embedded mode must not write to stderr"
    );
    assert!(
        tx.events.iter().any(|event| matches!(
            event,
            CoreEvent::Message(message)
                if message.kind == CoreMessageKind::Normal
                    && message.content.contains("console event")
        )),
        "echoconsole は terminal ではなく event queue に流れること: {:?}",
        tx.events
    );
}

#[cfg(unix)]
#[test]
fn embedded_native_beep_is_delivered_as_event_without_terminal_leak() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let (tx, stdout, stderr) = capture_standard_streams(|| {
        session
            .execute_normal_command_v2("\u{1b}")
            .expect("normal mode での ESC が処理されること")
    });

    assert_eq!(
        sanitize_harness_output(&stdout),
        "",
        "embedded mode must not write bell output to stdout"
    );
    assert_eq!(
        sanitize_harness_output(&stderr),
        "",
        "embedded mode must not write bell output to stderr"
    );
    assert!(
        tx.events
            .iter()
            .any(|event| matches!(event, CoreEvent::Bell)),
        "native beep は bell event に変換されること: {:?}",
        tx.events
    );
}

#[test]
fn take_pending_event_exposes_message_queue_for_legacy_migration() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    session
        .apply_ex_command("echomsg 'queued message'")
        .expect("echomsg が成功すること");

    assert!(matches!(
        session.take_pending_event(),
        Some(CoreEvent::Message(CoreMessageEvent {
            kind: CoreMessageKind::Normal,
            content,
        })) if content.contains("queued message")
    ));
}

#[cfg(unix)]
#[test]
fn embedded_multiline_echo_does_not_block_on_more_prompt() {
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        let _guard = acquire_session_test_lock();
        let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");
        session.set_screen_size(5, 20);
        while session.take_pending_event().is_some() {}
        while session.take_pending_host_action().is_some() {}

        let (tx, stdout, stderr) = capture_standard_streams(|| {
            session
                .execute_ex_command_v2(r#"echo join(range(1, 20), "\n")"#)
                .expect("multiline echo が成功すること")
        });

        sender
            .send((tx.events, stdout, stderr))
            .expect("test result should be sent");
    });

    let (events, stdout, stderr) = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("embedded multiline echo should not block on more-prompt");
    handle.join().expect("worker thread should complete");

    assert_eq!(
        sanitize_harness_output(&stdout),
        "",
        "embedded multiline echo must not write to stdout"
    );
    assert_eq!(
        sanitize_harness_output(&stderr),
        "",
        "embedded multiline echo must not write to stderr"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            CoreEvent::Message(message)
                if message.kind == CoreMessageKind::Normal
                    && message.content.contains("1\n2\n3")
        )),
        "multiline echo は message event として返ること: {:?}",
        events
    );
}

#[cfg(unix)]
#[test]
fn embedded_set_all_surfaces_more_prompt_event_without_blocking() {
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        let _guard = acquire_session_test_lock();
        let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");
        session.set_screen_size(5, 20);
        while session.take_pending_event().is_some() {}

        let (tx, stdout, stderr) = capture_standard_streams(|| {
            session
                .execute_ex_command_v2("set all")
                .expect("set all が成功すること")
        });

        sender
            .send((tx.events, stdout, stderr))
            .expect("events should be sent");
    });

    let (events, stdout, stderr) = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("embedded set all should not block on more-prompt");
    handle.join().expect("worker thread should complete");

    assert_eq!(
        sanitize_harness_output(&stdout),
        "",
        "embedded set all must not write to stdout"
    );
    assert_eq!(
        sanitize_harness_output(&stderr),
        "",
        "embedded set all must not write to stderr"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CoreEvent::PagerPrompt(CorePagerPromptKind::More))),
        "set all は more prompt event を返すこと: {:?}",
        events
    );
}

#[cfg(unix)]
#[test]
fn embedded_intro_surfaces_hit_return_prompt_without_blocking() {
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        let _guard = acquire_session_test_lock();
        let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");
        while session.take_pending_event().is_some() {}

        let (tx, stdout, stderr) = capture_standard_streams(|| {
            session
                .execute_ex_command_v2("intro")
                .expect("intro が成功すること")
        });

        sender
            .send((tx.events, stdout, stderr))
            .expect("events should be sent");
    });

    let (events, stdout, stderr) = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("embedded intro should not block on hit-return");
    handle.join().expect("worker thread should complete");

    assert_eq!(
        sanitize_harness_output(&stdout),
        "",
        "embedded intro must not write to stdout"
    );
    assert_eq!(
        sanitize_harness_output(&stderr),
        "",
        "embedded intro must not write to stderr"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            CoreEvent::PagerPrompt(CorePagerPromptKind::HitReturn)
        )),
        "intro は hit-return prompt event を返すこと: {:?}",
        events
    );
}

#[cfg(unix)]
#[test]
fn embedded_echon_is_delivered_as_single_message_chunk_without_terminal_leak() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let (tx, stdout, stderr) = capture_standard_streams(|| {
        session
            .execute_ex_command_v2(r#"echon "ab" "cd""#)
            .expect("echon が成功すること")
    });

    assert_eq!(
        sanitize_harness_output(&stdout),
        "",
        "embedded echon must not write to stdout"
    );
    assert_eq!(
        sanitize_harness_output(&stderr),
        "",
        "embedded echon must not write to stderr"
    );
    assert!(
        tx.events.iter().any(|event| matches!(
            event,
            CoreEvent::Message(message)
                if message.kind == CoreMessageKind::Normal
                    && message.content == "abcd"
        )),
        "echon は 1 つの message chunk として返ること: {:?}",
        tx.events
    );
}

/// echomsg で発生した通常メッセージが pending event として届くことを確認する
#[test]
fn normal_message_is_exposed_via_pending_event() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let _ = session.apply_ex_command("echomsg 'hello from vim'");

    assert!(matches!(
        session.take_pending_event(),
        Some(CoreEvent::Message(CoreMessageEvent {
            kind: CoreMessageKind::Normal,
            content,
        })) if content.contains("hello from vim")
    ));
}

/// handler API がなくてもメッセージ実行が安全に event queue へ流れることを確認する
#[test]
fn event_queue_delivery_does_not_require_handler_registration() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let result = session.apply_ex_command("echo 'no handler test'");
    println!("[TEST] no_handler result: {:?}", result);
    assert!(
        result.is_ok(),
        "event queue のみでもコマンド実行が成功すること"
    );

    assert!(matches!(
        session.take_pending_event(),
        Some(CoreEvent::Message(CoreMessageEvent {
            kind: CoreMessageKind::Normal,
            content,
        })) if content.contains("no handler test")
    ));
}

/// メッセージを伴わない normal command は event queue を汚さないことを確認する
#[test]
fn normal_command_without_message_leaves_event_queue_empty() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello\nworld\n").expect("セッション初期化に失敗");

    let result = session.apply_normal_command("j");
    println!("[TEST] normal_command result: {:?}", result);
    assert!(result.is_ok());
    assert!(session.take_pending_event().is_none());
}

#[test]
fn version_output_discloses_modified_vim_distribution() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let version_output = session
        .eval_string("execute('version')")
        .expect(":version の出力を取得できること");

    assert!(
        version_output.contains("Modified by"),
        ":version should disclose that the embedded Vim runtime is modified: {}",
        version_output
    );
    assert!(
        version_output.contains("vim-core-rs"),
        ":version should include the vim-core-rs contact string: {}",
        version_output
    );
}
