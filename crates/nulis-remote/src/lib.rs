//! Capability-scoped remote resources with keychain-backed credentials.

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use nulis_large_file::ReadAt;
use serde::{Deserialize, Serialize};
use ssh2::{HashType, HostKeyType, MethodType, Session};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Seek, SeekFrom},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use thiserror::Error;

pub const MAX_REMOTE_PROFILES: usize = 64;
pub const MAX_REMOTE_ENTRIES: usize = 1_000;
pub const MAX_REMOTE_PATH_BYTES: usize = 4_096;
pub const MAX_REMOTE_PATH_COMPONENTS: usize = 128;
pub const MAX_CREDENTIAL_BYTES: usize = 64 * 1024;
const MAX_PROFILE_STORE_BYTES: u64 = 256 * 1024;
const KEYRING_SERVICE: &str = "com.nulis.editor.remote";

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RemoteProfileId(String);

impl RemoteProfileId {
    /// Creates a validated opaque profile identifier.
    ///
    /// # Errors
    /// Returns an error unless the value is a short ASCII identifier.
    pub fn parse(value: impl Into<String>) -> Result<Self, RemoteError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(RemoteError::InvalidProfileId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteProfile {
    pub id: RemoteProfileId,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub root_path: String,
    pub credential_id: String,
    /// Lowercase SHA-256 host-key fingerprint without separators.
    pub host_key_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteProviderDescriptor {
    pub scheme: String,
    pub can_list: bool,
    pub can_read: bool,
    pub can_write: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoteEntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub kind: RemoteEntryKind,
    #[serde(skip_serializing_if = "Option::is_none", with = "optional_decimal_u64")]
    pub byte_len: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDirectory {
    pub path: String,
    pub entries: Vec<RemoteEntry>,
    pub truncated: bool,
}

pub struct RemoteOpenedFile {
    pub uri: String,
    pub name: String,
    pub source: Arc<dyn ReadAt>,
}

impl std::fmt::Debug for RemoteOpenedFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteOpenedFile")
            .field("uri", &self.uri)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum RemoteError {
    #[error("remote profile ID must use 1-64 ASCII letters, digits, '-' or '_'")]
    InvalidProfileId,
    #[error("remote profile is invalid: {0}")]
    InvalidProfile(String),
    #[error("remote path is invalid")]
    InvalidPath,
    #[error("unknown remote profile {0}")]
    UnknownProfile(String),
    #[error("remote profile limit of {MAX_REMOTE_PROFILES} reached")]
    ProfileLimit,
    #[error("credential is invalid or exceeds {MAX_CREDENTIAL_BYTES} bytes")]
    InvalidCredential,
    #[error("remote state is unavailable")]
    StateUnavailable,
    #[error("the server's {key_type} host key has the fingerprint {presented}, not the one saved in this profile")]
    HostKeyMismatch {
        /// The key type, as `ssh-keygen -l` names it (`ED25519`, `ECDSA`, `RSA`).
        key_type: &'static str,
        /// `SHA256:` and the unpadded base64 digest, as OpenSSH prints it.
        presented: String,
    },
    #[error("remote authentication failed")]
    AuthenticationFailed,
    #[error("remote operation failed: {0}")]
    Operation(String),
}

pub trait CredentialStore: Send + Sync {
    /// Stores secret bytes.
    ///
    /// # Errors
    /// Returns an error for invalid identifiers, limits, or storage failure.
    fn set(&self, id: &str, secret: &[u8]) -> Result<(), RemoteError>;
    /// Retrieves secret bytes.
    ///
    /// # Errors
    /// Returns an error for invalid identifiers, missing secrets, or storage failure.
    fn get(&self, id: &str) -> Result<Vec<u8>, RemoteError>;
    /// Deletes secret bytes.
    ///
    /// # Errors
    /// Returns an error for invalid identifiers or storage failure.
    fn delete(&self, id: &str) -> Result<(), RemoteError>;
}

#[derive(Debug, Default)]
pub struct PlatformCredentialStore {
    access: Mutex<()>,
}

impl CredentialStore for PlatformCredentialStore {
    fn set(&self, id: &str, secret: &[u8]) -> Result<(), RemoteError> {
        validate_credential(id, secret)?;
        let _guard = self
            .access
            .lock()
            .map_err(|_| RemoteError::StateUnavailable)?;
        keyring::Entry::new(KEYRING_SERVICE, id)
            .and_then(|entry| entry.set_secret(secret))
            .map_err(|error| RemoteError::Operation(error.to_string()))
    }

    fn get(&self, id: &str) -> Result<Vec<u8>, RemoteError> {
        validate_credential_id(id)?;
        let _guard = self
            .access
            .lock()
            .map_err(|_| RemoteError::StateUnavailable)?;
        let secret = keyring::Entry::new(KEYRING_SERVICE, id)
            .and_then(|entry| entry.get_secret())
            .map_err(|error| RemoteError::Operation(error.to_string()))?;
        if secret.len() > MAX_CREDENTIAL_BYTES {
            return Err(RemoteError::InvalidCredential);
        }
        Ok(secret)
    }

    fn delete(&self, id: &str) -> Result<(), RemoteError> {
        validate_credential_id(id)?;
        let _guard = self
            .access
            .lock()
            .map_err(|_| RemoteError::StateUnavailable)?;
        keyring::Entry::new(KEYRING_SERVICE, id)
            .and_then(|entry| entry.delete_credential())
            .map_err(|error| RemoteError::Operation(error.to_string()))
    }
}

#[derive(Debug)]
pub struct RemoteProfileStore {
    path: PathBuf,
    profiles: Mutex<BTreeMap<RemoteProfileId, RemoteProfile>>,
}

impl RemoteProfileStore {
    /// Opens the metadata-only profile store.
    ///
    /// # Errors
    /// Returns an error for oversized, malformed, or invalid metadata.
    pub fn open(path: PathBuf) -> Result<Self, RemoteError> {
        let profiles = if path.exists() {
            let metadata = fs::metadata(&path).map_err(operation)?;
            if metadata.len() > MAX_PROFILE_STORE_BYTES {
                return Err(RemoteError::Operation(
                    "remote profile store is too large".into(),
                ));
            }
            let stored: Vec<RemoteProfile> =
                serde_json::from_slice(&fs::read(&path).map_err(operation)?)
                    .map_err(|error| RemoteError::Operation(error.to_string()))?;
            if stored.len() > MAX_REMOTE_PROFILES {
                return Err(RemoteError::ProfileLimit);
            }
            let mut profiles = BTreeMap::new();
            for profile in stored {
                validate_profile(&profile)?;
                profiles.insert(profile.id.clone(), profile);
            }
            profiles
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            path,
            profiles: Mutex::new(profiles),
        })
    }

    /// Lists all stored profile metadata.
    ///
    /// # Errors
    /// Returns an error if shared profile state is unavailable.
    pub fn list(&self) -> Result<Vec<RemoteProfile>, RemoteError> {
        Ok(self
            .profiles
            .lock()
            .map_err(|_| RemoteError::StateUnavailable)?
            .values()
            .cloned()
            .collect())
    }

    /// Saves validated profile metadata atomically.
    ///
    /// # Errors
    /// Returns an error for invalid metadata, limits, or persistence failure.
    pub fn save(&self, profile: RemoteProfile) -> Result<RemoteProfile, RemoteError> {
        validate_profile(&profile)?;
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| RemoteError::StateUnavailable)?;
        if !profiles.contains_key(&profile.id) && profiles.len() >= MAX_REMOTE_PROFILES {
            return Err(RemoteError::ProfileLimit);
        }
        profiles.insert(profile.id.clone(), profile.clone());
        self.persist(&profiles)?;
        Ok(profile)
    }

    /// Deletes profile metadata.
    ///
    /// # Errors
    /// Returns an error for an unknown profile or persistence failure.
    pub fn delete(&self, id: &RemoteProfileId) -> Result<(), RemoteError> {
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| RemoteError::StateUnavailable)?;
        if profiles.remove(id).is_none() {
            return Err(RemoteError::UnknownProfile(id.as_str().into()));
        }
        self.persist(&profiles)
    }

    fn get(&self, id: &RemoteProfileId) -> Result<RemoteProfile, RemoteError> {
        self.profiles
            .lock()
            .map_err(|_| RemoteError::StateUnavailable)?
            .get(id)
            .cloned()
            .ok_or_else(|| RemoteError::UnknownProfile(id.as_str().into()))
    }

    fn persist(
        &self,
        profiles: &BTreeMap<RemoteProfileId, RemoteProfile>,
    ) -> Result<(), RemoteError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| RemoteError::Operation("profile store has no parent".into()))?;
        fs::create_dir_all(parent).map_err(operation)?;
        let mut temporary = tempfile::Builder::new()
            .prefix("remote-profiles-")
            .tempfile_in(parent)
            .map_err(operation)?;
        serde_json::to_writer(&mut temporary, &profiles.values().collect::<Vec<_>>())
            .map_err(|error| RemoteError::Operation(error.to_string()))?;
        temporary.as_file().sync_all().map_err(operation)?;
        temporary
            .persist(&self.path)
            .map_err(|error| operation(error.error))?;
        Ok(())
    }
}

pub struct RemoteService {
    profiles: RemoteProfileStore,
    credentials: Arc<dyn CredentialStore>,
}

impl std::fmt::Debug for RemoteService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteService")
            .field("profiles", &self.profiles)
            .finish_non_exhaustive()
    }
}

impl RemoteService {
    /// Opens a service backed by the platform credential store.
    ///
    /// # Errors
    /// Returns an error if profile metadata cannot be loaded.
    pub fn open(profile_path: PathBuf) -> Result<Self, RemoteError> {
        Self::with_credentials(profile_path, Arc::new(PlatformCredentialStore::default()))
    }

    /// Opens a service with an injected credential capability.
    ///
    /// # Errors
    /// Returns an error if profile metadata cannot be loaded.
    pub fn with_credentials(
        profile_path: PathBuf,
        credentials: Arc<dyn CredentialStore>,
    ) -> Result<Self, RemoteError> {
        Ok(Self {
            profiles: RemoteProfileStore::open(profile_path)?,
            credentials,
        })
    }

    #[must_use]
    pub fn providers(&self) -> Vec<RemoteProviderDescriptor> {
        vec![RemoteProviderDescriptor {
            scheme: "sftp".into(),
            can_list: true,
            can_read: true,
            can_write: false,
        }]
    }

    /// Lists profile metadata.
    ///
    /// # Errors
    /// Returns an error if profile state is unavailable.
    pub fn list_profiles(&self) -> Result<Vec<RemoteProfile>, RemoteError> {
        self.profiles.list()
    }
    /// Saves profile metadata.
    ///
    /// # Errors
    /// Returns an error for invalid metadata, limits, or persistence failure.
    pub fn save_profile(&self, mut profile: RemoteProfile) -> Result<RemoteProfile, RemoteError> {
        profile.host_key_sha256 = normalize_host_key_fingerprint(&profile.host_key_sha256)?;
        self.profiles.save(profile)
    }
    /// Deletes profile metadata.
    ///
    /// # Errors
    /// Returns an error for an unknown profile or persistence failure.
    pub fn delete_profile(&self, id: &RemoteProfileId) -> Result<(), RemoteError> {
        self.profiles.delete(id)
    }
    /// Stores a secret using the credential capability.
    ///
    /// # Errors
    /// Returns an error for invalid input or credential-store failure.
    pub fn set_credential(&self, id: &str, secret: &[u8]) -> Result<(), RemoteError> {
        self.credentials.set(id, secret)
    }
    /// Deletes a secret using the credential capability.
    ///
    /// # Errors
    /// Returns an error for invalid input or credential-store failure.
    pub fn delete_credential(&self, id: &str) -> Result<(), RemoteError> {
        self.credentials.delete(id)
    }

    /// Lists one bounded SFTP directory after host-key verification.
    ///
    /// # Errors
    /// Returns an error for invalid paths, connection, verification, or authentication failure.
    pub fn list(
        &self,
        profile_id: &RemoteProfileId,
        path: &str,
    ) -> Result<RemoteDirectory, RemoteError> {
        let profile = self.profiles.get(profile_id)?;
        let normalized = normalize_remote_path(path)?;
        let session = self.connect(&profile)?;
        let sftp = session.sftp().map_err(operation)?;
        reject_symlink_components(&sftp, &profile.root_path, &normalized)?;
        let absolute = remote_absolute_path(&profile.root_path, &normalized);
        let mut entries = sftp.readdir(Path::new(&absolute)).map_err(operation)?;
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        let truncated = entries.len() > MAX_REMOTE_ENTRIES;
        entries.truncate(MAX_REMOTE_ENTRIES);
        let mut result = entries
            .into_iter()
            .filter_map(|(entry_path, stat)| {
                let name = entry_path.file_name()?.to_string_lossy().into_owned();
                if matches!(name.as_str(), "." | "..") {
                    return None;
                }
                let kind = if stat.is_dir() {
                    RemoteEntryKind::Directory
                } else if stat.is_file() {
                    RemoteEntryKind::File
                } else if stat.file_type().is_symlink() {
                    RemoteEntryKind::Symlink
                } else {
                    RemoteEntryKind::Other
                };
                let child = if normalized.is_empty() {
                    name.clone()
                } else {
                    format!("{normalized}/{name}")
                };
                Some(RemoteEntry {
                    name,
                    path: child,
                    kind,
                    byte_len: stat.size,
                })
            })
            .collect::<Vec<_>>();
        result.sort_by(|left, right| {
            entry_rank(left.kind)
                .cmp(&entry_rank(right.kind))
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(RemoteDirectory {
            path: normalized,
            entries: result,
            truncated,
        })
    }

    /// Opens a remote file as a random-access source.
    ///
    /// # Errors
    /// Returns an error for invalid paths, connection, verification, or authentication failure.
    pub fn open_file(
        &self,
        profile_id: &RemoteProfileId,
        path: &str,
    ) -> Result<RemoteOpenedFile, RemoteError> {
        let profile = self.profiles.get(profile_id)?;
        let normalized = normalize_remote_path(path)?;
        if normalized.is_empty() {
            return Err(RemoteError::InvalidPath);
        }
        let session = self.connect(&profile)?;
        let sftp = session.sftp().map_err(operation)?;
        reject_symlink_components(&sftp, &profile.root_path, &normalized)?;
        let absolute = remote_absolute_path(&profile.root_path, &normalized);
        let mut file = sftp.open(Path::new(&absolute)).map_err(operation)?;
        let stat = file.stat().map_err(operation)?;
        if !stat.is_file() {
            return Err(RemoteError::InvalidPath);
        }
        let name = Path::new(&normalized).file_name().map_or_else(
            || normalized.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
        Ok(RemoteOpenedFile {
            uri: format!("sftp://{}/{}", profile.id.as_str(), normalized),
            name,
            source: Arc::new(SftpSource {
                file: Mutex::new(file),
                len: stat.size.unwrap_or(0),
                _session: session,
            }),
        })
    }

    fn connect(&self, profile: &RemoteProfile) -> Result<Session, RemoteError> {
        let address = (profile.host.as_str(), profile.port)
            .to_socket_addrs()
            .map_err(operation)?
            .next()
            .ok_or_else(|| RemoteError::Operation("remote host did not resolve".into()))?;
        let tcp =
            TcpStream::connect_timeout(&address, Duration::from_secs(10)).map_err(operation)?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(operation)?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(operation)?;
        let mut session = Session::new().map_err(operation)?;
        // Ask for the host key OpenSSH uses first, so the fingerprint `ssh`
        // showed on first connect is the one checked. Methods this build of
        // libssh2 lacks (Ed25519 and ECDSA with WinCNG) are dropped by it.
        let _ = session.method_pref(MethodType::HostKey, HOST_KEY_PREFERENCE);
        session.set_tcp_stream(tcp);
        session.handshake().map_err(operation)?;
        let (_, key_type) = session
            .host_key()
            .ok_or_else(|| RemoteError::Operation("remote did not provide a host key".into()))?;
        let host_key = session
            .host_key_hash(HashType::Sha256)
            .ok_or_else(|| RemoteError::Operation("remote did not provide a host key".into()))?;
        if hex(host_key) != profile.host_key_sha256 {
            return Err(RemoteError::HostKeyMismatch {
                key_type: key_type_name(key_type),
                presented: format!("SHA256:{}", STANDARD_NO_PAD.encode(host_key)),
            });
        }
        let mut secret = self.credentials.get(&profile.credential_id)?;
        let authentication = std::str::from_utf8(&secret)
            .map_err(|_| RemoteError::InvalidCredential)
            .and_then(|password| {
                session
                    .userauth_password(&profile.username, password)
                    .map_err(|_| RemoteError::AuthenticationFailed)
            });
        secret.fill(0);
        authentication?;
        if !session.authenticated() {
            return Err(RemoteError::AuthenticationFailed);
        }
        Ok(session)
    }
}

struct SftpSource {
    file: Mutex<ssh2::File>,
    len: u64,
    _session: Session,
}

impl ReadAt for SftpSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        if offset >= self.len || buffer.is_empty() {
            return Ok(0);
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("remote source is unavailable"))?;
        file.seek(SeekFrom::Start(offset))?;
        file.read(buffer)
    }
}

/// Host key algorithms in OpenSSH's default order.
const HOST_KEY_PREFERENCE: &str = "ssh-ed25519,ecdsa-sha2-nistp256,ecdsa-sha2-nistp384,ecdsa-sha2-nistp521,rsa-sha2-512,rsa-sha2-256,ssh-rsa";

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}

fn key_type_name(key_type: HostKeyType) -> &'static str {
    match key_type {
        HostKeyType::Ed25519 => "ED25519",
        HostKeyType::Ecdsa256 | HostKeyType::Ecdsa384 | HostKeyType::Ecdsa521 => "ECDSA",
        HostKeyType::Rsa => "RSA",
        HostKeyType::Dss => "DSA",
        HostKeyType::Unknown => "unknown",
    }
}

/// Reads a SHA-256 host key fingerprint as OpenSSH prints it
/// (`SHA256:` and base64, padded or not) or as hex (colons allowed), and
/// returns it as 64 lowercase hex digits.
///
/// # Errors
/// Returns an error when the text is neither form of a 32-byte digest.
pub fn normalize_host_key_fingerprint(text: &str) -> Result<String, RemoteError> {
    let text = text.trim();
    let digest = text
        .get(..7)
        .filter(|prefix| prefix.eq_ignore_ascii_case("sha256:"))
        .map_or(text, |_| &text[7..]);
    let without_colons = digest.replace(':', "");
    if without_colons.len() == 64 && without_colons.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(without_colons.to_ascii_lowercase());
    }
    match STANDARD_NO_PAD.decode(digest.trim_end_matches('=')) {
        Ok(bytes) if bytes.len() == 32 => Ok(hex(&bytes)),
        _ => Err(RemoteError::InvalidProfile(
            "the host key fingerprint must be a SHA-256 fingerprint, such as SHA256:67C81q… from ssh-keygen -lf".into(),
        )),
    }
}

fn validate_profile(profile: &RemoteProfile) -> Result<(), RemoteError> {
    RemoteProfileId::parse(profile.id.as_str())?;
    if profile.label.is_empty()
        || profile.label.len() > 128
        || profile.host.is_empty()
        || profile.host.len() > 253
        || profile.host.chars().any(char::is_whitespace)
        || profile.port == 0
        || profile.username.is_empty()
        || profile.username.len() > 128
        || !profile.root_path.starts_with('/')
        || normalize_remote_path(profile.root_path.trim_start_matches('/')).is_err()
        || validate_credential_id(&profile.credential_id).is_err()
        || profile.host_key_sha256.len() != 64
        || !profile
            .host_key_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(RemoteError::InvalidProfile("invalid SFTP metadata".into()));
    }
    Ok(())
}

fn validate_credential(id: &str, secret: &[u8]) -> Result<(), RemoteError> {
    validate_credential_id(id)?;
    if secret.is_empty() || secret.len() > MAX_CREDENTIAL_BYTES {
        Err(RemoteError::InvalidCredential)
    } else {
        Ok(())
    }
}

fn validate_credential_id(id: &str) -> Result<(), RemoteError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Err(RemoteError::InvalidCredential)
    } else {
        Ok(())
    }
}

fn normalize_remote_path(path: &str) -> Result<String, RemoteError> {
    if path.len() > MAX_REMOTE_PATH_BYTES
        || path.contains('\0')
        || path.contains('\\')
        || path.starts_with('/')
    {
        return Err(RemoteError::InvalidPath);
    }
    let mut normalized = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => return Err(RemoteError::InvalidPath),
            value => normalized.push(value.to_owned()),
        }
    }
    if normalized.len() > MAX_REMOTE_PATH_COMPONENTS {
        return Err(RemoteError::InvalidPath);
    }
    Ok(normalized.join("/"))
}

fn reject_symlink_components(
    sftp: &ssh2::Sftp,
    root: &str,
    relative: &str,
) -> Result<(), RemoteError> {
    let mut current = String::new();
    for component in root
        .trim_matches('/')
        .split('/')
        .chain(relative.split('/'))
        .filter(|component| !component.is_empty())
    {
        current.push('/');
        current.push_str(component);
        let stat = sftp.lstat(Path::new(&current)).map_err(operation)?;
        if stat.file_type().is_symlink() {
            return Err(RemoteError::InvalidPath);
        }
    }
    Ok(())
}

fn remote_absolute_path(root: &str, relative: &str) -> String {
    let root = root.trim_end_matches('/');
    if relative.is_empty() {
        if root.is_empty() {
            "/".into()
        } else {
            root.into()
        }
    } else if root.is_empty() {
        format!("/{relative}")
    } else {
        format!("{root}/{relative}")
    }
}

const fn entry_rank(kind: RemoteEntryKind) -> u8 {
    match kind {
        RemoteEntryKind::Directory => 0,
        RemoteEntryKind::File => 1,
        RemoteEntryKind::Symlink => 2,
        RemoteEntryKind::Other => 3,
    }
}

fn operation(error: impl std::fmt::Display) -> RemoteError {
    RemoteError::Operation(error.to_string())
}
mod optional_decimal_u64 {
    use serde::Serializer;
    #[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer.serialize_some(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CredentialStore, RemoteError, RemoteProfile, RemoteProfileId, RemoteProfileStore,
        RemoteService,
    };
    use std::{
        collections::BTreeMap,
        fs,
        sync::{Arc, Mutex},
    };

    #[derive(Debug, Default)]
    struct MemoryCredentials(Mutex<BTreeMap<String, Vec<u8>>>);
    impl CredentialStore for MemoryCredentials {
        fn set(&self, id: &str, secret: &[u8]) -> Result<(), RemoteError> {
            self.0
                .lock()
                .expect("lock")
                .insert(id.into(), secret.into());
            Ok(())
        }
        fn get(&self, id: &str) -> Result<Vec<u8>, RemoteError> {
            self.0
                .lock()
                .expect("lock")
                .get(id)
                .cloned()
                .ok_or(RemoteError::InvalidCredential)
        }
        fn delete(&self, id: &str) -> Result<(), RemoteError> {
            self.0.lock().expect("lock").remove(id);
            Ok(())
        }
    }
    fn profile() -> RemoteProfile {
        RemoteProfile {
            id: RemoteProfileId::parse("production").expect("id"),
            label: "Production".into(),
            host: "example.test".into(),
            port: 22,
            username: "developer".into(),
            root_path: "/srv/app".into(),
            credential_id: "production-password".into(),
            host_key_sha256: "ab".repeat(32),
        }
    }

    #[test]
    fn profile_store_persists_metadata_without_credentials() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("profiles.json");
        let store = RemoteProfileStore::open(path.clone()).expect("store");
        store.save(profile()).expect("save");
        let json = fs::read_to_string(path).expect("json");
        assert!(json.contains("production-password"));
        assert!(!json.contains("secret-value"));
        assert_eq!(store.list().expect("list"), vec![profile()]);
    }

    #[test]
    fn credentials_remain_behind_the_injected_capability() {
        let directory = tempfile::tempdir().expect("tempdir");
        let credentials = Arc::new(MemoryCredentials::default());
        let service = RemoteService::with_credentials(
            directory.path().join("profiles.json"),
            credentials.clone(),
        )
        .expect("service");
        service
            .set_credential("profile.password", b"secret-value")
            .expect("set");
        assert_eq!(
            credentials.get("profile.password").expect("get"),
            b"secret-value"
        );
        assert_eq!(service.providers()[0].scheme, "sftp");
    }

    #[test]
    fn rejects_traversal_and_unpinned_profiles() {
        assert!(super::normalize_remote_path("../secret").is_err());
        assert!(super::normalize_remote_path("folder\\secret").is_err());
        assert_eq!(
            super::normalize_remote_path("folder/./nested//file").expect("normalize"),
            "folder/nested/file"
        );
        let mut profile = profile();
        profile.host_key_sha256.clear();
        assert!(super::validate_profile(&profile).is_err());
    }

    #[test]
    fn host_key_fingerprints_are_read_as_openssh_prints_them_or_as_hex() {
        // `ssh-keygen -lf` for the ED25519 key of a test server.
        let hex = "ebb0bcd6a2255860da575b0cda3289af58d16a0f56c24eb04d335ff038b4f96c";
        for text in [
            "SHA256:67C81qIlWGDaV1sM2jKJr1jRag9Wwk6wTTNf8Di0+Ww",
            " sha256:67C81qIlWGDaV1sM2jKJr1jRag9Wwk6wTTNf8Di0+Ww= ",
            "67C81qIlWGDaV1sM2jKJr1jRag9Wwk6wTTNf8Di0+Ww",
            "EBB0BCD6A2255860DA575B0CDA3289AF58D16A0F56C24EB04D335FF038B4F96C",
            "eb:b0:bc:d6:a2:25:58:60:da:57:5b:0c:da:32:89:af:58:d1:6a:0f:56:c2:4e:b0:4d:33:5f:f0:38:b4:f9:6c",
        ] {
            assert_eq!(super::normalize_host_key_fingerprint(text).expect(text), hex);
        }
        for text in [
            "",
            "SHA256:",
            "SHA256:67C81qIlWGDaV1sM2jKJr1jRag9Wwk6w",
            "MD5:ab:cd",
            "ab".repeat(31).as_str(),
        ] {
            assert!(
                super::normalize_host_key_fingerprint(text).is_err(),
                "{text}"
            );
        }
        // Lowercasing base64, as the form used to, changes the digest.
        assert_ne!(
            super::normalize_host_key_fingerprint(
                "sha256:67c81qilwgdav1sm2jkjr1jrag9wwk6wwtnf8di0+ww"
            )
            .ok(),
            Some(hex.to_owned())
        );
    }

    #[test]
    fn a_profile_saved_with_an_openssh_fingerprint_stores_hex() {
        let directory = tempfile::tempdir().expect("tempdir");
        let service = RemoteService::with_credentials(
            directory.path().join("profiles.json"),
            Arc::new(MemoryCredentials::default()),
        )
        .expect("service");
        let mut entered = profile();
        entered.host_key_sha256 = "SHA256:67C81qIlWGDaV1sM2jKJr1jRag9Wwk6wTTNf8Di0+Ww".into();
        let saved = service.save_profile(entered).expect("save");
        assert_eq!(
            saved.host_key_sha256,
            "ebb0bcd6a2255860da575b0cda3289af58d16a0f56c24eb04d335ff038b4f96c"
        );
        assert_eq!(service.list_profiles().expect("list"), vec![saved]);
    }
}
