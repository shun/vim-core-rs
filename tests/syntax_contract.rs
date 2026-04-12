use std::sync::{Mutex, MutexGuard, OnceLock};
use vim_core_rs::VimCoreSession;

fn session_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn acquire_session_test_lock() -> MutexGuard<'static, ()> {
    session_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// Upstream: test_conceal.vim
#[test]
fn conceal_parity_from_upstream_test_conceal() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("session should initialize");
    let win_id = session.windows()[0].id;

    session
        .execute_ex_command("syntax on")
        .expect("syntax should be enabled");
    session
        .execute_ex_command("syntax clear")
        .expect("syntax should be cleared before adding local rules");
    session
        .execute_ex_command("setlocal conceallevel=2 concealcursor=n")
        .expect("conceal options should be set");
    session
        .execute_ex_command("call setline(1, ['aaaasecretbbbb', 'ccccsecretdddd'])")
        .expect("buffer lines should be installed");

    session
        .execute_ex_command("syntax match UpstreamSyntaxConceal /\\%1lsecret/ conceal cchar=X")
        .expect("syntax conceal rule should be installed");

    let chunks = session
        .get_line_syntax(win_id, 1)
        .expect("line syntax should be available");
    let chunk_summary = chunks
        .iter()
        .map(|chunk| (chunk.start_col, chunk.end_col, chunk.name.as_deref()))
        .collect::<Vec<_>>();
    assert_eq!(
        chunk_summary,
        vec![
            (0, 4, None),
            (4, 10, Some("UpstreamSyntaxConceal")),
            (10, 14, None),
        ]
    );

    assert_eq!(
        session
            .eval_string("string(synconcealed(1, 5)[0])")
            .as_deref(),
        Some("1")
    );
    assert_eq!(
        session
            .eval_string("string(synconcealed(1, 5)[1])")
            .as_deref(),
        Some("'X'")
    );
}

// Upstream: test_syntax.vim
#[test]
fn synstack_and_synidtrans_parity_from_upstream_test_syntax() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("session should initialize");
    let win_id = session.windows()[0].id;

    session
        .execute_ex_command("syntax on")
        .expect("syntax should be enabled");
    session
        .execute_ex_command("syntax clear")
        .expect("syntax should be cleared before adding local rules");
    session
        .execute_ex_command("highlight default link UpstreamComment Comment")
        .expect("comment highlight link should be installed");
    session
        .execute_ex_command("highlight default link UpstreamTodo Todo")
        .expect("todo highlight link should be installed");
    session
        .execute_ex_command("syntax keyword UpstreamTodo TODO contained")
        .expect("todo keyword should be installed");
    session
        .execute_ex_command(
            "syntax region UpstreamComment start=/\\/\\*/ end=/\\*\\// contains=UpstreamTodo",
        )
        .expect("comment region should be installed");
    session
        .execute_ex_command("call setline(1, '/* TODO */')")
        .expect("test line should be installed");
    session
        .execute_normal_command("0fT")
        .expect("cursor should move into the TODO token");

    // The public contract exposes syntax chunks, not highlight definition tables.
    let chunks = session
        .get_line_syntax(win_id, 1)
        .expect("line syntax should be available");
    assert!(
        chunks
            .iter()
            .any(|chunk| { chunk.name.as_deref() == Some("UpstreamComment") })
    );
    assert!(
        chunks
            .iter()
            .any(|chunk| { chunk.name.as_deref() == Some("UpstreamTodo") })
    );

    assert_eq!(
        session
            .eval_string("string(map(synstack(line('.'), col('.')), 'synIDattr(v:val, \"name\")'))")
            .as_deref(),
        Some("['UpstreamComment', 'UpstreamTodo']")
    );
    assert_eq!(
        session
            .eval_string(
                "string(map(synstack(line('.'), col('.')), 'synIDattr(synIDtrans(v:val), \"name\")'))"
            )
            .as_deref(),
        Some("['Comment', 'Todo']")
    );
}
