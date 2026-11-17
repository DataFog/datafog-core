//! Opt-in qualification against the real native credential provider.
//!
//! Every source value is synthetic. Only a unique qualification account is
//! modified/deleted; this executable never locks or reconfigures the OS store.
use datafog_fogvault_native::{
    application::{ApplicationVault, Error as VaultError, Options, Protection},
    core,
};
use keyring::credential::{Credential, CredentialBuilder, CredentialBuilderApi};
use serde_json::{Value, json};
use std::{
    any::Any,
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const SERVICE: &str = "DataFog.ApplicationVault.v1";
const PREFIX: &str = "datafog-credential-qualification-";
const STATE: &str = "qualification-state.json";
const SOURCE: &str = "credential-probe-synthetic@example.test";
const SCHEMA: &str = "datafog-real-credential-qualification/v1";

#[cfg(target_os = "macos")]
mod macos_control {
    use super::*;
    use std::{
        ffi::{CString, c_char, c_void},
        ptr,
    };
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecKeychainSetUserInteractionAllowed(allowed: u8) -> i32;
        fn SecKeychainOpen(path: *const c_char, keychain: *mut *mut c_void) -> i32;
        fn SecKeychainGetStatus(keychain: *mut c_void, status: *mut u32) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(value: *const c_void);
    }
    pub fn noninteractive() -> Result<()> {
        // This changes only this executable's prompt policy.
        if unsafe { SecKeychainSetUserInteractionAllowed(0) } != 0 {
            return Err(fail("interaction_policy", "cannot_disable_prompts"));
        }
        Ok(())
    }
    pub fn assert_locked(path: &Path) -> Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| fail("keychain_status", "invalid_path"))?;
        let mut keychain = ptr::null_mut();
        if unsafe { SecKeychainOpen(path.as_ptr(), &mut keychain) } != 0 || keychain.is_null() {
            return Err(fail("keychain_status", "keychain_open_failed"));
        }
        let mut status = 0;
        let result = unsafe { SecKeychainGetStatus(keychain, &mut status) };
        unsafe {
            CFRelease(keychain);
        }
        if result != 0 || status & 1 != 0 {
            return Err(fail("keychain_status", "keychain_not_locked"));
        }
        Ok(())
    }
}
type Result<T> = std::result::Result<T, Failure>;
#[derive(Debug)]
struct Failure {
    phase: &'static str,
    code: String,
}
fn fail(phase: &'static str, code: &str) -> Failure {
    Failure {
        phase,
        code: code.into(),
    }
}
fn app_error(phase: &'static str, error: VaultError) -> Failure {
    fail(phase, error.code())
}
fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| fail("random", "random_unavailable"))?;
    Ok(bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn backend() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos-keychain"
    }
    #[cfg(target_os = "windows")]
    {
        "windows-credential-manager"
    }
    #[cfg(target_os = "linux")]
    {
        "linux-secret-service"
    }
}
fn concrete_native(credential: &dyn Any) -> bool {
    #[cfg(target_os = "macos")]
    {
        credential.is::<keyring::macos::MacCredential>()
    }
    #[cfg(target_os = "windows")]
    {
        credential.is::<keyring::windows::WinCredential>()
    }
    #[cfg(target_os = "linux")]
    {
        credential.is::<keyring::secret_service::SsCredential>()
    }
}
fn options(app: &str, directory: &Path) -> Options {
    let mut options = Options::new(app);
    options.directory = Some(directory.to_owned());
    options
}
fn open(app: &str, directory: &Path) -> Result<ApplicationVault> {
    let owner =
        ApplicationVault::open(options(app, directory)).map_err(|e| app_error("open", e))?;
    if owner.protection_mode() != Protection::CredentialStore {
        return Err(fail("open", "unexpected_protection_mode"));
    }
    Ok(owner)
}
fn private_json(path: &Path, value: &Value, replace: bool) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| fail("evidence", "invalid_path"))?;
    fs::create_dir_all(parent).map_err(|_| fail("evidence", "directory_io"))?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| fail("evidence", "invalid_json"))?;
    if replace {
        let mut file =
            tempfile::NamedTempFile::new_in(parent).map_err(|_| fail("evidence", "create_io"))?;
        file.write_all(&bytes)
            .map_err(|_| fail("evidence", "write_io"))?;
        file.as_file()
            .sync_all()
            .map_err(|_| fail("evidence", "sync_io"))?;
        file.persist(path)
            .map_err(|_| fail("evidence", "replace_io"))?;
    } else {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(path)
            .map_err(|_| fail("evidence", "create_io"))?;
        file.write_all(&bytes)
            .map_err(|_| fail("evidence", "write_io"))?;
        file.sync_all().map_err(|_| fail("evidence", "sync_io"))?;
    }
    Ok(())
}
fn read_json(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).map_err(|_| fail("fixture", "read_io"))?;
    if bytes.len() > 256 * 1024 {
        return Err(fail("fixture", "oversized_metadata"));
    }
    serde_json::from_slice(&bytes).map_err(|_| fail("fixture", "invalid_metadata"))
}
fn valid_account(account: &str) -> bool {
    account.split_once('/').is_some_and(|(app, id)| {
        app.strip_prefix(PREFIX).is_some_and(|suffix| {
            suffix.len() == 32 && suffix.bytes().all(|b| b.is_ascii_hexdigit())
        }) && id.len() == 32
            && id.bytes().all(|b| b.is_ascii_hexdigit())
    })
}

// A transparent recorder writes only public account IDs before native creation.
// It forwards unchanged arguments and returns the real native credential object.
struct Recorder {
    inner: Box<CredentialBuilder>,
    accounts: Arc<Mutex<BTreeSet<String>>>,
    journal: PathBuf,
    directory: PathBuf,
}
impl CredentialBuilderApi for Recorder {
    fn build(
        &self,
        target: Option<&str>,
        service: &str,
        user: &str,
    ) -> keyring::Result<Box<Credential>> {
        if service != SERVICE || !valid_account(user) {
            return Err(keyring::Error::NoEntry);
        }
        {
            let mut accounts = self.accounts.lock().map_err(|_| keyring::Error::NoEntry)?;
            accounts.insert(user.into());
            private_json(&self.journal, &json!({"schema":"datafog-credential-cleanup/v1","service":SERVICE,"directory":self.directory,"accounts":&*accounts}), true)
                .map_err(|_| keyring::Error::NoEntry)?;
        }
        let credential = self.inner.build(target, service, user)?;
        if !concrete_native(credential.as_any()) {
            return Err(keyring::Error::NoEntry);
        }
        Ok(credential)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn persistence(&self) -> keyring::credential::CredentialPersistence {
        self.inner.persistence()
    }
}
fn install_recorder(
    output: &Path,
    directory: &Path,
) -> Result<(PathBuf, Arc<Mutex<BTreeSet<String>>>)> {
    let journal = PathBuf::from(format!("{}.cleanup.json", output.display()));
    private_json(
        &journal,
        &json!({"schema":"datafog-credential-cleanup/v1","service":SERVICE,"directory":directory,"accounts":[]}),
        false,
    )?;
    let accounts = Arc::new(Mutex::new(BTreeSet::new()));
    keyring::set_default_credential_builder(Box::new(Recorder {
        inner: keyring::default::default_credential_builder(),
        accounts: Arc::clone(&accounts),
        journal: journal.clone(),
        directory: directory.to_owned(),
    }));
    Ok((journal, accounts))
}
fn entry(account: &str) -> Result<keyring::Entry> {
    if !valid_account(account) {
        return Err(fail("credential", "foreign_account"));
    }
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|_| fail("credential", "provider_unavailable"))?;
    if !concrete_native(entry.get_credential()) {
        return Err(fail("credential", "non_native_provider"));
    }
    Ok(entry)
}
fn delete_owned(account: &str) -> Result<()> {
    let entry = entry(account)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => (),
        Err(_) => return Err(fail("cleanup", "credential_delete_failed")),
    }
    match entry.get_secret() {
        Err(keyring::Error::NoEntry) => Ok(()),
        _ => Err(fail("cleanup", "credential_delete_unconfirmed")),
    }
}
fn cleanup_journal(journal: &Path) -> Result<()> {
    let value = read_json(journal)?;
    if value["schema"] != "datafog-credential-cleanup/v1" || value["service"] != SERVICE {
        return Err(fail("cleanup", "invalid_journal"));
    }
    let accounts = value["accounts"]
        .as_array()
        .ok_or_else(|| fail("cleanup", "invalid_accounts"))?;
    for account in accounts {
        delete_owned(
            account
                .as_str()
                .ok_or_else(|| fail("cleanup", "invalid_account"))?,
        )?;
    }
    Ok(())
}
fn state(directory: &Path) -> Result<Value> {
    let state = read_json(&directory.join(STATE))?;
    if state["schema"] != "datafog-credential-fixture/v1"
        || !valid_account(account(&state)?)
        || account(&state)?.split_once('/').map(|(id, _)| id) != Some(app(&state)?)
    {
        return Err(fail("fixture", "invalid_fixture"));
    }
    Ok(state)
}
fn app(state: &Value) -> Result<&str> {
    state["app_id"]
        .as_str()
        .ok_or_else(|| fail("fixture", "invalid_app"))
}
fn account(state: &Value) -> Result<&str> {
    state["account"]
        .as_str()
        .ok_or_else(|| fail("fixture", "invalid_account"))
}
fn marker(state: &Value) -> Result<&str> {
    state["marker"]
        .as_str()
        .ok_or_else(|| fail("fixture", "invalid_marker"))
}
fn prepare(directory: &Path) -> Result<Value> {
    if directory.exists() {
        return Err(fail("prepare", "fixture_already_exists"));
    }
    let app = format!("{PREFIX}{}", hex(&random::<16>()?));
    let mut owner = open(&app, directory)?;
    let account = format!("{app}/{}", hex(&owner.vault_id()));
    let direct = entry(&account)?;
    let secret = Zeroizing::new(
        direct
            .get_secret()
            .map_err(|_| fail("prepare", "native_read_failed"))?,
    );
    if secret.len() != 32 {
        return Err(fail("prepare", "invalid_native_key_length"));
    }
    let finding = core::WorkspaceFinding::new(
        "EMAIL",
        SOURCE,
        datafog_core::TextRange {
            start: 0,
            end: SOURCE.len(),
        },
    );
    let committed = owner
        .prepare_and_commit(SOURCE, &[finding], random()?)
        .map_err(|e| app_error("prepare", e))?;
    let marker = committed
        .texts
        .first()
        .ok_or_else(|| fail("prepare", "missing_output"))?;
    if marker.contains(SOURCE) || !marker.starts_with("[[EMAIL_") {
        return Err(fail("prepare", "unprotected_output"));
    }
    let state = json!({"schema":"datafog-credential-fixture/v1","app_id":app,"account":account,"marker":marker});
    private_json(&directory.join(STATE), &state, false)?;
    owner.close();
    Ok(state)
}
fn verify(directory: &Path) -> Result<Value> {
    let state = state(directory)?;
    let start = Instant::now();
    let mut owner = open(app(&state)?, directory)?;
    if owner
        .restore(marker(&state)?)
        .map_err(|e| app_error("restore", e))?
        != SOURCE
    {
        return Err(fail("restore", "wrong_restored_value"));
    }
    if directory.join("workspace.key").exists() {
        return Err(fail("restore", "file_key_created"));
    }
    let millis = start.elapsed().as_secs_f64() * 1000.;
    owner.close();
    Ok(json!({"open_restore_ms":millis}))
}
fn expect_unavailable(directory: &Path) -> Result<()> {
    let state = state(directory)?;
    let before = fs::read(directory.join("workspace.enc"))
        .map_err(|_| fail("unavailable", "snapshot_read"))?;
    let mut options = options(app(&state)?, directory);
    options.allow_file_key = true;
    match ApplicationVault::open(options) {
        Err(VaultError::CredentialUnavailable) => (),
        Err(_) => return Err(fail("unavailable", "wrong_error")),
        Ok(mut owner) => {
            owner.close();
            return Err(fail("unavailable", "unexpected_open"));
        }
    }
    unchanged(directory, &before)
}
fn unchanged(directory: &Path, before: &[u8]) -> Result<()> {
    let after = fs::read(directory.join("workspace.enc"))
        .map_err(|_| fail("failure_check", "snapshot_read"))?;
    if after != before || directory.join("workspace.key").exists() {
        return Err(fail("failure_check", "vault_changed_or_downgraded"));
    }
    Ok(())
}
fn child(directory: &Path, mode: &str) -> Result<Value> {
    let result = Command::new(std::env::current_exe().map_err(|_| fail("child", "executable_io"))?)
        .arg(mode)
        .arg(directory)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| fail("child", "spawn_io"))?;
    if !result.status.success() {
        return Err(fail("child", "fresh_process_failed"));
    }
    serde_json::from_slice(&result.stdout).map_err(|_| fail("child", "invalid_child_report"))
}
fn writer(directory: &Path, index: usize) -> Result<Value> {
    let state = state(directory)?;
    let mut owner = open(app(&state)?, directory)?;
    private_flag(&directory.join(format!("qualification-ready-{index}")))?;
    let started = Instant::now();
    while !directory.join("qualification-start").exists() {
        if started.elapsed() > Duration::from_secs(30) {
            return Err(fail("writer_barrier", "start_timeout"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut markers = Vec::new();
    for iteration in 0..5 {
        let source = format!("synthetic-provider-{index}-{iteration}@example.test");
        let finding = core::WorkspaceFinding::new(
            "EMAIL",
            &source,
            datafog_core::TextRange {
                start: 0,
                end: source.len(),
            },
        );
        let saved = owner
            .prepare_and_commit(&source, &[finding], random()?)
            .map_err(|e| app_error("writer", e))?;
        markers.push(json!({"iteration":iteration,"marker":saved.texts.first().ok_or_else(|| fail("writer", "missing_output"))?}));
    }
    owner.close();
    Ok(json!({"worker":index,"markers":markers}))
}
fn private_flag(path: &Path) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|_| fail("writer_barrier", "flag_io"))?;
    Ok(())
}
fn summary(mut values: Vec<f64>) -> Value {
    values.sort_by(f64::total_cmp);
    json!({"samples":values.len(),"p50_ms":values[(values.len()*50).div_ceil(100)-1],"p95_ms":values[(values.len()*95).div_ceil(100)-1]})
}
fn case(cases: &mut Vec<Value>, id: &str) {
    cases.push(json!({"id":id,"status":"PASS"}));
}
struct Children(Vec<Child>);
impl Drop for Children {
    fn drop(&mut self) {
        // On any early error the other writers must stop before account cleanup.
        for child in &mut self.0 {
            if matches!(child.try_wait(), Ok(None)) && child.kill().is_err() {
                eprintln!("qualification child termination failed");
            }
            if child.wait().is_err() {
                eprintln!("qualification child reap failed");
            }
        }
    }
}
fn full(directory: &Path, cases: &mut Vec<Value>, measurements: &mut Value) -> Result<()> {
    let state = prepare(directory)?;
    keyring::set_default_credential_builder(keyring::default::default_credential_builder());
    case(cases, "real_provider_selected");
    case(cases, "credential_store_create");
    child(directory, "--child-verify")?;
    case(cases, "fresh_process_restore");
    let mut children = Children(Vec::new());
    for index in 0..4 {
        children.0.push(
            Command::new(std::env::current_exe().map_err(|_| fail("writers", "executable_io"))?)
                .arg("--child-writer")
                .arg(directory)
                .arg(index.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|_| fail("writers", "spawn_io"))?,
        );
    }
    let started = Instant::now();
    while (0..4).any(|index| {
        !directory
            .join(format!("qualification-ready-{index}"))
            .exists()
    }) {
        for child in &mut children.0 {
            if child
                .try_wait()
                .map_err(|_| fail("writer_barrier", "status_io"))?
                .is_some()
            {
                return Err(fail("writer_barrier", "writer_exited_before_start"));
            }
        }
        if started.elapsed() > Duration::from_secs(30) {
            return Err(fail("writer_barrier", "readiness_timeout"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // Every fresh process now holds a key obtained from the real native store.
    private_flag(&directory.join("qualification-start"))?;
    let mut reader = open(app(&state)?, directory)?;
    for (index, child) in children.0.iter_mut().enumerate() {
        let status = child.wait().map_err(|_| fail("writers", "wait_io"))?;
        if !status.success() {
            return Err(fail("writers", "child_failed"));
        }
        let mut stdout = Vec::new();
        child
            .stdout
            .take()
            .ok_or_else(|| fail("writers", "missing_stdout"))?
            .read_to_end(&mut stdout)
            .map_err(|_| fail("writers", "stdout_io"))?;
        let result: Value =
            serde_json::from_slice(&stdout).map_err(|_| fail("writers", "invalid_report"))?;
        let markers = result["markers"]
            .as_array()
            .ok_or_else(|| fail("writers", "missing_markers"))?;
        if markers.len() != 5 {
            return Err(fail("writers", "incomplete_markers"));
        }
        for (iteration, value) in markers.iter().enumerate() {
            let expected = format!("synthetic-provider-{index}-{iteration}@example.test");
            let restored = reader
                .restore(
                    value["marker"]
                        .as_str()
                        .ok_or_else(|| fail("writers", "invalid_marker"))?,
                )
                .map_err(|e| app_error("writers", e))?;
            if restored != expected {
                return Err(fail("writers", "wrong_restore"));
            }
        }
    }
    drop(children);
    reader.close();
    case(cases, "concurrent_fresh_process_writers");
    let native = entry(account(&state)?)?;
    let mut reads = Vec::new();
    let mut restores = Vec::new();
    for _ in 0..10 {
        let start = Instant::now();
        let key = Zeroizing::new(
            native
                .get_secret()
                .map_err(|_| fail("latency", "native_read_failed"))?,
        );
        if key.len() != 32 {
            return Err(fail("latency", "invalid_key_length"));
        }
        reads.push(start.elapsed().as_secs_f64() * 1000.);
        restores.push(
            verify(directory)?["open_restore_ms"]
                .as_f64()
                .ok_or_else(|| fail("latency", "invalid_timing"))?,
        );
    }
    *measurements = json!({"real_credential_get_secret":summary(reads),"open_restore_including_credential_lookup":summary(restores),"executable_startup_included":false,"synthetic_entries":21});
    case(cases, "real_secret_lookup_latency");
    case(cases, "real_open_restore_latency");
    let old_key = Zeroizing::new(
        native
            .get_secret()
            .map_err(|_| fail("negative", "native_read_failed"))?,
    );
    let wrong_key = Zeroizing::new(random::<32>()?);
    let before =
        fs::read(directory.join("workspace.enc")).map_err(|_| fail("negative", "snapshot_read"))?;
    native
        .set_secret(&*wrong_key)
        .map_err(|_| fail("negative", "native_write_failed"))?;
    match ApplicationVault::open(options(app(&state)?, directory)) {
        Err(VaultError::Integrity) => (),
        _ => return Err(fail("negative", "wrong_key_not_rejected")),
    }
    unchanged(directory, &before)?;
    case(cases, "wrong_stored_key_fails_closed");
    native
        .set_secret(&old_key)
        .map_err(|_| fail("negative", "native_restore_failed"))?;
    delete_owned(account(&state)?)?;
    expect_unavailable(directory)?;
    case(cases, "missing_stored_key_fails_closed");
    native
        .set_secret(&old_key)
        .map_err(|_| fail("negative", "native_restore_failed"))?;
    child(directory, "--child-verify")?;
    case(cases, "recovered_key_restores");
    Ok(())
}

fn run() -> Result<bool> {
    let noninteractive =
        std::env::var("DATAFOG_CREDENTIAL_PROBE_NONINTERACTIVE").as_deref() == Ok("1");
    #[cfg(target_os = "macos")]
    if noninteractive {
        macos_control::noninteractive()?;
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if let [mode, directory] = args.as_slice()
        && mode == "--child-verify"
    {
        println!("{}", verify(Path::new(directory))?);
        return Ok(true);
    }
    if let [mode, directory, index] = args.as_slice()
        && mode == "--child-writer"
    {
        let index = index
            .parse()
            .map_err(|_| fail("arguments", "invalid_worker"))?;
        println!("{}", writer(Path::new(directory), index)?);
        return Ok(true);
    }
    let mut mode = "full";
    let mut directory = None;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| fail("arguments", "missing_value"))?;
        match args[i].as_str() {
            "--output" => output = Some(PathBuf::from(value)),
            "--prepare"
            | "--verify"
            | "--expect-unavailable"
            | "--cleanup"
            | "--cleanup-journal"
            | "--assert-locked-keychain"
                if mode == "full" =>
            {
                mode = &args[i][2..];
                directory = Some(PathBuf::from(value));
            }
            _ => return Err(fail("arguments", "invalid_arguments")),
        }
        i += 2;
    }
    let output = output.ok_or_else(|| fail("arguments", "output_required"))?;
    let output = if output.is_absolute() {
        output
    } else {
        std::env::current_dir()
            .map_err(|_| fail("arguments", "current_directory_io"))?
            .join(output)
    };
    if output.exists() {
        return Err(fail("arguments", "output_exists"));
    }
    let temp = if mode == "full" {
        Some(
            tempfile::Builder::new()
                .prefix("datafog-real-credentials-")
                .tempdir()
                .map_err(|_| fail("fixture", "temporary_directory_io"))?,
        )
    } else {
        None
    };
    let directory = if let Some(temp) = &temp {
        temp.path()
            .canonicalize()
            .map_err(|_| fail("fixture", "canonical_path"))?
            .join("vault")
    } else {
        directory.ok_or_else(|| fail("arguments", "directory_required"))?
    };
    let mut cases = Vec::new();
    let mut measurements = Value::Null;
    let mut cleanup_status = "NOT_REQUESTED";
    let journal = if mode == "full" || mode == "prepare" {
        Some(install_recorder(&output, &directory)?.0)
    } else {
        None
    };
    let result = (|| -> Result<()> {
        match mode {
            "full" => full(&directory, &mut cases, &mut measurements),
            "prepare" => {
                prepare(&directory)?;
                keyring::set_default_credential_builder(
                    keyring::default::default_credential_builder(),
                );
                case(&mut cases, "real_provider_selected");
                case(&mut cases, "credential_store_create");
                Ok(())
            }
            "verify" => {
                measurements = verify(&directory)?;
                case(&mut cases, "fresh_process_restore");
                Ok(())
            }
            "expect-unavailable" => {
                expect_unavailable(&directory)?;
                case(&mut cases, "existing_store_unavailable_fails_closed");
                Ok(())
            }
            "cleanup" => {
                let state = state(&directory)?;
                delete_owned(account(&state)?)?;
                case(&mut cases, "owned_credentials_deleted");
                Ok(())
            }
            "cleanup-journal" => {
                cleanup_journal(&directory)?;
                case(&mut cases, "owned_credentials_deleted");
                Ok(())
            }
            "assert-locked-keychain" => {
                #[cfg(target_os = "macos")]
                {
                    macos_control::assert_locked(&directory)?;
                    case(&mut cases, "isolated_keychain_confirmed_locked");
                    Ok(())
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Err(fail("keychain_status", "unsupported_platform"))
                }
            }
            _ => Err(fail("arguments", "unknown_mode")),
        }
    })();
    // Creation may have failed after the recorder journal was written. Cleanup
    // must use an uninstrumented real provider even in that failure path.
    if journal.is_some() {
        keyring::set_default_credential_builder(keyring::default::default_credential_builder());
    }
    let cleanup = if mode == "full" || (mode == "prepare" && result.is_err()) {
        cleanup_journal(
            journal
                .as_deref()
                .ok_or_else(|| fail("cleanup", "missing_journal"))?,
        )
    } else {
        Ok(())
    };
    if mode == "full" || (mode == "prepare" && result.is_err()) || mode.starts_with("cleanup") {
        cleanup_status = if cleanup.is_ok()
            && (mode != "cleanup" && mode != "cleanup-journal" || result.is_ok())
        {
            "PASS"
        } else {
            "FAIL"
        };
    }
    if mode == "full" && cleanup.is_ok() {
        case(&mut cases, "owned_credentials_deleted");
    }
    let error = result.err().or_else(|| cleanup.err());
    let report = json!({"schema":SCHEMA,"mode":mode,"status":if error.is_none(){"PASS"}else{"FAIL"},"real_provider":true,"backend":backend(),"os":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "default_provider_instrumentation":if journal.is_some(){"initial_create_native_forwarder_recording_public_ids_only"}else{"none"},"timing_provider_override":false,"fresh_process_provider_override":false,"interaction_policy":if noninteractive{"noninteractive"}else{"native_default"},"mock_provider":false,"cases":cases,"measurements":measurements,"cleanup_status":cleanup_status,"cleanup_journal":journal,
        "error":error.as_ref().map(|e|json!({"phase":e.phase,"code":e.code})),"not_executed":["physical_reboot","global_os_store_lock_or_access_denial"],"provider_diagnostics_recorded":false,"secret_material_recorded":false});
    private_json(&output, &report, false)?;
    println!(
        "{}",
        json!({"schema":SCHEMA,"status":report["status"],"backend":backend(),"mode":mode})
    );
    Ok(error.is_none())
}
fn main() {
    match run() {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!(
                "{}",
                json!({"schema":SCHEMA,"status":"FAIL","error":{"phase":error.phase,"code":error.code}})
            );
            std::process::exit(1);
        }
    }
}
