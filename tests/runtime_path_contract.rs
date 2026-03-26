use std::fs;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;
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
fn runtimepath_contract_suite() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");

    // 1. Verify default runtimepath from pathdef.c
    session
        .execute_ex_command("put =&runtimepath")
        .expect("failed to put runtimepath");
    let snapshot1 = session.snapshot();
    let rtp = snapshot1.text.trim();
    println!("Initial runtimepath: {}", rtp);
    assert!(!rtp.is_empty(), "runtimepath should not be empty");
    assert!(
        rtp.contains("share/vim"),
        "runtimepath should contain 'share/vim'"
    );

    // 2. Verify we can add a new path and load a plugin from it
    let tmp_dir = tempdir().expect("failed to create temp dir");
    let plugin_dir = tmp_dir.path().join("plugin");
    fs::create_dir_all(&plugin_dir).expect("failed to create plugin dir");
    fs::write(plugin_dir.join("test.vim"), "let g:test_loaded = 1")
        .expect("failed to write plugin");

    // Clear buffer for next check
    session
        .execute_ex_command("%d")
        .expect("failed to clear buffer");

    let set_rtp = format!("let &rtp .= ',' . '{}'", tmp_dir.path().display());
    session
        .execute_ex_command(&set_rtp)
        .expect("failed to set rtp");

    session
        .execute_ex_command("runtime! plugin/test.vim")
        .expect("failed to run :runtime");
    session
        .execute_ex_command("if exists('g:test_loaded') | put ='LOADED' | endif")
        .expect("failed to check var");

    assert!(
        session.snapshot().text.contains("LOADED"),
        "Plugin should be loaded"
    );

    // 3. Verify $VIM
    session
        .execute_ex_command("%d")
        .expect("failed to clear buffer");
    session
        .execute_ex_command("put =$VIM")
        .expect("failed to put $VIM");
    let snapshot2 = session.snapshot();
    let vim_val = snapshot2.text.trim();
    assert!(
        vim_val.contains("share/vim"),
        "$VIM should point to share/vim"
    );
}
