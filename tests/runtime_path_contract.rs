use std::fs;
use std::path::Path;
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

fn vim_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn append_runtime_path(session: &mut VimCoreSession, path: &Path) {
    let path_literal = vim_string_literal(&path.to_string_lossy());
    session
        .execute_ex_command(&format!("let &rtp .= ',' . {path_literal}"))
        .expect("runtimepath should be extended");
}

fn edit_with_fnameescape(session: &mut VimCoreSession, path: &Path) {
    let path_literal = vim_string_literal(&path.to_string_lossy());
    session
        .execute_ex_command(&format!("execute 'edit ' . fnameescape({path_literal})"))
        .expect("edit should succeed through fnameescape");
}

fn eval_required(session: &mut VimCoreSession, expr: &str) -> String {
    session
        .eval_string(expr)
        .unwrap_or_else(|| panic!("expected eval result for {expr}"))
}

#[test]
fn runtimepath_exposes_non_empty_defaults_and_vim_dir() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");

    session
        .execute_ex_command("put =&runtimepath")
        .expect("failed to query runtimepath");
    let runtimepath = session.snapshot().text.trim().to_string();
    assert!(!runtimepath.is_empty(), "runtimepath should not be empty");
    assert!(
        runtimepath.contains("share/vim"),
        "runtimepath should contain the bundled Vim runtime: {runtimepath}"
    );

    session
        .execute_ex_command("%d")
        .expect("failed to clear buffer");
    session
        .execute_ex_command("put =$VIM")
        .expect("failed to query $VIM");
    let vim_dir = session.snapshot().text.trim().to_string();
    assert!(
        vim_dir.contains("share/vim"),
        "$VIM should point at the embedded runtime root: {vim_dir}"
    );
}

#[test]
fn runtimepath_contract_supports_runtime_and_autoload_loading() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");
    let tmp_dir = tempdir().expect("failed to create temp dir");

    let plugin_dir = tmp_dir.path().join("plugin");
    let autoload_dir = tmp_dir.path().join("autoload");
    fs::create_dir_all(&plugin_dir).expect("failed to create plugin dir");
    fs::create_dir_all(&autoload_dir).expect("failed to create autoload dir");
    fs::write(
        plugin_dir.join("contract_plugin.vim"),
        "let g:contract_plugin_loaded = 1\n",
    )
    .expect("failed to write plugin");
    fs::write(
        autoload_dir.join("contracttest.vim"),
        concat!(
            "let g:contract_autoload_sourced = get(g:, 'contract_autoload_sourced', 0) + 1\n",
            "function! contracttest#value() abort\n",
            "  return 'autoload-ok'\n",
            "endfunction\n"
        ),
    )
    .expect("failed to write autoload file");

    append_runtime_path(&mut session, tmp_dir.path());

    session
        .execute_ex_command("runtime! plugin/contract_plugin.vim")
        .expect("runtime! should load plugin");
    assert_eq!(
        eval_required(&mut session, "string(g:contract_plugin_loaded)"),
        "1"
    );

    assert_eq!(
        eval_required(&mut session, "contracttest#value()"),
        "autoload-ok"
    );
    assert_eq!(
        eval_required(&mut session, "string(g:contract_autoload_sourced)"),
        "1"
    );
}

#[test]
fn runtimepath_contract_supports_help_lookup_from_runtime_docs() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");
    let tmp_dir = tempdir().expect("failed to create temp dir");
    let doc_dir = tmp_dir.path().join("doc");
    fs::create_dir_all(&doc_dir).expect("failed to create doc dir");
    fs::write(
        doc_dir.join("contract-help.txt"),
        concat!(
            "*contract-runtime*  Runtime contract help\n",
            "\n",
            "This help file proves repository-managed runtime discovery.\n",
        ),
    )
    .expect("failed to write help file");

    session
        .execute_ex_command(&format!(
            "helptags {}",
            doc_dir.to_string_lossy().replace(' ', "\\ ")
        ))
        .expect("helptags should succeed");
    append_runtime_path(&mut session, tmp_dir.path());
    let tags_content = fs::read_to_string(doc_dir.join("tags")).expect("tags file should exist");
    assert!(
        tags_content.contains("contract-runtime"),
        "helptags should generate a tag entry for the runtime doc: {tags_content}"
    );

    session
        .execute_ex_command("help contract-runtime")
        .expect("help lookup should succeed");

    assert!(
        eval_required(&mut session, "globpath(&rtp, 'doc/tags')")
            .contains(doc_dir.join("tags").to_string_lossy().as_ref()),
        "runtimepath should expose the generated help tags file"
    );
}

#[test]
fn runtimepath_contract_supports_path_discovery_and_fnameescape() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");
    let tmp_dir = tempdir().expect("failed to create temp dir");
    let nested_dir = tmp_dir.path().join("dir with spaces").join("nested");
    fs::create_dir_all(&nested_dir).expect("failed to create nested dir");
    let note_path = nested_dir.join("notes.txt");
    fs::write(&note_path, "note\n").expect("failed to write note file");

    let root_literal = vim_string_literal(&tmp_dir.path().to_string_lossy());
    session
        .execute_ex_command(&format!("execute 'cd ' . fnameescape({root_literal})"))
        .expect("cd should succeed");

    let expected_cwd = fs::canonicalize(tmp_dir.path())
        .expect("temp dir should canonicalize")
        .to_string_lossy()
        .to_string();
    assert_eq!(
        eval_required(&mut session, "resolve(getcwd())"),
        expected_cwd
    );

    let found = eval_required(&mut session, "findfile('notes.txt', '**')");
    assert!(
        found.ends_with("notes.txt"),
        "findfile should resolve the nested note path: {found}"
    );

    edit_with_fnameescape(&mut session, &note_path);
    assert_eq!(eval_required(&mut session, "expand('%:t')"), "notes.txt");
    assert_eq!(
        eval_required(&mut session, "fnamemodify(expand('%'), ':t')"),
        "notes.txt"
    );
    assert!(
        eval_required(&mut session, "fnamemodify(expand('%'), ':h')").contains("dir with spaces"),
        "escaped edit should preserve the directory name with spaces"
    );
}

#[test]
fn runtimepath_contract_supports_filetype_detection_from_runtime() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");
    let tmp_dir = tempdir().expect("failed to create temp dir");
    let ftdetect_dir = tmp_dir.path().join("ftdetect");
    fs::create_dir_all(&ftdetect_dir).expect("failed to create ftdetect dir");
    fs::write(
        ftdetect_dir.join("contracttest.vim"),
        "au BufRead,BufNewFile *.contracttest setfiletype contracttest\n",
    )
    .expect("failed to write filetype detector");

    let sample_path = tmp_dir.path().join("sample.contracttest");
    fs::write(&sample_path, "hello\n").expect("failed to write sample file");

    append_runtime_path(&mut session, tmp_dir.path());
    session
        .execute_ex_command("filetype on")
        .expect("filetype on should succeed");
    session
        .execute_ex_command("runtime! ftdetect/*.vim")
        .expect("runtime should load repository filetype detectors");
    edit_with_fnameescape(&mut session, &sample_path);

    assert_eq!(eval_required(&mut session, "&filetype"), "contracttest");
}
