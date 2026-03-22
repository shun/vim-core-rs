use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const UPSTREAM_TESTDIR: &str = "vendor/vim_src/src/testdir";
const UPSTREAM_SKIPLIST: &str = "upstream-test-skiplist.txt";
const GENERATED_TESTS_RS: &str = "upstream_vim_tests.rs";
const GENERATED_MANIFEST_JSON: &str = "upstream_test_manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedUpstreamTestCase {
    pub name: String,
    pub relative_path: String,
    pub ignored: bool,
    pub ignore_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedUpstreamTestManifest {
    pub cases: Vec<GeneratedUpstreamTestCase>,
}

pub fn generate_upstream_tests(out_dir: &Path) -> Result<(), String> {
    let repo_root = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR")
            .map_err(|error| format!("missing CARGO_MANIFEST_DIR: {error}"))?,
    );
    generate_upstream_tests_from(&repo_root, out_dir)
}

pub fn generate_upstream_tests_from(repo_root: &Path, out_dir: &Path) -> Result<(), String> {
    let test_dir = repo_root.join(UPSTREAM_TESTDIR);
    let skiplist_path = repo_root.join(UPSTREAM_SKIPLIST);
    let skiplist = parse_skiplist(&skiplist_path)?;
    let manifest = GeneratedUpstreamTestManifest {
        cases: collect_cases(repo_root, &test_dir, &skiplist)?,
    };

    write_manifest(out_dir, &manifest)?;
    write_generated_runner(out_dir, &manifest)?;
    Ok(())
}

pub fn parse_skiplist(skiplist_path: &Path) -> Result<BTreeMap<String, String>, String> {
    if !skiplist_path.exists() {
        return Ok(BTreeMap::new());
    }

    let content = fs::read_to_string(skiplist_path).map_err(|error| {
        format!(
            "failed to read upstream test skiplist {}: {error}",
            skiplist_path.display()
        )
    })?;

    let mut skiplist = BTreeMap::new();
    for (line_no, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((case_name, reason)) = line.split_once('|') else {
            return Err(format!(
                "invalid upstream test skiplist line {} in {}: expected `test_name.vim | reason`",
                line_no + 1,
                skiplist_path.display()
            ));
        };

        let case_name = case_name.trim();
        let reason = reason.trim();
        if case_name.is_empty() || reason.is_empty() {
            return Err(format!(
                "invalid upstream test skiplist line {} in {}: case name and reason must be non-empty",
                line_no + 1,
                skiplist_path.display()
            ));
        }

        skiplist.insert(case_name.to_string(), reason.to_string());
    }

    Ok(skiplist)
}

fn collect_cases(
    repo_root: &Path,
    test_dir: &Path,
    skiplist: &BTreeMap<String, String>,
) -> Result<Vec<GeneratedUpstreamTestCase>, String> {
    if !test_dir.exists() {
        return Ok(Vec::new());
    }

    let mut cases = Vec::new();
    for entry in fs::read_dir(test_dir).map_err(|error| {
        format!(
            "failed to read upstream test directory {}: {error}",
            test_dir.display()
        )
    })? {
        let entry = entry.map_err(|error| {
            format!(
                "failed to enumerate upstream test directory {}: {error}",
                test_dir.display()
            )
        })?;
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?
            .is_file()
        {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.starts_with("test_") || !file_name.ends_with(".vim") {
            continue;
        }

        let relative_path = path
            .strip_prefix(repo_root)
            .map_err(|error| {
                format!(
                    "failed to compute relative upstream test path for {}: {error}",
                    path.display()
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        let ignore_reason = skiplist.get(file_name).cloned();
        cases.push(GeneratedUpstreamTestCase {
            name: file_name.to_string(),
            relative_path,
            ignored: ignore_reason.is_some(),
            ignore_reason,
        });
    }

    cases.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(cases)
}

fn write_manifest(out_dir: &Path, manifest: &GeneratedUpstreamTestManifest) -> Result<(), String> {
    let manifest_json = serde_json::to_string_pretty(manifest)
        .map_err(|error| format!("failed to serialize upstream test manifest: {error}"))?;
    fs::write(
        out_dir.join(GENERATED_MANIFEST_JSON),
        format!("{manifest_json}\n"),
    )
    .map_err(|error| {
        format!(
            "failed to write upstream test manifest {}: {error}",
            out_dir.join(GENERATED_MANIFEST_JSON).display()
        )
    })
}

fn write_generated_runner(
    out_dir: &Path,
    manifest: &GeneratedUpstreamTestManifest,
) -> Result<(), String> {
    let mut generated = String::from(
        "// @generated by build.rs\n\
         // Each upstream Vim case is isolated in its own subprocess to avoid\n\
         // violating the single-session-per-process contract.\n\n",
    );

    if manifest.cases.is_empty() {
        generated.push_str("// No upstream Vim test cases were vendored into this workspace.\n");
    } else {
        for case in &manifest.cases {
            if let Some(reason) = &case.ignore_reason {
                generated.push_str(&format!("#[ignore = {:?}]\n", reason));
            }
            generated.push_str("#[test]\n");
            generated.push_str(&format!(
                "fn {}() {{\n",
                rust_test_name(&case.name)
            ));
            generated.push_str(&format!(
                "    run_case_in_subprocess({:?});\n",
                case.relative_path
            ));
            generated.push_str("}\n\n");
        }
    }

    fs::write(out_dir.join(GENERATED_TESTS_RS), generated).map_err(|error| {
        format!(
            "failed to write generated upstream test runner {}: {error}",
            out_dir.join(GENERATED_TESTS_RS).display()
        )
    })
}

fn rust_test_name(case_name: &str) -> String {
    let mut name = String::from("upstream_");
    for ch in case_name.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch.to_ascii_lowercase());
        } else {
            name.push('_');
        }
    }
    name
}
