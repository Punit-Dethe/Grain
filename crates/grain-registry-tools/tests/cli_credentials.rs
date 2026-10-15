use std::process::Command;

#[test]
fn reviewed_staging_accepts_unlock_flags_but_read_only_commands_refuse_them() {
    let owned = tempfile::tempdir().unwrap();
    std::fs::create_dir(owned.path().join("empty-store")).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_grain-registry"))
            .args(args)
            .current_dir(owned.path())
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let staged = run(&[
        "stage-reviewed-release",
        "--submission",
        "submission",
        "--prepared",
        "prepared",
        "--candidate",
        "candidate",
        "--policy",
        "policy.json",
        "--policy-sha256",
        &"1".repeat(64),
        "--authority",
        "ownership.json",
        "--gh",
        "gh",
        "--attestation",
        "attestation.jsonl",
        "--key",
        "publisher.key",
        "--store",
        "empty-store",
        "--expected-current-sha256",
        &"2".repeat(64),
        "--out",
        "release",
        "--key-password-file",
        "missing-password",
    ]);
    assert!(!staged.status.success());
    let error = String::from_utf8_lossy(&staged.stderr);
    assert!(
        error.contains("Release base verification failed"),
        "{error}"
    );
    assert!(!error.contains("Credential options apply only"), "{error}");
    assert!(!owned.path().join("release").exists());

    let read_only = run(&[
        "verify-hosting-bundle",
        "--bundle",
        "missing-bundle",
        "--expected-receipt-sha256",
        &"3".repeat(64),
        "--key-password-file",
        "missing-password",
    ]);
    assert!(!read_only.status.success());
    assert!(String::from_utf8_lossy(&read_only.stderr).contains("Credential options apply only"));
}
