//! 根密钥落盘、引导凭证重放、轮换前后验签。
//!
//! 时刻全部来自 [`ManualClock`]，不睡眠。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use af_credential::{
    attenuate, authorize, deserialize_verify, inspect, issue_root, open_with_clock,
    rotate_with_clock, serialize, ChildSpec, GracePolicy, ManualClock, OpenOutcome, RootError,
    RootSpec,
};

fn spec() -> RootSpec {
    RootSpec {
        tools: vec!["git".into(), "test".into()],
        namespace: "line-a".into(),
        budget_cents: 1000,
    }
}

fn child() -> ChildSpec {
    ChildSpec {
        tools: vec!["test".into()],
        spend_cap_cents: 500,
    }
}

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "af-credential-root-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
}

fn key_bytes(dir: &Path) -> Vec<u8> {
    fs::read(dir.join("root.key")).unwrap()
}

#[test]
fn created_reopen_replays_same_key_and_bootstrap() {
    let dir = Temp::new();
    let clock = ManualClock::new(1_700_000_000_000);
    let opened = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap();
    assert_eq!(opened.outcome(), OpenOutcome::Created);
    assert_eq!(opened.generation().unwrap(), 1);
    assert_eq!(opened.bootstrap_generation(), 1);
    assert_eq!(opened.bootstrap_issued_at_ms(), 1_700_000_000_000);
    assert_eq!(opened.grace_until(1).unwrap(), None);
    assert_eq!(mode(&dir.path().join("root.key")), 0o600);
    assert_eq!(mode(&dir.path().join("credential.db")), 0o600);

    let token = opened.verify(opened.bootstrap_bytes()).unwrap();
    assert_eq!(token.root_key_id(), Some(1));
    let caps = inspect(&token).unwrap();
    assert!(caps.tools.contains("git") && caps.tools.contains("test"));
    assert_eq!(caps.namespace.as_deref(), Some("line-a"));
    assert_eq!(caps.budget_cents, Some(1000));
    assert!(authorize(&token, "test", "line-a", 100).unwrap());
    assert!(!authorize(&token, "git", "line-a", 1001).unwrap());

    let pk = opened.public_key().unwrap();
    assert!(deserialize_verify(opened.bootstrap_bytes(), &pk).is_ok());
    let foreign = issue_root(&spec()).unwrap();
    assert!(deserialize_verify(opened.bootstrap_bytes(), &foreign.keypair.public()).is_err());

    let file = key_bytes(dir.path());
    let boot = opened.bootstrap_bytes().to_vec();
    drop(opened);

    let again = open_with_clock(dir.path(), &spec(), clock).unwrap();
    assert_eq!(again.outcome(), OpenOutcome::AlreadyDeclared);
    assert_eq!(key_bytes(dir.path()), file);
    assert_eq!(again.bootstrap_bytes(), boot.as_slice());
    assert_eq!(again.public_key().unwrap().to_bytes(), pk.to_bytes());
    assert_eq!(again.bootstrap_issued_at_ms(), 1_700_000_000_000);
    assert_eq!(again.generation().unwrap(), 1);
    let replayed = again.verify(again.bootstrap_bytes()).unwrap();
    assert_eq!(replayed.root_key_id(), Some(1));
    assert!(authorize(&replayed, "git", "line-a", 100).unwrap());
}

#[test]
fn corrupt_key_fail_closed_does_not_regenerate() {
    let dir = Temp::new();
    let clock = ManualClock::new(10);
    let opened = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap();
    drop(opened);

    let path = dir.path().join("root.key");
    let mut original = fs::read(&path).unwrap();
    original[20] ^= 0xff;
    fs::write(&path, &original).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    let err = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap_err();
    assert!(matches!(err, RootError::CorruptKey { .. }), "{err}");
    assert_eq!(fs::read(&path).unwrap(), original);

    original.pop();
    fs::write(&path, &original).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let err = open_with_clock(dir.path(), &spec(), clock).unwrap_err();
    assert!(matches!(err, RootError::CorruptKey { .. }), "{err}");
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn bad_permissions_fail_closed_does_not_regenerate() {
    let dir = Temp::new();
    let clock = ManualClock::new(10);
    let opened = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap();
    let pk = opened.public_key().unwrap().to_bytes();
    drop(opened);

    let path = dir.path().join("root.key");
    let original = fs::read(&path).unwrap();
    for bad in [0o644u32, 0o400] {
        fs::set_permissions(&path, fs::Permissions::from_mode(bad)).unwrap();
        let err = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap_err();
        match err {
            RootError::BadPermissions { mode, .. } => assert_eq!(mode, bad),
            other => panic!("expected bad permissions, got {other}"),
        }
        assert_eq!(mode(&path), bad);
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let again = open_with_clock(dir.path(), &spec(), clock).unwrap();
    assert_eq!(again.outcome(), OpenOutcome::AlreadyDeclared);
    assert_eq!(again.public_key().unwrap().to_bytes(), pk);
}

#[test]
fn declaration_mismatch_fail_closed() {
    let dir = Temp::new();
    let clock = ManualClock::new(10);
    let opened = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap();
    drop(opened);
    let original = key_bytes(dir.path());

    let cases = [
        RootSpec {
            tools: spec().tools,
            namespace: "line-b".into(),
            budget_cents: 1000,
        },
        RootSpec {
            tools: vec!["test".into(), "git".into()],
            namespace: "line-a".into(),
            budget_cents: 1000,
        },
        RootSpec {
            tools: spec().tools,
            namespace: "line-a".into(),
            budget_cents: 1001,
        },
    ];
    let fields = ["namespace", "tools", "budget_cents"];
    for (spec, field) in cases.iter().zip(fields) {
        let err = open_with_clock(dir.path(), spec, clock.clone()).unwrap_err();
        match err {
            RootError::DeclarationMismatch { field: got } => assert_eq!(got, field),
            other => panic!("expected declaration mismatch on {field}, got {other}"),
        }
        assert_eq!(key_bytes(dir.path()), original);
    }
}

#[test]
fn partial_store_fail_closed() {
    let dir = Temp::new();
    let clock = ManualClock::new(10);
    let opened = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap();
    drop(opened);

    let key = key_bytes(dir.path());
    fs::remove_file(dir.path().join("credential.db")).unwrap();
    let err = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap_err();
    assert!(matches!(err, RootError::Incomplete { .. }), "{err}");
    assert_eq!(key_bytes(dir.path()), key);
    assert!(!dir.path().join("credential.db").exists());

    let dir = Temp::new();
    let opened = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap();
    drop(opened);
    fs::remove_file(dir.path().join("root.key")).unwrap();
    let err = open_with_clock(dir.path(), &spec(), clock).unwrap_err();
    assert!(matches!(err, RootError::Incomplete { .. }), "{err}");
    assert!(!dir.path().join("root.key").exists());
    assert!(dir.path().join("credential.db").is_file());
}

#[test]
fn symlink_root_key_fail_closed() {
    let dir = Temp::new();
    let clock = ManualClock::new(10);
    let opened = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap();
    drop(opened);

    let key = dir.path().join("root.key");
    let real = dir.path().join("real.key");
    fs::rename(&key, &real).unwrap();
    std::os::unix::fs::symlink(&real, &key).unwrap();
    let err = open_with_clock(dir.path(), &spec(), clock).unwrap_err();
    assert!(matches!(err, RootError::Inconsistent { .. }), "{err}");
    assert!(fs::symlink_metadata(&key).unwrap().file_type().is_symlink());
}

#[test]
fn orphan_journal_does_not_create_a_key() {
    let dir = Temp::new();
    fs::write(dir.path().join("root.key.pending"), b"orphan").unwrap();
    let err = open_with_clock(dir.path(), &spec(), ManualClock::new(1)).unwrap_err();
    assert!(matches!(err, RootError::Inconsistent { .. }), "{err}");
    assert!(!dir.path().join("root.key").exists());
    assert!(!dir.path().join("credential.db").exists());
}

#[test]
fn rotate_missing_store_does_not_create_a_key() {
    let missing = std::env::temp_dir().join(format!(
        "af-credential-missing-{}-{}",
        std::process::id(),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&missing);
    assert!(!missing.exists());
    let err = rotate_with_clock(
        &missing,
        &spec(),
        GracePolicy {
            grace: Duration::from_millis(1),
        },
        ManualClock::new(0),
    )
    .unwrap_err();
    assert!(matches!(err, RootError::NotInitialized { .. }), "{err}");
    assert!(!missing.exists());

    let dir = Temp::new();
    let err = rotate_with_clock(
        dir.path(),
        &spec(),
        GracePolicy {
            grace: Duration::ZERO,
        },
        ManualClock::new(0),
    )
    .unwrap_err();
    assert!(matches!(err, RootError::NotInitialized { .. }), "{err}");
    assert!(!dir.path().join("root.key").exists());
    assert!(!dir.path().join("credential.db").exists());
}

#[test]
fn rotation_verify_matrix() {
    let dir = Temp::new();
    let clock = ManualClock::new(10_000);
    let mut opened = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap();
    let old = opened.issue(&spec()).unwrap();
    let old_bytes = serialize(&old).unwrap();
    assert_eq!(old.root_key_id(), Some(1));
    let child_tok = attenuate(&old, &child()).unwrap();
    let child_bytes = serialize(&child_tok).unwrap();
    assert_eq!(child_tok.root_key_id(), Some(1));
    let boot = opened.bootstrap_bytes().to_vec();

    let rotation = opened
        .rotate(GracePolicy {
            grace: Duration::from_millis(1_000),
        })
        .unwrap();
    assert_eq!(rotation.previous_generation, 1);
    assert_eq!(rotation.generation, 2);
    assert_eq!(rotation.retired_at_ms, 10_000);
    assert_eq!(rotation.grace_until_ms, 11_000);
    assert_eq!(opened.generation().unwrap(), 2);
    assert_eq!(mode(&dir.path().join("root.key")), 0o600);
    assert_eq!(opened.bootstrap_bytes(), boot.as_slice());

    let old_v = opened.verify(&old_bytes).unwrap();
    assert_eq!(old_v.root_key_id(), Some(1));
    assert!(authorize(&old_v, "git", "line-a", 100).unwrap());
    let child_v = opened.verify(&child_bytes).unwrap();
    assert!(authorize(&child_v, "test", "line-a", 100).unwrap());
    assert!(!authorize(&child_v, "git", "line-a", 100).unwrap());
    assert!(opened.verify(&boot).is_ok());

    let new = opened.issue(&spec()).unwrap();
    let new_bytes = serialize(&new).unwrap();
    assert_eq!(new.root_key_id(), Some(2));
    assert_ne!(new_bytes, old_bytes);
    assert!(authorize(&opened.verify(&new_bytes).unwrap(), "git", "line-a", 100).unwrap());

    clock.set(10_999);
    assert!(opened.verify(&old_bytes).is_ok());
    assert!(opened.verify(&child_bytes).is_ok());
    assert!(opened.verify(&new_bytes).is_ok());

    clock.set(11_000);
    assert!(matches!(
        opened.verify(&old_bytes),
        Err(RootError::GraceExpired { generation: 1 })
    ));
    assert!(matches!(
        opened.verify(&child_bytes),
        Err(RootError::GraceExpired { generation: 1 })
    ));
    assert!(matches!(
        opened.verify(&boot),
        Err(RootError::GraceExpired { generation: 1 })
    ));
    assert!(opened.verify(&new_bytes).is_ok());

    let pk = opened.public_key().unwrap().to_bytes();
    drop(opened);
    let again = open_with_clock(dir.path(), &spec(), clock).unwrap();
    assert_eq!(again.outcome(), OpenOutcome::AlreadyDeclared);
    assert_eq!(again.generation().unwrap(), 2);
    assert_eq!(again.public_key().unwrap().to_bytes(), pk);
    assert_eq!(again.bootstrap_bytes(), boot.as_slice());
    assert_eq!(again.grace_until(1).unwrap(), Some(11_000));
    assert_eq!(again.grace_until(2).unwrap(), None);
    assert!(matches!(
        again.verify(&old_bytes),
        Err(RootError::GraceExpired { generation: 1 })
    ));
    assert!(again.verify(&new_bytes).is_ok());
}

#[test]
fn zero_grace_rejects_old_immediately() {
    let dir = Temp::new();
    let clock = ManualClock::new(500);
    let mut opened = open_with_clock(dir.path(), &spec(), clock).unwrap();
    let old = serialize(&opened.issue(&spec()).unwrap()).unwrap();
    let rotation = opened
        .rotate(GracePolicy {
            grace: Duration::ZERO,
        })
        .unwrap();
    assert_eq!(rotation.retired_at_ms, 500);
    assert_eq!(rotation.grace_until_ms, 500);
    assert!(matches!(
        opened.verify(&old),
        Err(RootError::GraceExpired { generation: 1 })
    ));
    let new = serialize(&opened.issue(&spec()).unwrap()).unwrap();
    assert_eq!(opened.verify(&new).unwrap().root_key_id(), Some(2));
}

#[test]
fn successive_rotations_keep_independent_grace() {
    let dir = Temp::new();
    let clock = ManualClock::new(1_000);
    let mut opened = open_with_clock(dir.path(), &spec(), clock.clone()).unwrap();
    let t1 = serialize(&opened.issue(&spec()).unwrap()).unwrap();
    let first = opened
        .rotate(GracePolicy {
            grace: Duration::from_millis(100),
        })
        .unwrap();
    assert_eq!(first.grace_until_ms, 1_100);

    clock.set(1_050);
    let t2 = serialize(&opened.issue(&spec()).unwrap()).unwrap();
    assert_eq!(opened.verify(&t2).unwrap().root_key_id(), Some(2));
    let second = opened
        .rotate(GracePolicy {
            grace: Duration::from_millis(1_000),
        })
        .unwrap();
    assert_eq!(second.previous_generation, 2);
    assert_eq!(second.generation, 3);
    assert_eq!(second.retired_at_ms, 1_050);
    assert_eq!(second.grace_until_ms, 2_050);

    assert!(opened.verify(&t1).is_ok());
    assert!(opened.verify(&t2).is_ok());
    clock.set(1_100);
    assert!(matches!(
        opened.verify(&t1),
        Err(RootError::GraceExpired { generation: 1 })
    ));
    assert!(opened.verify(&t2).is_ok());
    clock.set(2_050);
    assert!(matches!(
        opened.verify(&t2),
        Err(RootError::GraceExpired { generation: 2 })
    ));
    let t3 = serialize(&opened.issue(&spec()).unwrap()).unwrap();
    assert_eq!(opened.verify(&t3).unwrap().root_key_id(), Some(3));

    drop(opened);
    let again = open_with_clock(dir.path(), &spec(), clock).unwrap();
    assert_eq!(again.grace_until(1).unwrap(), Some(1_100));
    assert_eq!(again.grace_until(2).unwrap(), Some(2_050));
    assert_eq!(again.grace_until(3).unwrap(), None);
    assert_eq!(again.generation().unwrap(), 3);
    assert!(matches!(
        again.verify(&t1),
        Err(RootError::GraceExpired { generation: 1 })
    ));
    assert!(again.verify(&t3).is_ok());
}

#[test]
fn token_without_root_key_id_is_rejected() {
    let dir = Temp::new();
    let opened = open_with_clock(dir.path(), &spec(), ManualClock::new(1)).unwrap();
    let ephemeral = issue_root(&spec()).unwrap();
    let bytes = serialize(&ephemeral.token).unwrap();
    let err = opened.verify(&bytes).unwrap_err();
    assert!(matches!(err, RootError::MissingRootKeyId), "{err}");
}
