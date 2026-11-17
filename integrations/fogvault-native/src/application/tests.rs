use super::*;
use std::process::Command;

const SECRET: &str = "private-sentinel@example.test";
fn options(path: &Path) -> Options {
    let mut options = Options::new("test.app");
    options.directory = Some(path.to_owned());
    options.key = Some(Zeroizing::new([7; 32]));
    options
}
fn finding(text: &str) -> WorkspaceFinding {
    WorkspaceFinding::new(
        "EMAIL",
        text,
        datafog_core::TextRange {
            start: 0,
            end: text.len(),
        },
    )
}
fn root() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
fn canonical(temp: &tempfile::TempDir) -> PathBuf {
    temp.path().canonicalize().unwrap().join("app")
}
fn commit(owner: &mut ApplicationVault, text: &str) -> Committed {
    owner
        .prepare_and_commit(text, &[finding(text)], random().unwrap())
        .unwrap()
}

#[test]
fn stale_owners_refresh_cas_and_forget_preserve_contract() {
    let temp = root();
    let path = canonical(&temp);
    let mut first = ApplicationVault::open(options(&path)).unwrap();
    let mut stale = ApplicationVault::open(options(&path)).unwrap();
    let saved = commit(&mut first, SECRET);
    assert_eq!(stale.restore(&saved.texts[0]).unwrap(), SECRET);
    let second = commit(&mut stale, "other-sentinel@example.test");
    assert_eq!(
        first.restore(&second.texts[0]).unwrap(),
        "other-sentinel@example.test"
    );
    assert_eq!(first.restore(&saved.texts[0]).unwrap(), SECRET);
    let old_head = saved.head;
    let bundle = Zeroizing::new(first.workspace.export_bundle().unwrap());
    assert!(matches!(
        first.persist(Some(old_head), old_head, &bundle),
        Err(Error::Conflict)
    ));
    let code = saved.texts[0]
        .trim_start_matches("[[EMAIL_")
        .trim_end_matches("]]")
        .to_string();
    let plan = first
        .workspace
        .prepare_forget(std::slice::from_ref(&code), random().unwrap())
        .unwrap();
    assert_eq!(plan.snapshot_format().unwrap().candidate_version, 4);
    drop(plan);
    first.forget(&[code], random().unwrap()).unwrap();
    let raw = Zeroizing::new(first.workspace.export_bundle().unwrap());
    let older_error = published_beta3::workspace::Workspace::open_bundle(&raw)
        .err()
        .unwrap();
    assert_eq!(older_error.code.as_str(), "invalid_snapshot");
    assert!(!older_error.to_string().contains(SECRET));
    let mut reopened = ApplicationVault::open(options(&path)).unwrap();
    let report = reopened.restore_report(&saved.texts[0]).unwrap();
    assert!(report.text.is_none());
    assert_eq!(report.issues[0].kind.as_str(), "expired");
    assert_eq!(
        reopened.restore(&second.texts[0]).unwrap(),
        "other-sentinel@example.test"
    );
}

#[test]
fn ciphertext_metadata_keys_and_errors_fail_closed_without_values() {
    let temp = root();
    let path = canonical(&temp);
    let mut owner = ApplicationVault::open(options(&path)).unwrap();
    let saved = commit(&mut owner, SECRET);
    let mut missing = Options::new("test.app");
    missing.directory = Some(path.clone());
    assert!(matches!(
        ApplicationVault::open(missing),
        Err(Error::KeyRequired)
    ));
    let ciphertext = fs::read(path.join("workspace.enc")).unwrap();
    assert!(
        !ciphertext
            .windows(SECRET.len())
            .any(|w| w == SECRET.as_bytes())
    );
    let mut wrong = options(&path);
    wrong.key = Some(Zeroizing::new([8; 32]));
    assert!(matches!(
        ApplicationVault::open(wrong),
        Err(Error::Integrity)
    ));
    let mut wrong_app = options(&path);
    wrong_app.app_id = "other.app".into();
    assert!(matches!(
        ApplicationVault::open(wrong_app),
        Err(Error::Integrity)
    ));
    let mut tampered = ciphertext.clone();
    tampered[14] ^= 1;
    atomic_write(&path, &path.join("workspace.enc"), &tampered).unwrap();
    let error = owner.restore(&saved.texts[0]).unwrap_err();
    assert!(!error.to_string().contains(SECRET));
    assert!(!format!("{error:?}").contains(SECRET));
    atomic_write(&path, &path.join("workspace.enc"), &ciphertext).unwrap();
    assert_eq!(owner.restore(&saved.texts[0]).unwrap(), SECRET);
    let before = owner.head().unwrap();
    let oversized = "x".repeat(core::MAX_SOURCE_BYTES + 1);
    assert!(matches!(
        owner.prepare_and_commit(&oversized, &[], random().unwrap()),
        Err(Error::Core(core::WorkspaceError {
            code: core::ErrorCode::ResourceLimit,
            ..
        }))
    ));
    assert_eq!(owner.head().unwrap(), before);
    owner.close();
    assert!(matches!(owner.restore(&saved.texts[0]), Err(Error::Closed)));
}

#[test]
fn file_key_is_explicit_encrypted_and_never_replaces_existing_key_mode() {
    let temp = root();
    let path = canonical(&temp);
    let mut opts = Options::new("file.app");
    opts.directory = Some(path.clone());
    opts.allow_file_key = true;
    let mut owner = ApplicationVault::open(opts).unwrap();
    assert_eq!(owner.protection_mode(), Protection::FileKey);
    let saved = commit(&mut owner, SECRET);
    assert!(
        !fs::read(path.join("workspace.enc"))
            .unwrap()
            .windows(SECRET.len())
            .any(|w| w == SECRET.as_bytes())
    );
    let mut refused = Options::new("file.app");
    refused.directory = Some(path.clone());
    assert!(matches!(
        ApplicationVault::open(refused),
        Err(Error::KeyRequired)
    ));
    let mut reopen = Options::new("file.app");
    reopen.directory = Some(path);
    reopen.allow_file_key = true;
    assert_eq!(
        ApplicationVault::open(reopen)
            .unwrap()
            .restore(&saved.texts[0])
            .unwrap(),
        SECRET
    );
}

#[test]
fn unavailable_default_provider_never_silently_downgrades() {
    struct Unavailable;
    impl keyring::credential::CredentialBuilderApi for Unavailable {
        fn build(
            &self,
            _: Option<&str>,
            _: &str,
            _: &str,
        ) -> keyring::Result<Box<keyring::credential::Credential>> {
            Err(keyring::Error::NoEntry)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
    // Other application tests use explicit synthetic keys; no physical provider
    // is read or written. This process-local failing provider needs no OS service.
    keyring::set_default_credential_builder(Box::new(Unavailable));
    let temp = root();
    let path = canonical(&temp);
    let mut opts = Options::new("provider.app");
    opts.directory = Some(path.clone());
    assert!(matches!(
        ApplicationVault::open(opts),
        Err(Error::CredentialUnavailable)
    ));
    assert!(!path.join("workspace.enc").exists());
    assert!(!path.join("workspace.key").exists());
    let mut opts = Options::new("provider.app");
    opts.directory = Some(path);
    opts.allow_file_key = true;
    assert_eq!(
        ApplicationVault::open(opts).unwrap().protection_mode(),
        Protection::FileKey
    );
}

#[test]
fn backend_cas_rejects_wrong_head_and_wrong_encrypted_generation() {
    let temp = root();
    let path = canonical(&temp);
    let mut owner = ApplicationVault::open(options(&path)).unwrap();
    let backend = LocalBackend { directory: &path };
    let before = backend.load().unwrap().unwrap();
    let head = owner.head().unwrap();
    let mut wrong = head;
    wrong.revision += 1;
    assert!(matches!(
        backend.compare_and_save(Some(wrong), Some(&before), &before),
        Err(Error::Conflict)
    ));
    commit(&mut owner, SECRET);
    assert!(matches!(
        backend.compare_and_save(Some(head), Some(&before), &before),
        Err(Error::Conflict)
    ));
    let marker = owner
        .workspace
        .prepare(SECRET, &[finding(SECRET)], random().unwrap())
        .unwrap()
        .previews()
        .unwrap()[0]
        .text
        .clone();
    assert_eq!(owner.restore(&marker).unwrap(), SECRET);
}

#[test]
fn crashed_transaction_child() {
    let Some(path) = std::env::var_os("DATAFOG_APPLICATION_TEST_CHILD") else {
        return;
    };
    let mut owner = ApplicationVault::open(options(Path::new(&path))).unwrap();
    owner
        .prepare_and_commit(SECRET, &[finding(SECRET)], [42; 16])
        .unwrap();
    panic!("crash point did not execute");
}

#[test]
fn real_process_crashes_leave_old_or_complete_new_snapshot_and_release_lock() {
    for point in ["before_replace", "after_replace", "before_activate"] {
        let temp = root();
        let path = canonical(&temp);
        let mut owner = ApplicationVault::open(options(&path)).unwrap();
        let original = commit(&mut owner, "original@example.test");
        let before = owner.head().unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "application::tests::crashed_transaction_child",
                "--nocapture",
            ])
            .env("DATAFOG_APPLICATION_TEST_CHILD", &path)
            .env("DATAFOG_APPLICATION_TEST_CRASH", point)
            .output()
            .unwrap();
        assert_eq!(
            child.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        let mut recovered = ApplicationVault::open(options(&path)).unwrap();
        assert_eq!(
            recovered.restore(&original.texts[0]).unwrap(),
            "original@example.test"
        );
        if point == "before_replace" {
            assert_eq!(recovered.head().unwrap(), before);
            assert!(
                !recovered
                    .operations()
                    .unwrap()
                    .iter()
                    .any(|o| o.id == [42; 16])
            );
        } else {
            assert_eq!(recovered.head().unwrap().commit_id, [42; 16]);
            let plan = recovered
                .workspace
                .prepare(SECRET, &[finding(SECRET)], [43; 16])
                .unwrap();
            let marker = plan.previews().unwrap()[0].text.clone();
            assert_eq!(recovered.restore(&marker).unwrap(), SECRET);
        }
        for entry in fs::read_dir(&path).unwrap() {
            let bytes = fs::read(entry.unwrap().path()).unwrap();
            assert!(!bytes.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()));
        }
    }
}

#[cfg(unix)]
#[test]
fn unix_permissions_symlinks_hardlinks_and_failed_save_are_fail_closed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = root();
    let path = canonical(&temp);
    let mut owner = ApplicationVault::open(options(&path)).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(path.join("workspace.enc"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let before = owner.head().unwrap();
    fs::set_permissions(
        path.join("workspace.enc"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(matches!(
        owner.prepare_and_commit(SECRET, &[finding(SECRET)], random().unwrap()),
        Err(Error::Permissions)
    ));
    assert_eq!(owner.head().unwrap(), before);
    fs::set_permissions(
        path.join("workspace.enc"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let alias = path.join("alias");
    fs::hard_link(path.join("workspace.enc"), &alias).unwrap();
    assert!(matches!(owner.restore("plain"), Err(Error::Permissions)));
    fs::remove_file(alias).unwrap();
    let link = temp.path().canonicalize().unwrap().join("link");
    symlink(&path, &link).unwrap();
    assert!(matches!(
        ApplicationVault::open(options(&link)),
        Err(Error::UnsafePath)
    ));
    assert!(matches!(
        ApplicationVault::open(Options::new("../escape")),
        Err(Error::InvalidAppId)
    ));
}
