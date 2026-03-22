use std::env;
use std::fs;
use std::path::Path;

#[test]
fn native_source_audit_report_is_traceable() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let report_path = Path::new(&out_dir).join("native-source-audit-report.txt");
    
    assert!(report_path.exists(), "native-source-audit-report.txt should exist");
    let content = fs::read_to_string(&report_path).expect("report should be readable");
    assert!(content.contains("native source audit report"));
    assert!(content.contains("status: passed"), "actual native/ should pass audit");
}

#[test]
fn archive_member_audit_report_is_traceable() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let report_path = Path::new(&out_dir).join("archive-member-audit-report.txt");
    
    assert!(report_path.exists(), "archive-member-audit-report.txt should exist");
    let content = fs::read_to_string(&report_path).expect("report should be readable");
    assert!(content.contains("archive member audit report"));
    assert!(content.contains("status: passed"), "actual archive should pass member audit");
}

#[test]
fn normal_delegation_proof_is_traceable() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let report_path = Path::new(&out_dir).join("normal-delegation-proof.txt");
    
    assert!(report_path.exists(), "normal-delegation-proof.txt should exist");
    let content = fs::read_to_string(&report_path).expect("report should be readable");
    assert!(content.contains("normal delegation proof report"));
    assert!(content.contains("status: passed"), "actual archive should pass normal delegation proof");
}

#[test]
fn ex_delegation_proof_is_traceable() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let report_path = Path::new(&out_dir).join("ex-delegation-proof.txt");
    
    assert!(report_path.exists(), "ex-delegation-proof.txt should exist");
    let content = fs::read_to_string(&report_path).expect("report should be readable");
    assert!(content.contains("ex delegation proof report"));
    assert!(content.contains("status: passed"), "actual archive should pass ex delegation proof");
}

#[test]
fn compile_proof_is_traceable() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR should be set during cargo test");
    let proof_path = Path::new(&out_dir).join("upstream_build_fingerprint.json");
    
    assert!(proof_path.exists(), "upstream_build_fingerprint.json should exist");
    let content = fs::read_to_string(&proof_path).expect("proof should be readable");
    
    let proof: serde_json::Value = serde_json::from_str(&content).expect("proof should be valid JSON");
    assert!(proof.get("tag").is_some());
    assert!(proof.get("commit").is_some());
    assert!(proof.get("native_sources").is_some());
    assert!(proof.get("vendor_sources").is_some());
}
