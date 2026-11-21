use super::*;
fn artifacts(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = vec![];
    for d in fs::read_dir(root).unwrap() {
        for f in fs::read_dir(d.unwrap().path()).unwrap() {
            let p = f.unwrap().path();
            out.push((p.clone(), fs::read(p).unwrap()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
#[test]
fn shared_outcomes_distinguish_native_failure_present_absent_conflict_and_unknown() {
    for (mode, status, process, package) in [
        ("present-fail", "process_failed", "failed", "exact_present"),
        ("absent", "package_absent", "success", "absent"),
        (
            "unavailable",
            "package_unavailable",
            "success",
            "unavailable",
        ),
        ("conflict", "package_conflict", "success", "conflict"),
    ] {
        let f = Fixture::new(mode);
        let out = f.install().arg("--yes").output().unwrap();
        assert_eq!(out.status.code(), Some(1));
        let decoded: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let report = &decoded["installation"]["results"][0]["report"];
        assert_eq!(report["summary"]["status"], status);
        assert_eq!(report["outcome"]["process"], process);
        assert_eq!(report["outcome"]["package"], package);
        assert_eq!(report["summary"]["package"], package);
        assert_eq!(report["summary"]["binary"], "missing");
        assert_eq!(report["summary"]["readiness_verified"], false);
        assert_eq!(report["summary"]["next_step"], "fresh_review");
        assert!(
            report["summary"]["next_step_hint"]
                .as_str()
                .unwrap()
                .contains("Check installed again")
        );
        let root = f.host.state_dir.join("tools-attempts");
        let before = artifacts(&root);
        let calls = f.calls();
        let attempts = f.attempts();
        assert_eq!(attempts[0]["summary"], report["summary"]);
        assert_eq!(attempts[0]["unresolved"], false);
        assert_eq!(before, artifacts(&root));
        assert_eq!(calls, f.calls());
        assert_eq!(calls, "QQIQ");
        for (_, bytes) in &before {
            let text = std::str::from_utf8(bytes).unwrap();
            assert!(!text.contains("summary") && !text.contains("PRIVATE"));
        }
        assert!(!String::from_utf8(out.stdout).unwrap().contains("PRIVATE"));
        if mode == "present-fail" {
            assert!(
                report["summary"]["explanation"]
                    .as_str()
                    .unwrap()
                    .contains("even though a package was reported present")
            );
        }
    }
}
