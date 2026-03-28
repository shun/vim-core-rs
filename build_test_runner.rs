use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const UPSTREAM_TESTDIR: &str = "vendor/vim_src/src/testdir";
const UPSTREAM_SKIPLIST: &str = "upstream-test-skiplist.txt";
const UPSTREAM_CLASSIFICATION_MANIFEST: &str = "upstream-test-classification.json";
const GENERATED_TESTS_RS: &str = "upstream_vim_tests.rs";
const GENERATED_MANIFEST_JSON: &str = "upstream_test_manifest.json";
const COMPATIBILITY_ADR: &str = "docs/adr/0002-define-compatibility-boundaries.md";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamTestClassification {
    PreserveDirectly,
    PreserveThroughAdaptation,
    OutOfScope,
    TemporarilyExcluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedSelectionStatus {
    Included,
    ExcludedByPolicy,
    TemporarilyExcluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedUpstreamTestCase {
    pub name: String,
    pub relative_path: String,
    pub classification: UpstreamTestClassification,
    pub selection_status: GeneratedSelectionStatus,
    pub selected_for_generated_runner: bool,
    pub exclusion_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedUpstreamTestManifest {
    pub cases: Vec<GeneratedUpstreamTestCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ClassificationManifest {
    counts: ClassificationCounts,
    cases: Vec<ClassificationCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ClassificationCounts {
    total_cases: usize,
    preserve_directly: usize,
    preserve_through_adaptation: usize,
    out_of_scope: usize,
    temporarily_excluded: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ClassificationCase {
    name: String,
    relative_path: String,
    classification: UpstreamTestClassification,
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
    if !test_dir.exists() {
        let manifest = GeneratedUpstreamTestManifest { cases: Vec::new() };
        write_manifest(out_dir, &manifest)?;
        write_generated_runner(out_dir, &manifest)?;
        return Ok(());
    }

    let skiplist_path = repo_root.join(UPSTREAM_SKIPLIST);
    let classification_path = repo_root.join(UPSTREAM_CLASSIFICATION_MANIFEST);
    let skiplist = parse_skiplist(&skiplist_path)?;
    let classification_manifest = parse_classification_manifest(&classification_path)?;
    let manifest = GeneratedUpstreamTestManifest {
        cases: collect_cases(repo_root, &test_dir, &skiplist, &classification_manifest)?,
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

fn parse_classification_manifest(
    classification_path: &Path,
) -> Result<ClassificationManifest, String> {
    let content = fs::read_to_string(classification_path).map_err(|error| {
        format!(
            "failed to read upstream test classification manifest {}: {error}",
            classification_path.display()
        )
    })?;
    let manifest: ClassificationManifest = serde_json::from_str(&content).map_err(|error| {
        format!(
            "failed to parse upstream test classification manifest {}: {error}",
            classification_path.display()
        )
    })?;

    validate_classification_counts(&manifest, classification_path)?;
    Ok(manifest)
}

fn validate_classification_counts(
    manifest: &ClassificationManifest,
    classification_path: &Path,
) -> Result<(), String> {
    let mut preserve_directly = 0usize;
    let mut preserve_through_adaptation = 0usize;
    let mut out_of_scope = 0usize;
    let mut temporarily_excluded = 0usize;

    for case in &manifest.cases {
        match case.classification {
            UpstreamTestClassification::PreserveDirectly => preserve_directly += 1,
            UpstreamTestClassification::PreserveThroughAdaptation => {
                preserve_through_adaptation += 1
            }
            UpstreamTestClassification::OutOfScope => out_of_scope += 1,
            UpstreamTestClassification::TemporarilyExcluded => temporarily_excluded += 1,
        }
    }

    let actual = ClassificationCounts {
        total_cases: manifest.cases.len(),
        preserve_directly,
        preserve_through_adaptation,
        out_of_scope,
        temporarily_excluded,
    };

    if manifest.counts != actual {
        return Err(format!(
            "upstream test classification counts in {} do not match case list: expected {:?}, actual {:?}",
            classification_path.display(),
            manifest.counts,
            actual
        ));
    }

    Ok(())
}

fn collect_cases(
    repo_root: &Path,
    test_dir: &Path,
    skiplist: &BTreeMap<String, String>,
    classification_manifest: &ClassificationManifest,
) -> Result<Vec<GeneratedUpstreamTestCase>, String> {
    let vendored_cases = collect_vendored_cases(repo_root, test_dir)?;
    validate_manifest_alignment(&vendored_cases, classification_manifest, skiplist)?;

    let mut cases = Vec::new();
    for case in &classification_manifest.cases {
        let selection = determine_selection(case, skiplist)?;
        cases.push(GeneratedUpstreamTestCase {
            name: case.name.clone(),
            relative_path: case.relative_path.clone(),
            classification: case.classification.clone(),
            selection_status: selection.status,
            selected_for_generated_runner: selection.selected,
            exclusion_reason: selection.reason,
        });
    }

    cases.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(cases)
}

fn collect_vendored_cases(
    repo_root: &Path,
    test_dir: &Path,
) -> Result<BTreeMap<String, String>, String> {
    let mut cases = BTreeMap::new();
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
        cases.insert(file_name.to_string(), relative_path);
    }

    Ok(cases)
}

fn validate_manifest_alignment(
    vendored_cases: &BTreeMap<String, String>,
    classification_manifest: &ClassificationManifest,
    skiplist: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut manifest_names = BTreeSet::new();
    for case in &classification_manifest.cases {
        if !manifest_names.insert(case.name.clone()) {
            return Err(format!(
                "upstream test classification manifest contains duplicate case entry: {}",
                case.name
            ));
        }

        let Some(vendored_relative_path) = vendored_cases.get(&case.name) else {
            return Err(format!(
                "upstream test classification manifest references non-vendored case: {}",
                case.name
            ));
        };

        if vendored_relative_path != &case.relative_path {
            return Err(format!(
                "upstream test classification manifest has mismatched path for {}: expected {}, got {}",
                case.name, vendored_relative_path, case.relative_path
            ));
        }
    }

    for vendored_name in vendored_cases.keys() {
        if !manifest_names.contains(vendored_name) {
            return Err(format!(
                "vendored upstream test is missing from classification manifest: {}",
                vendored_name
            ));
        }
    }

    for skipped_name in skiplist.keys() {
        let Some(case) = classification_manifest
            .cases
            .iter()
            .find(|case| case.name == *skipped_name)
        else {
            return Err(format!(
                "upstream test skiplist references unknown case: {}",
                skipped_name
            ));
        };

        if case.classification != UpstreamTestClassification::TemporarilyExcluded {
            return Err(format!(
                "upstream test skiplist entry {} must be classified as temporarily_excluded",
                skipped_name
            ));
        }
    }

    Ok(())
}

struct CaseSelection {
    selected: bool,
    status: GeneratedSelectionStatus,
    reason: Option<String>,
}

fn determine_selection(
    case: &ClassificationCase,
    skiplist: &BTreeMap<String, String>,
) -> Result<CaseSelection, String> {
    match case.classification {
        UpstreamTestClassification::PreserveDirectly => Ok(CaseSelection {
            selected: true,
            status: GeneratedSelectionStatus::Included,
            reason: None,
        }),
        UpstreamTestClassification::PreserveThroughAdaptation => Ok(CaseSelection {
            selected: false,
            status: GeneratedSelectionStatus::ExcludedByPolicy,
            reason: Some(
                "Covered by repository contract tests for adapted host-boundary behavior."
                    .to_string(),
            ),
        }),
        UpstreamTestClassification::OutOfScope => Ok(CaseSelection {
            selected: false,
            status: GeneratedSelectionStatus::ExcludedByPolicy,
            reason: Some(format!(
                "Out of scope per {}.",
                COMPATIBILITY_ADR
            )),
        }),
        UpstreamTestClassification::TemporarilyExcluded => {
            let reason = skiplist.get(&case.name).cloned().ok_or_else(|| {
                format!(
                    "temporarily_excluded case {} is missing a skiplist reason",
                    case.name
                )
            })?;
            Ok(CaseSelection {
                selected: false,
                status: GeneratedSelectionStatus::TemporarilyExcluded,
                reason: Some(reason),
            })
        }
    }
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

    let selected_cases = manifest
        .cases
        .iter()
        .filter(|case| case.selected_for_generated_runner)
        .collect::<Vec<_>>();

    if selected_cases.is_empty() {
        generated.push_str("// No upstream Vim test cases were selected for the generated runner.\n");
    } else {
        for case in selected_cases {
            generated.push_str("#[test]\n");
            generated.push_str(&format!("fn {}() {{\n", rust_test_name(&case.name)));
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
