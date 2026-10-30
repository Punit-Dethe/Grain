//! Normal CI-intake regressions using the existing author fixture and real prior
//! signature. Only the network provenance callback is substituted; the public
//! command always uses the pinned official GH verifier for both subjects.
use super::*;
use serde_json::{json, Value};

struct Fixture {
    _source: crate::prepare::tests::Fixture,
    args: Args,
}

fn fixture(mcp: bool) -> Fixture {
    let source = crate::prepare::tests::fixture(mcp, true);
    let root = source.out.parent().unwrap();
    crate::prepare::prepare(
        source.submission.clone(),
        root.join("tools"),
        source.out.clone(),
    )
    .unwrap();
    let (receipt, producer) = crate::prepare::tests::receipt_pins(&source);
    let evidence = root.join("evidence");
    fs::create_dir(&evidence).unwrap();
    fs::rename(&source.out, evidence.join("prepared")).unwrap();
    crate::catalogue::prepare(
        &source.submission,
        &evidence.join("prepared"),
        &receipt,
        &producer,
        &evidence.join("candidate"),
    )
    .unwrap();
    let operator = root.join("operator");
    fs::create_dir(&operator).unwrap();
    let gh = operator.join("gh-fixture");
    fs::write(&gh, b"fixture verifier; never executed").unwrap();
    crate::keygen(
        operator.join("keys"),
        "test".into(),
        &crate::Unlock::development(),
    )
    .unwrap();
    let public = minisign::PublicKeyBox::from_string(
        &fs::read_to_string(operator.join("keys/test.pub")).unwrap(),
    )
    .unwrap()
    .into_public_key()
    .unwrap()
    .to_base64();
    let previous = root.join("previous");
    fs::create_dir(&previous).unwrap();
    let index = serde_json::to_vec(
        &json!({"spec":1,"version":7,"expires":"2099-01-01T00:00:00Z","entries":[]}),
    )
    .unwrap();
    fs::write(previous.join("index.json"), &index).unwrap();
    fs::write(
        previous.join("index.json.minisig"),
        crate::sign_bytes(
            &operator.join("keys/test.key"),
            &index,
            &crate::Unlock::development(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut policy = crate::review::tests::policy();
    policy.candidate_sha256.clear();
    policy.submission_sha256.clear();
    policy.receipt_sha256.clear();
    policy.authority_sha256.clear();
    policy.previous_index_sha256.clear();
    policy.previous_index_version = 0;
    policy.approved_at.clear();
    policy.expires_at.clear();
    policy.producer_sha256 = producer.clone();
    policy.verifier_sha256 = digest(&fs::read(&gh).unwrap());
    policy.publishing_public_key = public.clone();
    let template = operator.join("template.json");
    fs::write(&template, serde_json::to_vec(&policy).unwrap()).unwrap();
    let authority = operator.join("ownership.json");
    fs::write(&authority, serde_json::to_vec(&json!({"schema":1,"registry_repo":policy.registry_repo,"registry_ref":policy.registry_ref,
        "publishing_public_key":public,"reviewers":["reviewer","example"],"extensions":[{"id":source.value.id,
        "source_repo":source.value.source_repo,"artifact_kind":source.value.artifact_kind,"maintainers":["example"]}]})).unwrap()).unwrap();
    let tooling_commit = "d".repeat(40);
    fs::write(
        evidence.join("producer.json"),
        serde_json::to_vec(
            &json!({"schema":1,"evidence_class":"pinned-remote-source-build/not-human-approved",
        "grain_commit":tooling_commit,"producer_sha256":producer,"receipt_sha256":receipt,
        "submission_sha256":digest(&serde_json::to_vec(&source.value).unwrap()),
        "candidate_sha256":digest(&fs::read(evidence.join("candidate/candidate.json")).unwrap())}),
        )
        .unwrap(),
    )
    .unwrap();
    fs::write(
        evidence.join("attestation.jsonl"),
        b"fixture callback owns provenance",
    )
    .unwrap();
    let args = Args {
        submission: source.submission.clone(),
        evidence,
        template,
        authority,
        previous,
        tooling_commit,
        gh,
        out: root.join("review"),
    };
    Fixture {
        _source: source,
        args,
    }
}

fn edit(path: &std::path::Path, field: &str, value: Value) {
    let mut object: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    object[field] = value;
    fs::write(path, serde_json::to_vec(&object).unwrap()).unwrap();
}

#[test]
fn native_and_mcp_intake_emit_exact_drafts_that_cannot_be_used_as_approval() {
    for mcp in [false, true] {
        let f = fixture(mcp);
        prepare_with(&f.args, |_, policy, scratch| {
            policy.validate_inputs()?;
            assert!(policy.validate(chrono::Utc::now().timestamp()).is_err());
            assert_eq!(
                digest(&read(&scratch.join("snapshot/candidate.json"), 65536)?),
                policy.candidate_sha256
            );
            assert!(scratch.join("producer.json").is_file());
            assert!(scratch.join("attestation.jsonl").is_file());
            Ok(())
        })
        .unwrap();
        let policy: Policy =
            serde_json::from_slice(&fs::read(f.args.out.join("policy.json")).unwrap()).unwrap();
        assert_eq!(policy.previous_index_version, 7);
        assert!(policy.approved_at.is_empty() && policy.expires_at.is_empty());
        assert!(policy.validate(chrono::Utc::now().timestamp()).is_err());
        assert_eq!(
            policy.authority_sha256,
            digest(&fs::read(&f.args.authority).unwrap())
        );
        let summary: Value =
            serde_json::from_slice(&fs::read(f.args.out.join("review.json")).unwrap()).unwrap();
        assert_eq!(
            summary["source"]["artifact_kind"],
            if mcp { "mcp-descriptor" } else { "native" }
        );
        assert!(prepare_with(&f.args, |_, _, _| panic!(
            "existing output reached provenance"
        ))
        .is_err());
    }
}

#[test]
fn drifted_tooling_producer_receipt_or_candidate_refuses_before_provenance() {
    let f = fixture(true);
    let producer = f.args.evidence.join("producer.json");
    let original = fs::read(&producer).unwrap();
    for (field, value) in [
        ("grain_commit", json!("f".repeat(40))),
        ("producer_sha256", json!("f".repeat(64))),
        ("receipt_sha256", json!("f".repeat(64))),
        ("submission_sha256", json!("f".repeat(64))),
        ("candidate_sha256", json!("f".repeat(64))),
        ("skip_verification", json!(true)),
    ] {
        edit(&producer, field, value);
        assert!(
            prepare_with(&f.args, |_, _, _| panic!(
                "unvalidated evidence reached provenance"
            ))
            .is_err(),
            "{field}"
        );
        assert!(!f.args.out.exists());
        fs::write(&producer, &original).unwrap();
    }
    let candidate = f.args.evidence.join("candidate/candidate.json");
    let mut changed = fs::read(&candidate).unwrap();
    changed.push(b' ');
    fs::write(candidate, changed).unwrap();
    assert!(prepare_with(&f.args, |_, _, _| panic!(
        "changed candidate reached provenance"
    ))
    .is_err());
}

#[test]
fn provenance_failure_or_changed_current_ownership_emits_no_policy() {
    let f = fixture(true);
    assert!(prepare_with(&f.args, |_, _, _| bail!("provenance refused")).is_err());
    assert!(!f.args.out.exists());
    assert!(prepare_with(&f.args, |_, _, _| {
        edit(&f.args.authority, "reviewers", json!(["removed-reviewer"]));
        Ok(())
    })
    .is_err());
    assert!(!f.args.out.exists());
}

#[test]
fn approved_templates_bad_prior_signatures_and_nested_output_are_refused() {
    let f = fixture(false);
    let original = fs::read(&f.args.template).unwrap();
    for (field, value) in [
        ("approved_at", json!("2026-10-15T00:00:00Z")),
        ("maintainer_approval", json!("previous approval")),
        ("candidate_sha256", json!("f".repeat(64))),
    ] {
        edit(&f.args.template, field, value);
        assert!(prepare_with(&f.args, |_, _, _| panic!(
            "approved template reached provenance"
        ))
        .is_err());
        fs::write(&f.args.template, &original).unwrap();
    }
    let signature = f.args.previous.join("index.json.minisig");
    let old = fs::read(&signature).unwrap();
    fs::write(&signature, b"invalid").unwrap();
    assert!(prepare_with(&f.args, |_, _, _| panic!("bad base reached provenance")).is_err());
    fs::write(signature, old).unwrap();
    let mut args = f.args;
    args.out = args.evidence.join("nested-output");
    assert!(prepare_with(&args, |_, _, _| panic!("nested output reached provenance")).is_err());
    assert!(!args.out.exists());
}
