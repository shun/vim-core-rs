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
    let expected_vim_dir = Path::new(env!("OUT_DIR")).join("share").join("vim");

    let runtimepath = eval_required(&mut session, "&runtimepath");
    assert!(!runtimepath.is_empty(), "runtimepath should not be empty");
    let vim_dir = eval_required(&mut session, "$VIM");
    let vimruntime_dir = eval_required(&mut session, "$VIMRUNTIME");

    assert_eq!(
        Path::new(&vim_dir),
        expected_vim_dir.as_path(),
        "$VIM should point at the current build's bundled runtime root"
    );
    assert!(
        Path::new(&vimruntime_dir).starts_with(&expected_vim_dir),
        "$VIMRUNTIME should live under the current build's bundled runtime root: {vimruntime_dir}"
    );
    assert!(
        runtimepath.split(',').any(|entry| entry == vimruntime_dir),
        "runtimepath should contain the bundled runtime directory: {runtimepath}"
    );
    assert_eq!(
        eval_required(&mut session, "isdirectory($VIM)"),
        "1",
        "$VIM should resolve to an existing bundled runtime root: {vim_dir}"
    );
    assert_eq!(
        eval_required(&mut session, "isdirectory($VIMRUNTIME)"),
        "1",
        "$VIMRUNTIME should resolve to an existing bundled runtime dir: {vimruntime_dir}"
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
fn runtimepath_contract_supports_help_tagjump_from_runtime_docs() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");
    let tmp_dir = tempdir().expect("failed to create temp dir");
    let doc_dir = tmp_dir.path().join("doc");
    fs::create_dir_all(&doc_dir).expect("failed to create doc dir");
    let helpfile_path = doc_dir.join("help.txt");
    fs::write(
        &helpfile_path,
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
    session
        .execute_ex_command(&format!(
            "let &helpfile = {}",
            vim_string_literal(&helpfile_path.to_string_lossy())
        ))
        .expect("helpfile should be redirected to the runtime doc");
    append_runtime_path(&mut session, tmp_dir.path());

    session
        .execute_ex_command("help! contract-runtime")
        .expect("help! tagjump should succeed");

    assert_eq!(eval_required(&mut session, "&filetype"), "help");
    assert!(
        eval_required(&mut session, "getline('.')").contains("*contract-runtime*"),
        "help tagjump should open the runtime doc topic"
    );
    assert_eq!(
        eval_required(&mut session, "expand('<cword>')"),
        "contract-runtime"
    );
}

#[test]
fn runtimepath_contract_supports_help_local_additions_from_runtime_docs() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");
    let tmp_dir = tempdir().expect("failed to create temp dir");
    let doc_dir = tmp_dir.path().join("doc");
    fs::create_dir_all(&doc_dir).expect("failed to create doc dir");
    let bundled_helpfile =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/upstream/vim/runtime/doc/help.txt");
    fs::write(doc_dir.join("mydoc.txt"), "*mydoc.txt* my awesome doc\n")
        .expect("failed to write first help file");
    fs::write(
        doc_dir.join("mydoc-ext.txt"),
        "*mydoc-ext.txt* my extended awesome doc\n",
    )
    .expect("failed to write second help file");

    session
        .execute_ex_command(&format!(
            "helptags {}",
            doc_dir.to_string_lossy().replace(' ', "\\ ")
        ))
        .expect("helptags should succeed");
    session
        .execute_ex_command(&format!(
            "let &helpfile = {}",
            vim_string_literal(&bundled_helpfile.to_string_lossy())
        ))
        .expect("helpfile should be redirected to the runtime doc");
    append_runtime_path(&mut session, tmp_dir.path());

    session
        .execute_ex_command("help local-additions@en")
        .expect("help local-additions@en should succeed");

    assert_eq!(eval_required(&mut session, "&filetype"), "help");
    let lines = eval_required(&mut session, "join(getline(1, '$'), '\\n')");
    assert!(
        lines.contains("|mydoc-ext.txt| my extended awesome doc"),
        "help local-additions should list the extended runtime doc: {lines}"
    );
    assert!(
        lines.contains("|mydoc.txt| my awesome doc"),
        "help local-additions should list the base runtime doc: {lines}"
    );
}

#[test]
fn runtimepath_contract_supports_checkpath_includeexpr_recursion() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");
    let tmp_dir = tempdir().expect("failed to create temp dir");
    let include_dir = tmp_dir.path().join("include").join("nested");
    fs::create_dir_all(&include_dir).expect("failed to create include dir");
    let foo_path = include_dir.join("foo.b");
    let bar_path = include_dir.join("bar.b");
    let baz_path = include_dir.join("baz.b");
    let base_path = tmp_dir.path().join("base.b");
    fs::write(&foo_path, "%inc    /bar/\n").expect("failed to write foo include");
    fs::write(&bar_path, "%inc    /baz/\n").expect("failed to write bar include");
    fs::write(&baz_path, "%inc    /foo/\n").expect("failed to write baz include");
    fs::write(&base_path, "%inc    /foo/\n").expect("failed to write base include");

    session
        .execute_ex_command(&format!(
            "let &include = {}",
            vim_string_literal(r"^\s*%inc\s*/\zs[^/]\+\ze")
        ))
        .expect("include should be configured");
    session
        .execute_ex_command(&format!(
            "let &includeexpr = {}",
            vim_string_literal(r"substitute(v:fname, '\.', '/', 'g') . '.b'")
        ))
        .expect("includeexpr should be configured");
    session
        .execute_ex_command(&format!(
            "let &path = {}",
            vim_string_literal(&include_dir.to_string_lossy())
        ))
        .expect("path should be configured");
    edit_with_fnameescape(&mut session, &base_path);

    let output = eval_required(&mut session, "execute('checkpath!')");
    let expected = format!(
        concat!(
            "\n--- Included files in path ---\n",
            "{foo}\n",
            "{foo} -->\n",
            "  {bar}\n",
            "  {bar} -->\n",
            "    {baz}\n",
            "    {baz} -->\n",
            "      foo  (Already listed)"
        ),
        foo = foo_path.to_string_lossy(),
        bar = bar_path.to_string_lossy(),
        baz = baz_path.to_string_lossy(),
    );
    assert_eq!(output, expected);
}

#[test]
fn runtimepath_contract_supports_wildcard_path_expansion_for_buffer_selection() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").expect("failed to create session");
    let tmp_dir = tempdir().expect("failed to create temp dir");
    let xdir1 = tmp_dir.path().join("Xdir1");
    let xdir2 = tmp_dir.path().join("Xdir2");
    let xdir4 = tmp_dir.path().join("Xdir3").join("Xdir4");
    fs::create_dir_all(&xdir1).expect("failed to create Xdir1");
    fs::create_dir_all(&xdir2).expect("failed to create Xdir2");
    fs::create_dir_all(&xdir4).expect("failed to create Xdir3/Xdir4");
    fs::write(xdir1.join("file"), "a\nb\n").expect("failed to write Xdir1/file");
    fs::write(xdir4.join("file"), "a\nb\n").expect("failed to write Xdir3/Xdir4/file");

    let root_literal = vim_string_literal(&tmp_dir.path().to_string_lossy());
    session
        .execute_ex_command(&format!("execute 'cd ' . fnameescape({root_literal})"))
        .expect("cd should succeed");

    session
        .execute_ex_command("next Xdir?/*/file")
        .expect("next should expand the wildcard path to the nested file");
    assert_eq!(
        eval_required(&mut session, "expand('%')"),
        "Xdir3/Xdir4/file"
    );

    #[cfg(unix)]
    {
        session
            .execute_ex_command("next! Xdir?/*/nofile")
            .expect("next! should keep the unmatched wildcard argument");
        assert_eq!(eval_required(&mut session, "expand('%')"), "Xdir?/*/nofile");
    }
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
