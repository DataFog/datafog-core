//! Application-scoped encrypted snapshots, independent of FogVault's password owner.
//! All mutable semantics remain in Core; a transaction owns the process lock
//! from refresh through durable CAS and activation. No unsaved preview escapes.
use crate::core::{self, Head, PublicId, Workspace, WorkspaceFinding};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

const MAGIC: &[u8; 8] = b"DFAPP001";
const MAX_FILE: usize = core::MAX_BUNDLE_BYTES + 512;
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// Controlled codes never include paths, key-provider messages or source text.
#[derive(Debug)]
pub enum Error {
    InvalidAppId,
    UnsafePath,
    Permissions,
    Io,
    Integrity,
    UnsupportedEnvelope,
    CredentialUnavailable,
    KeyRequired,
    LockTimeout,
    Conflict,
    SavedNotActivated,
    OutcomeUnknown,
    Closed,
    Core(core::WorkspaceError),
}
impl Error {
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidAppId => "invalid_app_id",
            Self::UnsafePath => "unsafe_path",
            Self::Permissions => "unsafe_permissions",
            Self::Io => "storage_error",
            Self::Integrity => "vault_integrity",
            Self::UnsupportedEnvelope => "unsupported_envelope",
            Self::CredentialUnavailable => "credential_store_unavailable",
            Self::KeyRequired => "key_required",
            Self::LockTimeout => "vault_busy",
            Self::Conflict => "workspace_conflict",
            Self::SavedNotActivated => "saved_but_not_activated",
            Self::Core(e) => e.code.as_str(),
            Self::OutcomeUnknown => "outcome_unknown",
            Self::Closed => "owner_closed",
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for Error {}
impl From<core::WorkspaceError> for Error {
    fn from(e: core::WorkspaceError) -> Self {
        Self::Core(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    CredentialStore = 1,
    FileKey = 2,
    ExternalKey = 3,
}
impl Protection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CredentialStore => "credential_store",
            Self::FileKey => "file_key",
            Self::ExternalKey => "external_key",
        }
    }
}

pub struct Options {
    pub app_id: String,
    /// Explicit directory takes precedence over DATAFOG_APPLICATION_VAULT_DIR.
    pub directory: Option<PathBuf>,
    pub key: Option<Zeroizing<[u8; 32]>>,
    /// Explicit local file-key mode for NEW vaults, bypassing OS credentials.
    /// Also permits reopening an existing file-key vault; never downgrades one.
    pub allow_file_key: bool,
}
impl Options {
    pub fn new(app_id: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            directory: None,
            key: None,
            allow_file_key: false,
        }
    }
}

/// Receipt of local commit, not of remote delivery. Caller owns returned text.
pub struct Committed {
    pub head: Head,
    pub texts: Vec<String>,
}

pub struct ApplicationVault {
    app_id: String,
    directory: PathBuf,
    vault_id: PublicId,
    namespace: PublicId,
    protection: Protection,
    key: Zeroizing<[u8; 32]>,
    workspace: Workspace,
    active: bool,
}

/// Encrypted storage boundary. There is intentionally no unconditional save.
/// The owner authenticates loaded metadata before supplying the exact expected
/// envelope and Workspace head. A future backend must make this CAS atomic.
/// Backends never receive plaintext or encryption keys. No remote backend exists.
pub trait SnapshotBackend {
    fn load(&self) -> Result<Option<Vec<u8>>>;
    fn compare_and_save(
        &self,
        expected_head: Option<Head>,
        expected_envelope: Option<&[u8]>,
        candidate: &[u8],
    ) -> Result<()>;
}
struct LocalBackend<'a> {
    directory: &'a Path,
}
impl SnapshotBackend for LocalBackend<'_> {
    fn load(&self) -> Result<Option<Vec<u8>>> {
        let path = self.directory.join("workspace.enc");
        if path.exists() {
            Ok(Some(read_private(&path, MAX_FILE)?))
        } else {
            Ok(None)
        }
    }
    // ApplicationVault holds the directory lock across load/prepare/CAS. Exposing
    // this local backend would permit bypassing that lock, so it remains private.
    fn compare_and_save(
        &self,
        expected_head: Option<Head>,
        expected_envelope: Option<&[u8]>,
        candidate: &[u8],
    ) -> Result<()> {
        let actual = self.load()?;
        if actual.as_deref() != expected_envelope
            || actual.as_deref().map(envelope).transpose()?.map(|m| m.head) != expected_head
        {
            return Err(Error::Conflict);
        }
        envelope(candidate)?;
        atomic_write(
            self.directory,
            &self.directory.join("workspace.enc"),
            candidate,
        )
    }
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut value = [0; N];
    getrandom::fill(&mut value).map_err(|_| Error::Io)?;
    Ok(value)
}
fn valid_app(app: &str) -> bool {
    let stem = app
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    !app.is_empty()
        && app.len() <= 128
        && app.as_bytes()[0].is_ascii_alphanumeric()
        && app
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !app.contains("..")
        && !app.ends_with('.')
        && !reserved
}

pub fn resolve_directory(app: &str, explicit: Option<&Path>) -> Result<PathBuf> {
    if !valid_app(app) {
        return Err(Error::InvalidAppId);
    }
    let selected = explicit
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("DATAFOG_APPLICATION_VAULT_DIR").map(PathBuf::from));
    if let Some(path) = selected {
        if !path.is_absolute() {
            return Err(Error::UnsafePath);
        }
        return Ok(path);
    }
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(target_os = "linux")]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let base: Option<PathBuf> = None;
    let base = base.filter(|p| p.is_absolute()).ok_or(Error::UnsafePath)?;
    Ok(base.join("DataFog").join(app))
}

// Refuse traversal/symlink components before creating anything. Ancestors may
// be shared OS directories; the final directory and every owned file are private.
fn reject_links(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(Error::UnsafePath);
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        // A Windows drive/device prefix is not a filesystem path until its
        // RootDir is appended (canonical paths use e.g. \\?\C:). Validate every
        // actual directory entry, without querying that incomplete prefix.
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => return Err(Error::UnsafePath),
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(Error::Io),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn secure_new(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|_| Error::Permissions)
}
#[cfg(unix)]
fn check_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let m = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
    // SAFETY: geteuid has no pointers or preconditions.
    let uid = unsafe { libc::geteuid() };
    if m.file_type().is_symlink()
        || m.uid() != uid
        || m.mode() & 0o077 != 0
        || (!m.is_dir() && (!m.is_file() || m.nlink() != 1))
    {
        return Err(Error::Permissions);
    }
    Ok(())
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::{check_private, secure_new};

fn ensure_directory(path: &Path) -> Result<()> {
    reject_links(path)?;
    if !path.exists() {
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            ensure_directory(parent)?;
        }
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let mut builder = builder;
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(path) {
            Ok(()) => secure_new(path)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(_) => return Err(Error::Io),
        }
    }
    if !path.is_dir() {
        return Err(Error::UnsafePath);
    }
    check_private(path)?;
    #[cfg(unix)]
    sync_ancestor_chain(path)?;
    Ok(())
}

#[cfg(unix)]
fn sync_ancestor_chain(path: &Path) -> Result<()> {
    // A concurrent creator can expose ANY ancestor before syncing its parent.
    // Every opener flushes the complete chain, including already-visible dirs,
    // before any key/snapshot write or acknowledgment. No chmod of shared parents.
    for parent in path.ancestors().skip(1) {
        #[cfg(test)]
        tests::parent_sync_checkpoint(parent)?;
        File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|_| Error::Io)?;
    }
    Ok(())
}

// Closing the descriptor releases the OS lock, including unwinding/process exit.
struct Lock {
    _file: File,
}
fn lock_contended(error: &std::io::Error) -> bool {
    // Windows ERROR_LOCK_VIOLATION is not ErrorKind::WouldBlock. Use the
    // lock backend's platform code rather than treating contention as I/O loss.
    error.kind() == std::io::ErrorKind::WouldBlock
        || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
}
fn acquire(directory: &Path) -> Result<Lock> {
    reject_links(directory)?;
    check_private(directory)?;
    let path = directory.join("workspace.lock");
    reject_links(&path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    let existed = path.exists();
    let file = options.open(&path).map_err(|_| Error::Io)?;
    #[cfg(windows)]
    if !existed {
        secure_new(&path)?;
    }
    check_private(&path)?;
    let start = Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(Lock { _file: file }),
            Err(e) if lock_contended(&e) => {
                if start.elapsed() >= LOCK_TIMEOUT {
                    return Err(Error::LockTimeout);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return Err(Error::Io),
        }
    }
}

fn read_private(path: &Path, limit: usize) -> Result<Vec<u8>> {
    reject_links(path)?;
    check_private(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|_| Error::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata().map_err(|_| Error::Io)?;
        // SAFETY: geteuid has no pointers or preconditions.
        if m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o077 != 0
            || m.nlink() != 1
            || !m.is_file()
        {
            return Err(Error::Permissions);
        }
    }
    if file.metadata().map_err(|_| Error::Io)?.len() > limit as u64 {
        return Err(Error::Integrity);
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() > limit {
        return Err(Error::Integrity);
    }
    Ok(bytes)
}

fn atomic_write(directory: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    reject_links(path)?;
    if path.exists() {
        check_private(path)?;
    }
    let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(|_| Error::Io)?;
    secure_new(temp.path())?;
    temp.write_all(bytes).map_err(|_| Error::Io)?;
    temp.as_file().sync_all().map_err(|_| Error::Io)?;
    #[cfg(test)]
    crash_point("before_replace");
    #[cfg(unix)]
    temp.persist(path).map_err(|_| Error::OutcomeUnknown)?;
    #[cfg(windows)]
    windows::atomic_replace(temp.path(), path)?;
    #[cfg(test)]
    crash_point("after_replace");
    #[cfg(unix)]
    File::open(directory)
        .and_then(|d| d.sync_all())
        .map_err(|_| Error::OutcomeUnknown)?;
    Ok(())
}

struct Envelope<'a> {
    aad: &'a [u8],
    nonce: &'a [u8],
    ciphertext: &'a [u8],
    app: &'a str,
    vault_id: PublicId,
    namespace: PublicId,
    head: Head,
    protection: Protection,
}
fn envelope(bytes: &[u8]) -> Result<Envelope<'_>> {
    if bytes.len() < 8 + 4 + 1 + 16 + 16 + 8 + 16 + 1 + 24 + 16 || bytes.len() > MAX_FILE {
        return Err(Error::Integrity);
    }
    if &bytes[..8] != MAGIC || bytes[8..12] != core::WORKSPACE_CONTRACT_VERSION.to_le_bytes() {
        return Err(Error::UnsupportedEnvelope);
    }
    let length = bytes[12] as usize;
    let end = 13 + length;
    if bytes.len() < end + 81 + 16 {
        return Err(Error::Integrity);
    }
    let app = std::str::from_utf8(&bytes[13..end]).map_err(|_| Error::Integrity)?;
    let take16 = |start| {
        bytes[start..start + 16]
            .try_into()
            .map_err(|_| Error::Integrity)
    };
    let vault_id = take16(end)?;
    let namespace = take16(end + 16)?;
    let revision = u64::from_le_bytes(
        bytes[end + 32..end + 40]
            .try_into()
            .map_err(|_| Error::Integrity)?,
    );
    let head = Head {
        revision,
        commit_id: take16(end + 40)?,
    };
    let protection = match bytes[end + 56] {
        1 => Protection::CredentialStore,
        2 => Protection::FileKey,
        3 => Protection::ExternalKey,
        _ => return Err(Error::UnsupportedEnvelope),
    };
    Ok(Envelope {
        aad: &bytes[..end + 57],
        nonce: &bytes[end + 57..end + 81],
        ciphertext: &bytes[end + 81..],
        app,
        vault_id,
        namespace,
        head,
        protection,
    })
}

fn credential(app: &str, vault_id: PublicId) -> Result<keyring::Entry> {
    let hex: String = vault_id.iter().map(|b| format!("{b:02x}")).collect();
    keyring::Entry::new("DataFog.ApplicationVault.v1", &format!("{app}/{hex}"))
        .map_err(|_| Error::CredentialUnavailable)
}

impl ApplicationVault {
    pub fn open(mut options: Options) -> Result<Self> {
        let directory = resolve_directory(&options.app_id, options.directory.as_deref())?;
        ensure_directory(&directory)?;
        let _lock = acquire(&directory)?;
        let path = directory.join("workspace.enc");
        let existing = if path.exists() {
            Some(read_private(&path, MAX_FILE)?)
        } else {
            None
        };
        let meta = existing.as_deref().map(envelope).transpose()?;
        if meta.as_ref().is_some_and(|m| m.app != options.app_id) {
            return Err(Error::Integrity);
        }
        let vault_id = if let Some(m) = &meta {
            m.vault_id
        } else {
            random()?
        };
        let namespace = if let Some(m) = &meta {
            m.namespace
        } else {
            random()?
        };
        if meta
            .as_ref()
            .is_some_and(|m| m.protection == Protection::ExternalKey)
            && options.key.is_none()
        {
            return Err(Error::KeyRequired);
        }
        let (key, protection) = if let Some(key) = options.key.take() {
            (key, Protection::ExternalKey)
        } else if meta
            .as_ref()
            .is_some_and(|m| m.protection == Protection::FileKey)
        {
            if !options.allow_file_key {
                return Err(Error::KeyRequired);
            }
            let bytes = Zeroizing::new(read_private(&directory.join("workspace.key"), 32)?);
            (
                Zeroizing::new(bytes.as_slice().try_into().map_err(|_| Error::Integrity)?),
                Protection::FileKey,
            )
        } else if options.allow_file_key && meta.is_none() {
            let key = Zeroizing::new(random()?);
            let keypath = directory.join("workspace.key");
            if keypath.exists() {
                return Err(Error::Integrity);
            }
            atomic_write(&directory, &keypath, &*key)?;
            (key, Protection::FileKey)
        } else {
            let stored = credential(&options.app_id, vault_id).and_then(|entry| {
                if meta.is_some() {
                    let secret = Zeroizing::new(
                        entry
                            .get_secret()
                            .map_err(|_| Error::CredentialUnavailable)?,
                    );
                    Ok(Zeroizing::new(
                        secret.as_slice().try_into().map_err(|_| Error::Integrity)?,
                    ))
                } else {
                    let key = Zeroizing::new(random()?);
                    entry
                        .set_secret(&*key)
                        .map_err(|_| Error::CredentialUnavailable)?;
                    Ok(key)
                }
            });
            (stored?, Protection::CredentialStore)
        };
        if meta.as_ref().is_some_and(|m| m.protection != protection) {
            return Err(Error::KeyRequired);
        }
        let workspace = if let Some(m) = meta {
            Self::decode(&options.app_id, vault_id, namespace, protection, &key, m)?
        } else {
            Workspace::create(namespace, random()?)?
        };
        let owner = Self {
            app_id: options.app_id,
            directory,
            vault_id,
            namespace,
            protection,
            key,
            workspace,
            active: true,
        };
        if existing.is_none() {
            let bundle = Zeroizing::new(owner.workspace.export_bundle()?);
            owner.persist(None, owner.workspace.head()?, &bundle)?;
        }
        Ok(owner)
    }
    fn decode(
        app: &str,
        vault: PublicId,
        namespace: PublicId,
        protection: Protection,
        key: &[u8; 32],
        m: Envelope<'_>,
    ) -> Result<Workspace> {
        if m.app != app
            || m.vault_id != vault
            || m.namespace != namespace
            || m.protection != protection
        {
            return Err(Error::Integrity);
        }
        let cipher = XChaCha20Poly1305::new(key.into());
        let plain = Zeroizing::new(
            cipher
                .decrypt(
                    <&XNonce>::try_from(m.nonce).map_err(|_| Error::Integrity)?,
                    Payload {
                        msg: m.ciphertext,
                        aad: m.aad,
                    },
                )
                .map_err(|_| Error::Integrity)?,
        );
        let workspace = Workspace::open_bundle(&plain)?;
        if workspace.head()? != m.head || workspace.namespace()? != namespace {
            return Err(Error::Integrity);
        }
        Ok(workspace)
    }
    fn load(&self) -> Result<Workspace> {
        let bytes = read_private(&self.directory.join("workspace.enc"), MAX_FILE)?;
        Self::decode(
            &self.app_id,
            self.vault_id,
            self.namespace,
            self.protection,
            &self.key,
            envelope(&bytes)?,
        )
    }
    /// CAS is deliberately internal: caller-supplied snapshots cannot bypass Core.
    fn persist(&self, expected: Option<Head>, saved: Head, bundle: &[u8]) -> Result<()> {
        let backend = LocalBackend {
            directory: &self.directory,
        };
        let prior = backend.load()?;
        if let (Some(expected), Some(prior)) = (expected, prior.as_deref()) {
            let actual = Self::decode(
                &self.app_id,
                self.vault_id,
                self.namespace,
                self.protection,
                &self.key,
                envelope(prior)?,
            )?;
            if actual.head()? != expected {
                return Err(Error::Conflict);
            }
        } else if expected.is_some() || prior.is_some() {
            return Err(Error::Conflict);
        }
        let mut aad = Vec::with_capacity(256);
        aad.extend_from_slice(MAGIC);
        aad.extend_from_slice(&core::WORKSPACE_CONTRACT_VERSION.to_le_bytes());
        aad.push(self.app_id.len() as u8);
        aad.extend_from_slice(self.app_id.as_bytes());
        aad.extend_from_slice(&self.vault_id);
        aad.extend_from_slice(&self.namespace);
        aad.extend_from_slice(&saved.revision.to_le_bytes());
        aad.extend_from_slice(&saved.commit_id);
        aad.push(self.protection as u8);
        let nonce: [u8; 24] = random()?;
        let cipher = XChaCha20Poly1305::new((&*self.key).into());
        let encrypted = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: bundle,
                    aad: &aad,
                },
            )
            .map_err(|_| Error::Integrity)?;
        aad.extend_from_slice(&nonce);
        aad.extend_from_slice(&encrypted);
        backend.compare_and_save(expected, prior.as_deref(), &aad)
    }
    pub fn protection_mode(&self) -> Protection {
        self.protection
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn vault_id(&self) -> PublicId {
        self.vault_id
    }
    pub fn head(&self) -> Result<Head> {
        if !self.active {
            return Err(Error::Closed);
        }
        Ok(self.workspace.head()?)
    }
    pub fn refresh(&mut self) -> Result<Head> {
        self.head()?;
        let _lock = acquire(&self.directory)?;
        self.workspace = self.load()?;
        self.head()
    }
    fn transact(
        &mut self,
        prepare: impl Fn(&Workspace) -> std::result::Result<core::PreparedChange, core::WorkspaceError>,
    ) -> Result<Committed> {
        self.head()?;
        let _lock = acquire(&self.directory)?;
        // Three total attempts with the same operation ID. Only a known CAS
        // conflict retries; errors after possible replacement never do.
        for attempt in 0..3 {
            self.workspace = self.load()?;
            let mut plan = prepare(&self.workspace)?;
            let expected = plan.expected_head()?;
            let saved = plan.candidate_head()?;
            let mut texts = Zeroizing::new(
                plan.previews()?
                    .iter()
                    .map(|p| p.text.clone())
                    .collect::<Vec<_>>(),
            );
            match self.persist(Some(expected), saved, plan.export_candidate_bundle()?) {
                Err(Error::Conflict) if attempt < 2 => continue,
                Err(Error::OutcomeUnknown) => {
                    self.close();
                    return Err(Error::OutcomeUnknown);
                }
                result => result?,
            }
            #[cfg(test)]
            crash_point("before_activate");
            if self
                .workspace
                .activate_saved(&mut plan, expected, saved)
                .is_err()
            {
                self.close();
                return Err(Error::SavedNotActivated);
            }
            return Ok(Committed {
                head: saved,
                texts: std::mem::take(&mut *texts),
            });
        }
        Err(Error::Conflict)
    }
    /// Findings must describe the exact approved input; this helper never sends.
    pub fn prepare_and_commit(
        &mut self,
        text: &str,
        findings: &[WorkspaceFinding],
        operation: PublicId,
    ) -> Result<Committed> {
        let occurred_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| Error::Io)?
            .as_millis()
            .try_into()
            .map_err(|_| Error::Io)?;
        self.prepare_and_commit_with_operation(
            text,
            findings,
            core::OperationContext {
                id: operation,
                occurred_at_ms,
            },
        )
    }
    pub fn prepare_and_commit_with_operation(
        &mut self,
        text: &str,
        findings: &[WorkspaceFinding],
        operation: core::OperationContext,
    ) -> Result<Committed> {
        self.transact(|w| w.prepare_with_operation(text, findings, operation))
    }
    pub fn forget(&mut self, codes: &[String], operation: PublicId) -> Result<Head> {
        Ok(self.transact(|w| w.prepare_forget(codes, operation))?.head)
    }
    pub fn restore(&mut self, text: &str) -> Result<String> {
        self.refresh()?;
        Ok(self.workspace.restore(text)?)
    }
    pub fn restore_report(&mut self, text: &str) -> Result<core::RestoreReport> {
        self.refresh()?;
        Ok(self.workspace.restore_report(text)?)
    }
    pub fn operations(&mut self) -> Result<Vec<core::OperationRecord>> {
        self.refresh()?;
        Ok(self.workspace.operations()?.to_vec())
    }
    pub fn close(&mut self) {
        self.active = false;
        self.workspace.close();
        self.key.zeroize();
    }
}

#[cfg(test)]
fn crash_point(point: &str) {
    if std::env::var("DATAFOG_APPLICATION_TEST_CRASH").as_deref() == Ok(point) {
        std::process::exit(73);
    }
}

#[cfg(test)]
mod tests;
