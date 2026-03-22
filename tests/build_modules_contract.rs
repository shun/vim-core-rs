mod build_allowlist {
    include!("../build_allowlist.rs");
}
mod build_compile_plan {
    include!("../build_compile_plan.rs");
}
mod build_link_audit {
    include!("../build_link_audit.rs");
}

mod archive_member_audit_tests {
    use super::build_compile_plan::CompilePlan;
    use super::build_link_audit::{
        audit_archive_members, build_ex_delegation_proof_from_symbols,
        build_normal_delegation_proof_from_symbols, ArchiveAuditViolation,
    };
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn create_temp_dir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("vim_core_rs_test_archive_audit")
            .join(suffix);
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("should clean test dir");
        }
        fs::create_dir_all(&dir).expect("should create test dir");
        dir
    }

    fn create_archive(dir: &Path, archive_name: &str, members: &[&str]) -> PathBuf {
        for member in members {
            fs::write(dir.join(member), format!("fake object: {member}\n"))
                .expect("should write fake object");
        }

        let archive_path = dir.join(archive_name);
        let ar = std::env::var("AR").unwrap_or_else(|_| "ar".to_string());
        let status = Command::new(&ar)
            .current_dir(dir)
            .arg("crus")
            .arg(&archive_path)
            .args(members)
            .status()
            .expect("should invoke ar");

        assert!(status.success(), "ar should create archive successfully");
        archive_path
    }

    #[test]
    fn archive_missing_manifest_member_fails_audit() {
        let dir = create_temp_dir("missing_manifest_member");
        let archive = create_archive(
            &dir,
            "libvimcore.a",
            &["abc123-vim_bridge.o", "abc123-upstream_runtime.o"],
        );
        let plan = CompilePlan {
            native_sources: vec![
                PathBuf::from("native/vim_bridge.c"),
                PathBuf::from("native/upstream_runtime.c"),
            ],
            vendor_sources: vec![PathBuf::from("vendor/vim_src/src/normal.c")],
        };

        let report =
            audit_archive_members(&archive, &plan, &[PathBuf::from("generated/pathdef.c")])
                .expect("audit should succeed");

        assert!(!report.passed, "missing manifest member should fail");
        assert!(
            report.violations.iter().any(|violation| matches!(
                violation,
                ArchiveAuditViolation::MissingExpectedTranslationUnit { translation_unit, .. }
                if translation_unit == "normal"
            )),
            "should report missing `normal` translation unit: {:?}",
            report.violations
        );
    }

    #[test]
    fn archive_with_unexpected_member_fails_audit() {
        let dir = create_temp_dir("unexpected_member");
        let archive = create_archive(
            &dir,
            "libvimcore.a",
            &["abc123-vim_bridge.o", "def456-gui_gtk.o"],
        );
        let plan = CompilePlan {
            native_sources: vec![PathBuf::from("native/vim_bridge.c")],
            vendor_sources: vec![],
        };

        let report = audit_archive_members(&archive, &plan, &[]).expect("audit should succeed");

        assert!(!report.passed, "unexpected archive member should fail");
        assert!(
            report.violations.iter().any(|violation| matches!(
                violation,
                ArchiveAuditViolation::UnexpectedArchiveMember { archive_member, .. }
                if archive_member == "gui_gtk"
            )),
            "should report unexpected `gui_gtk` translation unit: {:?}",
            report.violations
        );
    }

    #[test]
    fn prohibited_translation_unit_fails_even_when_expected() {
        let dir = create_temp_dir("prohibited_member");
        let archive = create_archive(&dir, "libvimcore.a", &["abc123-gui_gtk.o"]);
        let plan = CompilePlan {
            native_sources: vec![],
            vendor_sources: vec![PathBuf::from("vendor/vim_src/src/gui_gtk.c")],
        };

        let report = audit_archive_members(&archive, &plan, &[]).expect("audit should succeed");

        assert!(!report.passed, "prohibited translation unit should fail");
        assert!(
            report.violations.iter().any(|violation| matches!(
                violation,
                ArchiveAuditViolation::ProhibitedTranslationUnit { translation_unit, .. }
                if translation_unit == "gui_gtk"
            )),
            "should report prohibited `gui_gtk` translation unit: {:?}",
            report.violations
        );
    }

    #[test]
    fn normal_delegation_proof_passes_with_required_symbols() {
        let report = build_normal_delegation_proof_from_symbols(BTreeSet::from([
            "exec_normal_cmd".to_string(),
            "ins_typebuf".to_string(),
            "ml_delete".to_string(),
            "changed_bytes".to_string(),
            "changed_lines".to_string(),
            "check_cursor".to_string(),
            "restart_edit".to_string(),
            "coladvance".to_string(),
            "inc_cursor".to_string(),
            "gchar_cursor".to_string(),
            "open_line".to_string(),
            "u_save".to_string(),
        ]));

        assert!(report.passed, "proof should pass: {:?}", report);
        assert_eq!(report.scenarios.len(), 2);
        assert!(report
            .scenarios
            .iter()
            .all(|scenario| scenario.missing_symbols.is_empty()));
    }

    #[test]
    fn normal_delegation_proof_fails_when_insert_symbols_are_missing() {
        let report = build_normal_delegation_proof_from_symbols(BTreeSet::from([
            "exec_normal_cmd".to_string(),
            "ml_delete".to_string(),
            "changed_bytes".to_string(),
            "changed_lines".to_string(),
            "check_cursor".to_string(),
        ]));

        assert!(
            !report.passed,
            "proof should fail when insert symbols are missing"
        );
        let insert_scenario = report
            .scenarios
            .iter()
            .find(|scenario| scenario.name == "normal-insert-enter")
            .expect("insert scenario should exist");
        assert!(
            insert_scenario
                .missing_symbols
                .contains(&"restart_edit".to_string()),
            "missing insert symbols should include restart_edit: {:?}",
            insert_scenario.missing_symbols
        );
    }

    #[test]
    fn ex_delegation_proof_passes_with_required_symbols() {
        let report = build_ex_delegation_proof_from_symbols(BTreeSet::from([
            "vim_bridge_apply_ex_command".to_string(),
            "upstream_runtime_apply_ex_command".to_string(),
            "do_cmdline_cmd".to_string(),
            "do_cmdline".to_string(),
            "vim_bridge_take_pending_host_action".to_string(),
            "upstream_runtime_take_pending_host_action".to_string(),
            "buf_write".to_string(),
            "buf_write_all".to_string(),
            "ex_exitval".to_string(),
            "redraw_cmd".to_string(),
        ]));

        assert!(report.passed, "proof should pass: {:?}", report);
        assert_eq!(report.scenarios.len(), 3);
        assert!(report
            .scenarios
            .iter()
            .all(|scenario| scenario.missing_symbols.is_empty()));
    }

    #[test]
    fn ex_delegation_proof_fails_when_host_action_symbols_are_missing() {
        let report = build_ex_delegation_proof_from_symbols(BTreeSet::from([
            "vim_bridge_apply_ex_command".to_string(),
            "upstream_runtime_apply_ex_command".to_string(),
            "do_cmdline_cmd".to_string(),
            "do_cmdline".to_string(),
            "redraw_cmd".to_string(),
        ]));

        assert!(
            !report.passed,
            "proof should fail when host action symbols are missing"
        );
        let host_action_scenario = report
            .scenarios
            .iter()
            .find(|scenario| scenario.name == "ex-host-actions")
            .expect("host action scenario should exist");
        assert!(
            host_action_scenario
                .missing_symbols
                .contains(&"vim_bridge_take_pending_host_action".to_string()),
            "missing host action symbols should include vim_bridge_take_pending_host_action: {:?}",
            host_action_scenario.missing_symbols
        );
    }
}

mod native_source_audit_tests {
    use super::build_link_audit::{
        audit_native_sources, NativeSourceAuditReport, NativeSourceViolation,
    };
    use std::fs;
    use std::path::Path;

    fn create_temp_native_dir(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join("vim_core_rs_test_native_audit")
            .join(suffix);
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("should clean test dir");
        }
        fs::create_dir_all(&dir).expect("should create test dir");
        dir
    }

    #[test]
    fn clean_bridge_passes_audit() {
        // vim_bridge.c が runtime への委譲のみを含む場合、監査は成功する
        let dir = create_temp_native_dir("clean_bridge");
        fs::write(
            dir.join("vim_bridge.c"),
            r#"#include "vim_bridge.h"
#include "upstream_runtime.h"
#include <stdlib.h>

struct vim_bridge_state {
    upstream_runtime_session_t* runtime;
};

vim_bridge_state_t* vim_bridge_state_new(
    const char* initial_text, uintptr_t text_len
) {
    vim_bridge_state_t* state = calloc(1, sizeof(*state));
    state->runtime = upstream_runtime_session_new(initial_text, text_len);
    return state;
}

vim_core_command_result_t vim_bridge_apply_normal_command(
    vim_bridge_state_t* state, const char* command, uintptr_t command_len
) {
    return upstream_runtime_apply_normal_command(state->runtime, command, command_len);
}
"#,
        )
        .expect("should write test file");
        fs::write(dir.join("vim_bridge.h"), "#ifndef VIM_BRIDGE_H\n#endif\n")
            .expect("should write header");

        let report = audit_native_sources(&dir).expect("audit should succeed");
        assert!(
            report.violations.is_empty(),
            "clean bridge should have no violations, got: {:?}",
            report.violations
        );
        assert!(report.passed);
    }

    #[test]
    fn bridge_with_vim_internal_call_fails_audit() {
        // vim_bridge.c が exec_normal_cmd を直接呼ぶと監査失敗
        let dir = create_temp_native_dir("bridge_vim_call");
        fs::write(
            dir.join("vim_bridge.c"),
            r#"#include "vim_bridge.h"
void vim_bridge_do_something(void) {
    exec_normal_cmd((char_u*)"dd", REMAP_NONE, FALSE);
}
"#,
        )
        .expect("should write test file");
        fs::write(dir.join("vim_bridge.h"), "#ifndef VIM_BRIDGE_H\n#endif\n")
            .expect("should write header");

        let report = audit_native_sources(&dir).expect("audit should succeed");
        assert!(!report.passed, "bridge with Vim internal call should fail");
        assert!(
            report.violations.iter().any(|v| matches!(
                v,
                NativeSourceViolation::BridgeContainsVimInternalCall { .. }
            )),
            "should detect exec_normal_cmd in bridge: {:?}",
            report.violations
        );
    }

    #[test]
    fn bridge_with_do_cmdline_cmd_fails_audit() {
        // vim_bridge.c が do_cmdline_cmd を直接呼ぶと監査失敗
        let dir = create_temp_native_dir("bridge_cmdline");
        fs::write(
            dir.join("vim_bridge.c"),
            r#"#include "vim_bridge.h"
void vim_bridge_execute(const char* cmd) {
    do_cmdline_cmd((char_u*)cmd);
}
"#,
        )
        .expect("should write test file");
        fs::write(dir.join("vim_bridge.h"), "#ifndef VIM_BRIDGE_H\n#endif\n")
            .expect("should write header");

        let report = audit_native_sources(&dir).expect("audit should succeed");
        assert!(!report.passed, "bridge with do_cmdline_cmd should fail");
        assert!(
            report.violations.iter().any(|v| matches!(
                v,
                NativeSourceViolation::BridgeContainsVimInternalCall { .. }
            )),
            "should detect do_cmdline_cmd in bridge: {:?}",
            report.violations
        );
    }

    #[test]
    fn bridge_with_custom_ex_parser_fails_audit() {
        // vim_bridge.c が Ex コマンドを独自に文字列比較で分岐すると監査失敗
        let dir = create_temp_native_dir("bridge_custom_ex_parser");
        fs::write(
            dir.join("vim_bridge.c"),
            r#"#include "vim_bridge.h"
#include <string.h>
void vim_bridge_execute(const char* cmd) {
    if (strcmp(cmd, ":write") == 0) {
        return;
    }
}
"#,
        )
        .expect("should write test file");
        fs::write(dir.join("vim_bridge.h"), "#ifndef VIM_BRIDGE_H\n#endif\n")
            .expect("should write header");

        let report = audit_native_sources(&dir).expect("audit should succeed");
        assert!(!report.passed, "bridge with custom Ex parser should fail");
        assert!(
            report.violations.iter().any(|v| matches!(
                v,
                NativeSourceViolation::BridgeContainsCustomExParser { .. }
            )),
            "should detect custom Ex parser in bridge: {:?}",
            report.violations
        );
    }

    #[test]
    fn bridge_with_ml_functions_fails_audit() {
        // vim_bridge.c が ml_get/ml_replace/ml_delete/ml_append を呼ぶと失敗
        let dir = create_temp_native_dir("bridge_ml");
        fs::write(
            dir.join("vim_bridge.c"),
            r#"#include "vim_bridge.h"
void vim_bridge_hack(void) {
    char_u* line = ml_get(1);
    ml_replace(1, (char_u*)"new", TRUE);
}
"#,
        )
        .expect("should write test file");
        fs::write(dir.join("vim_bridge.h"), "#ifndef VIM_BRIDGE_H\n#endif\n")
            .expect("should write header");

        let report = audit_native_sources(&dir).expect("audit should succeed");
        assert!(!report.passed, "bridge with ml_ calls should fail");
        assert!(
            report.violations.len() >= 2,
            "should detect multiple violations: {:?}",
            report.violations
        );
    }

    #[test]
    fn runtime_with_vim_calls_passes_audit() {
        // upstream_runtime.c が exec_normal_cmd 等を呼ぶのは正当な委譲
        let dir = create_temp_native_dir("runtime_vim_call");
        fs::write(
            dir.join("upstream_runtime.c"),
            r#"#include "upstream_runtime.h"
void upstream_runtime_do(void) {
    exec_normal_cmd((char_u*)"dd", REMAP_NONE, FALSE);
    do_cmdline_cmd((char_u*)":set number");
    ml_get(1);
}
"#,
        )
        .expect("should write test file");
        fs::write(dir.join("vim_bridge.h"), "#ifndef VIM_BRIDGE_H\n#endif\n")
            .expect("should write header");
        // vim_bridge.c が存在しない場合はスキップ（または空でOK）
        fs::write(
            dir.join("vim_bridge.c"),
            r#"#include "vim_bridge.h"
// clean bridge
"#,
        )
        .expect("should write bridge");

        let report = audit_native_sources(&dir).expect("audit should succeed");
        assert!(
            report.passed,
            "runtime Vim calls are legitimate delegation, violations: {:?}",
            report.violations
        );
    }

    #[test]
    fn report_contains_file_and_line_info() {
        // 違反レポートがファイル名と行番号を含むこと
        let dir = create_temp_native_dir("report_info");
        fs::write(
            dir.join("vim_bridge.c"),
            "line one\nline two\nexec_normal_cmd(args);\nline four\n",
        )
        .expect("should write test file");
        fs::write(dir.join("vim_bridge.h"), "#ifndef VIM_BRIDGE_H\n#endif\n")
            .expect("should write header");

        let report = audit_native_sources(&dir).expect("audit should succeed");
        assert!(!report.passed);

        let violation = &report.violations[0];
        match violation {
            NativeSourceViolation::BridgeContainsVimInternalCall {
                file,
                line_number,
                matched_symbol,
                line_content,
            } => {
                assert!(
                    file.ends_with("vim_bridge.c"),
                    "file should be vim_bridge.c, got: {}",
                    file
                );
                assert_eq!(*line_number, 3, "violation is on line 3");
                assert_eq!(matched_symbol, "exec_normal_cmd");
                assert!(line_content.contains("exec_normal_cmd"));
            }
            NativeSourceViolation::BridgeContainsCustomExParser { .. } => {
                panic!("expected Vim internal call violation, got custom Ex parser violation")
            }
        }
    }

    #[test]
    fn audit_report_serializes_to_readable_format() {
        // レポートが人間可読な文字列に変換できること
        let report = NativeSourceAuditReport {
            passed: false,
            violations: vec![
                NativeSourceViolation::BridgeContainsVimInternalCall {
                    file: "vim_bridge.c".to_string(),
                    line_number: 42,
                    matched_symbol: "exec_normal_cmd".to_string(),
                    line_content: "    exec_normal_cmd(args);".to_string(),
                },
                NativeSourceViolation::BridgeContainsCustomExParser {
                    file: "vim_bridge.c".to_string(),
                    line_number: 64,
                    matched_pattern: "strcmp(".to_string(),
                    line_content: "    if (strcmp(cmd, \":write\") == 0) {".to_string(),
                },
            ],
        };

        let formatted = format!("{report}");
        assert!(formatted.contains("vim_bridge.c"));
        assert!(formatted.contains("42"));
        assert!(formatted.contains("exec_normal_cmd"));
        assert!(formatted.contains("64"));
        assert!(formatted.contains("strcmp("));
    }

    #[test]
    fn actual_native_directory_passes_audit() {
        // 実際のプロジェクトの native/ ディレクトリが監査に合格することを確認
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set");
        let native_dir = Path::new(&manifest_dir).join("native");

        let report = audit_native_sources(&native_dir).expect("audit should succeed");
        assert!(
            report.passed,
            "actual native/ should pass audit, violations: {:?}",
            report.violations
        );
    }
}
mod build_test_runner {
    include!("../build_test_runner.rs");
}

mod build_test_runner_contract_tests {
    use super::build_test_runner::{
        generate_upstream_tests_from, parse_skiplist, GeneratedUpstreamTestManifest,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    fn create_temp_dir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("vim_core_rs_test_build_test_runner")
            .join(suffix);
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("should clean test dir");
        }
        fs::create_dir_all(&dir).expect("should create test dir");
        dir
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("should create parent dir");
        }
        fs::write(path, content).expect("should write file");
    }

    #[test]
    fn parse_skiplist_accepts_comment_and_reason_format() {
        let dir = create_temp_dir("skiplist_parse");
        let skiplist_path = dir.join("upstream-test-skiplist.txt");
        write_file(
            &skiplist_path,
            "# headless-incompatible cases\n\
             test_beta.vim | requires terminal UI\n\
             \n\
             test_gamma.vim | depends on +channel\n",
        );

        let skiplist = parse_skiplist(&skiplist_path).expect("skiplist should parse");

        assert_eq!(
            skiplist.get("test_beta.vim"),
            Some(&"requires terminal UI".to_string())
        );
        assert_eq!(
            skiplist.get("test_gamma.vim"),
            Some(&"depends on +channel".to_string())
        );
    }

    #[test]
    fn generate_upstream_tests_writes_manifest_and_generated_runner() {
        let dir = create_temp_dir("generate_runner");
        let repo_root = dir.join("repo");
        let out_dir = dir.join("out");
        write_file(
            &repo_root.join("vendor/vim_src/src/testdir/test_beta.vim"),
            "quit!\n",
        );
        write_file(
            &repo_root.join("vendor/vim_src/src/testdir/test_alpha.vim"),
            "quit!\n",
        );
        write_file(
            &repo_root.join("upstream-test-skiplist.txt"),
            "test_beta.vim | requires terminal UI\n",
        );
        fs::create_dir_all(&out_dir).expect("should create out dir");

        generate_upstream_tests_from(&repo_root, &out_dir)
            .expect("upstream test runner should generate");

        let manifest = fs::read_to_string(out_dir.join("upstream_test_manifest.json"))
            .expect("manifest should be written");
        let manifest: GeneratedUpstreamTestManifest =
            serde_json::from_str(&manifest).expect("manifest should deserialize");
        assert_eq!(manifest.cases.len(), 2);
        assert_eq!(manifest.cases[0].name, "test_alpha.vim");
        assert_eq!(
            manifest.cases[0].relative_path,
            "vendor/vim_src/src/testdir/test_alpha.vim"
        );
        assert!(!manifest.cases[0].ignored);
        assert_eq!(manifest.cases[1].name, "test_beta.vim");
        assert!(manifest.cases[1].ignored);
        assert_eq!(
            manifest.cases[1].ignore_reason.as_deref(),
            Some("requires terminal UI")
        );

        let generated = fs::read_to_string(out_dir.join("upstream_vim_tests.rs"))
            .expect("generated runner should be written");
        assert!(
            generated.contains("fn upstream_test_alpha_vim()"),
            "expected alpha test entry in generated runner: {generated}"
        );
        assert!(
            generated.contains("run_case_in_subprocess(\"vendor/vim_src/src/testdir/test_alpha.vim\");"),
            "expected alpha subprocess entry in generated runner: {generated}"
        );
        assert!(
            generated.contains("#[ignore = \"requires terminal UI\"]"),
            "expected ignored beta test entry in generated runner: {generated}"
        );
    }

    #[test]
    fn generate_upstream_tests_without_vendored_testdir_writes_empty_runner() {
        let dir = create_temp_dir("empty_runner");
        let repo_root = dir.join("repo");
        let out_dir = dir.join("out");
        fs::create_dir_all(&out_dir).expect("should create out dir");

        generate_upstream_tests_from(&repo_root, &out_dir)
            .expect("empty upstream runner should still generate");

        let manifest = fs::read_to_string(out_dir.join("upstream_test_manifest.json"))
            .expect("manifest should be written");
        let manifest: GeneratedUpstreamTestManifest =
            serde_json::from_str(&manifest).expect("manifest should deserialize");
        assert!(manifest.cases.is_empty(), "expected no generated cases");

        let generated = fs::read_to_string(out_dir.join("upstream_vim_tests.rs"))
            .expect("generated runner should be written");
        assert!(
            generated.contains("No upstream Vim test cases were vendored"),
            "expected empty-runner marker: {generated}"
        );
    }
}

mod ffi_boundary_contract_tests {
    use std::fs;
    use std::path::Path;

    #[test]
    fn bridge_header_exposes_foundation_types_for_mode_pending_marks_and_jumplist() {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set");
        let header_path = Path::new(&manifest_dir).join("native").join("vim_bridge.h");
        let header = fs::read_to_string(&header_path).expect("bridge header should be readable");

        for required in [
            "VIM_CORE_MODE_SELECT_LINE",
            "VIM_CORE_MODE_SELECT_BLOCK",
            "typedef enum vim_core_pending_input",
            "typedef struct vim_core_mark_position",
            "typedef struct vim_core_jumplist_entry",
            "typedef struct vim_core_jumplist",
        ] {
            assert!(
                header.contains(required),
                "bridge header should contain `{required}`"
            );
        }
    }

    #[test]
    fn bridge_header_exposes_foundation_entrypoints_for_new_state_accessors() {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set");
        let header_path = Path::new(&manifest_dir).join("native").join("vim_bridge.h");
        let header = fs::read_to_string(&header_path).expect("bridge header should be readable");

        for required in [
            "vim_bridge_get_pending_input(",
            "vim_bridge_get_mark(",
            "vim_bridge_set_mark(",
            "vim_bridge_get_jumplist(",
            "vim_bridge_free_jumplist(",
        ] {
            assert!(
                header.contains(required),
                "bridge header should contain `{required}`"
            );
        }
    }

    #[test]
    fn bridge_header_exposes_option_foundation_types_and_results() {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set");
        let header_path = Path::new(&manifest_dir).join("native").join("vim_bridge.h");
        let header = fs::read_to_string(&header_path).expect("bridge header should be readable");

        for required in [
            "typedef enum vim_core_option_type",
            "VIM_CORE_OPTION_TYPE_BOOL",
            "VIM_CORE_OPTION_TYPE_NUMBER",
            "VIM_CORE_OPTION_TYPE_STRING",
            "VIM_CORE_OPTION_TYPE_UNKNOWN",
            "typedef enum vim_core_option_scope",
            "VIM_CORE_OPTION_SCOPE_DEFAULT",
            "VIM_CORE_OPTION_SCOPE_GLOBAL",
            "VIM_CORE_OPTION_SCOPE_LOCAL",
            "typedef struct vim_core_option_get_result",
            "vim_core_option_type_t option_type;",
            "int64_t number_value;",
            "const char* string_value_ptr;",
            "uintptr_t string_value_len;",
            "typedef struct vim_core_option_set_result",
            "const char* error_message_ptr;",
            "uintptr_t error_message_len;",
        ] {
            assert!(
                header.contains(required),
                "bridge header should contain `{required}`"
            );
        }
    }

    #[test]
    fn bridge_header_exposes_option_bridge_entrypoints() {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set");
        let header_path = Path::new(&manifest_dir).join("native").join("vim_bridge.h");
        let header = fs::read_to_string(&header_path).expect("bridge header should be readable");

        for required in [
            "vim_core_option_get_result_t vim_bridge_get_option(",
            "vim_core_option_set_result_t vim_bridge_set_option_number(",
            "vim_core_option_set_result_t vim_bridge_set_option_string(",
        ] {
            assert!(
                header.contains(required),
                "bridge header should contain `{required}`"
            );
        }
    }

    #[test]
    fn upstream_runtime_header_exposes_option_entrypoints() {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set");
        let header_path = Path::new(&manifest_dir)
            .join("native")
            .join("upstream_runtime.h");
        let header = fs::read_to_string(&header_path).expect("runtime header should be readable");

        for required in [
            "vim_core_option_get_result_t upstream_runtime_get_option(",
            "vim_core_option_set_result_t upstream_runtime_set_option_number(",
            "vim_core_option_set_result_t upstream_runtime_set_option_string(",
        ] {
            assert!(
                header.contains(required),
                "runtime header should contain `{required}`"
            );
        }
    }

    #[test]
    fn bridge_source_delegates_option_access_to_runtime_with_null_guards() {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set");
        let source_path = Path::new(&manifest_dir).join("native").join("vim_bridge.c");
        let source = fs::read_to_string(&source_path).expect("bridge source should be readable");

        for required in [
            "vim_bridge_get_option(",
            "upstream_runtime_get_option(",
            "vim_bridge_set_option_number(",
            "upstream_runtime_set_option_number(",
            "vim_bridge_set_option_string(",
            "upstream_runtime_set_option_string(",
            "return upstream_runtime_get_option(NULL, name, scope);",
            "return upstream_runtime_set_option_number(NULL, name, value, scope);",
            "return upstream_runtime_set_option_string(NULL, name, value, scope);",
        ] {
            assert!(
                source.contains(required),
                "bridge source should contain `{required}`"
            );
        }
    }

    #[test]
    fn upstream_runtime_source_delegates_option_access_to_vim_core() {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set");
        let source_path = Path::new(&manifest_dir)
            .join("native")
            .join("upstream_runtime.c");
        let source = fs::read_to_string(&source_path).expect("runtime source should be readable");

        for required in [
            "get_option_value(",
            "set_option_value(",
            "strdup(",
            "vim_free(",
            "OPT_GLOBAL",
            "OPT_LOCAL",
        ] {
            assert!(
                source.contains(required),
                "runtime source should contain `{required}`"
            );
        }
    }
}

#[test]
fn build_modules_are_shared_with_tests_via_include() {
    let _ = build_allowlist::Allowlist::load;
    let _ = build_allowlist::validate_allowlist;
    let _ = build_compile_plan::create_compile_plan;
    let _ = build_compile_plan::UpstreamMetadata::load;
    let _ = build_link_audit::run_link_audit;
    let _ = build_test_runner::generate_upstream_tests;
}

#[test]
fn generated_config_h_exists_in_build_output() {
    // config.h は configure スクリプトにより生成され、HAVE_CONFIG_H が
    // 定義されている前提でコンパイルされる。生成物が存在しないと
    // upstream ソースのコンパイルが失敗する。
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let config_h = std::path::Path::new(&out_dir).join("vim_build/auto/config.h");
    assert!(
        config_h.exists(),
        "config.h should exist at {}",
        config_h.display()
    );
    let content = std::fs::read_to_string(&config_h).expect("config.h should be readable");
    assert!(!content.is_empty(), "config.h should not be empty");
}

#[test]
fn generated_config_h_defines_modified_by_for_vim_license_notice() {
    // Vim ライセンス II.3 では、改変版配布時に :version と intro screen で
    // 改変者情報を表示できる状態が必要になる。configure 生成の config.h に
    // MODIFIED_BY が入っていないと、その表示が有効化されない。
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let config_h = std::path::Path::new(&out_dir).join("vim_build/auto/config.h");
    let content = std::fs::read_to_string(&config_h).expect("config.h should be readable");

    assert!(
        content.contains("#define MODIFIED_BY "),
        "config.h should define MODIFIED_BY so modified Vim builds disclose the distributor"
    );
}

#[test]
fn generated_osdef_h_exists_in_build_output() {
    // osdef.h は osdef.sh により生成される OS 固有の定義ファイルで、
    // upstream ソースが要求する。
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let osdef_h = std::path::Path::new(&out_dir).join("vim_build/auto/osdef.h");
    assert!(
        osdef_h.exists(),
        "osdef.h should exist at {}",
        osdef_h.display()
    );
}

#[test]
fn generated_pathdef_c_contains_non_empty_vim_dir() {
    // pathdef.c の default_vim_dir が空文字ではなく、configure 由来の
    // パスで埋められていることを検証する。これが空文字だと Vim の
    // runtimepath / help / syntax 探索がフォールバック先を失う。
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let pathdef_c = std::path::Path::new(&out_dir).join("vim_build/auto/pathdef.c");
    assert!(
        pathdef_c.exists(),
        "pathdef.c should exist at {}",
        pathdef_c.display()
    );
    let content = std::fs::read_to_string(&pathdef_c).expect("pathdef.c should be readable");

    // default_vim_dir が空文字 "" ではなく、実際のパスを含むこと
    assert!(
        !content.contains("default_vim_dir = (char_u *)\"\""),
        "default_vim_dir should not be an empty string placeholder"
    );
    assert!(
        content.contains("default_vim_dir"),
        "pathdef.c should define default_vim_dir"
    );
}

#[test]
fn generated_pathdef_c_contains_non_empty_vimruntime_dir() {
    // default_vimruntime_dir も空文字ではなく、Vim のバージョンを含む
    // ランタイムパスで埋められていることを検証する。
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let pathdef_c = std::path::Path::new(&out_dir).join("vim_build/auto/pathdef.c");
    let content = std::fs::read_to_string(&pathdef_c).expect("pathdef.c should be readable");

    assert!(
        !content.contains("default_vimruntime_dir = (char_u *)\"\""),
        "default_vimruntime_dir should not be an empty string placeholder"
    );
    assert!(
        content.contains("default_vimruntime_dir"),
        "pathdef.c should define default_vimruntime_dir"
    );
}

#[test]
fn generated_pathdef_c_vimruntime_dir_contains_version() {
    // default_vimruntime_dir は vim{major}{minor} 形式のバージョンを含む
    // べきである（例: /usr/local/share/vim/vim92）。
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let pathdef_c = std::path::Path::new(&out_dir).join("vim_build/auto/pathdef.c");
    let content = std::fs::read_to_string(&pathdef_c).expect("pathdef.c should be readable");

    // upstream-metadata.json の tag から major.minor を取得して、
    // vim{major}{minor} が pathdef.c に含まれることを確認する。
    // tag は v9.2.0131 なので vim92 が含まれるべき。
    assert!(
        content.contains("vim9"),
        "default_vimruntime_dir should contain Vim version identifier (vim9x)"
    );
}
