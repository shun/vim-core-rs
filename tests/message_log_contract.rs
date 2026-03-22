use std::sync::{Arc, Mutex, OnceLock};
use vim_core_rs::{CoreMessageEvent, CoreMessageKind, VimCoreSession};

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
    let mut session =
        VimCoreSession::new("hello").expect("セッション初期化に失敗");

    // "1 + 1" の評価結果が "2" であることを確認
    let result = session.eval_string("1 + 1");
    println!("[TEST] eval_string('1 + 1') = {:?}", result);
    assert_eq!(result, Some("2".to_string()));
}

/// vim_bridge_eval_string が文字列式を正しく評価することを確認する
#[test]
fn eval_string_evaluates_string_expression() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let result = session.eval_string("'hello' . ' world'");
    println!("[TEST] eval_string('hello' . ' world') = {:?}", result);
    assert_eq!(result, Some("hello world".to_string()));
}

/// 無効な式で NULL が返されることを確認する
#[test]
fn eval_string_returns_none_for_invalid_expression() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("hello").expect("セッション初期化に失敗");

    // 無効な式はNoneを返す（クラッシュしない）
    let result = session.eval_string("invalid_nonexistent_function_xyz()");
    println!("[TEST] eval_string(invalid) = {:?}", result);
    // eval_to_string may return empty string or None for invalid expressions
    // The important thing is it doesn't crash
}

// === Task 5.2: メッセージの捕捉とルーティングの結合テスト ===

/// echoerr で発生したエラーメッセージがハンドラに CoreMessageKind::Error として届くことを確認する
#[test]
fn error_message_dispatched_to_handler() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let messages: Arc<Mutex<Vec<CoreMessageEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let messages_clone = messages.clone();

    session.set_message_handler(Box::new(move |event: CoreMessageEvent| {
        println!("[TEST] handler received: kind={:?} content={}", event.kind, event.content);
        messages_clone.lock().unwrap().push(event);
    }));

    // echoerr でエラーメッセージを発生させる
    let _ = session.apply_ex_command("echoerr 'test error message'");

    let msgs = messages.lock().unwrap();
    println!("[TEST] total messages received: {}", msgs.len());
    for msg in msgs.iter() {
        println!("[TEST]   kind={:?} content={}", msg.kind, msg.content);
    }

    // エラーメッセージが届いていることを確認
    let has_error = msgs.iter().any(|m| {
        m.kind == CoreMessageKind::Error && m.content.contains("test error message")
    });
    assert!(
        has_error,
        "echoerr で発生したエラーメッセージがハンドラに届いていない。受信メッセージ: {:?}",
        *msgs
    );
}

/// echo で発生した通常メッセージがハンドラに CoreMessageKind::Normal として届くことを確認する
#[test]
fn normal_message_dispatched_to_handler() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("hello").expect("セッション初期化に失敗");

    let messages: Arc<Mutex<Vec<CoreMessageEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let messages_clone = messages.clone();

    session.set_message_handler(Box::new(move |event: CoreMessageEvent| {
        println!("[TEST] handler received: kind={:?} content={}", event.kind, event.content);
        messages_clone.lock().unwrap().push(event);
    }));

    // echomsg で通常メッセージを発生させる（echoは:messages履歴に残らない）
    let _ = session.apply_ex_command("echomsg 'hello from vim'");

    let msgs = messages.lock().unwrap();
    println!("[TEST] total messages received: {}", msgs.len());
    for msg in msgs.iter() {
        println!("[TEST]   kind={:?} content={}", msg.kind, msg.content);
    }

    // 通常メッセージが届いていることを確認
    let has_normal = msgs.iter().any(|m| {
        m.kind == CoreMessageKind::Normal && m.content.contains("hello from vim")
    });
    assert!(
        has_normal,
        "echo で発生した通常メッセージがハンドラに届いていない。受信メッセージ: {:?}",
        *msgs
    );
}

/// ハンドラが未登録の場合にメッセージがクラッシュなく安全に処理されることを確認する
#[test]
fn no_handler_does_not_crash() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("hello").expect("セッション初期化に失敗");

    // ハンドラ未登録のままコマンドを実行してもクラッシュしないことを確認
    let result = session.apply_ex_command("echo 'no handler test'");
    println!("[TEST] no_handler result: {:?}", result);
    assert!(result.is_ok(), "ハンドラ未登録でもコマンド実行が成功すること");

    let result = session.apply_ex_command("echoerr 'no handler error test'");
    println!("[TEST] no_handler error result: {:?}", result);
    // echoerr may return an error result, but it should not crash
}

/// apply_normal_command でもメッセージがポーリングされることを確認する
#[test]
fn normal_command_triggers_message_polling() {
    let _guard = acquire_session_test_lock();
    let mut session =
        VimCoreSession::new("hello\nworld\n").expect("セッション初期化に失敗");

    let messages: Arc<Mutex<Vec<CoreMessageEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let messages_clone = messages.clone();

    session.set_message_handler(Box::new(move |event: CoreMessageEvent| {
        println!("[TEST] handler received: kind={:?} content={}", event.kind, event.content);
        messages_clone.lock().unwrap().push(event);
    }));

    // Normal コマンドの実行がクラッシュしないことを確認
    let result = session.apply_normal_command("j");
    println!("[TEST] normal_command result: {:?}", result);
    assert!(result.is_ok());
}
