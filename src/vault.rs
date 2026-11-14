//! Profile-local encryption: OS-protected master key, authenticated streaming files.
//! Original user documents, IPC endpoints, settings and updater packages stay separate.
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{
    aead, hkdf, pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"ETSEC153\x01";
const CHUNK: usize = 65536;
const ITERATIONS: u32 = 600_000;
static ROOT: OnceLock<PathBuf> = OnceLock::new();
static KEY: Mutex<Option<Zeroizing<[u8; 32]>>> = Mutex::new(None);
static MODE: Mutex<&'static str> = Mutex::new("Locked");
fn error(message: impl ToString) -> io::Error {
    io::Error::other(message.to_string())
}
fn random<const N: usize>() -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| error("Secure random source unavailable"))?;
    Ok(bytes)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    version: u8,
    id: String,
    mode: String,
    salt: String,
    nonce: String,
    wrapped: String,
    check: String,
    #[serde(default)]
    migration_complete: bool,
}
fn small(
    key: &[u8],
    nonce: [u8; 12],
    aad: &[u8],
    data: &[u8],
    opening: bool,
) -> io::Result<Zeroizing<Vec<u8>>> {
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, key)
            .map_err(|_| error("Invalid encryption key"))?,
    );
    let mut bytes = Zeroizing::new(data.to_vec());
    if opening {
        let length = key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(aad),
                &mut bytes,
            )
            .map_err(|_| error("Unlock failed: wrong password or damaged data"))?
            .len();
        bytes.truncate(length);
    } else {
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut *bytes,
        )
        .map_err(|_| error("Encryption failed"))?;
    }
    Ok(bytes)
}
fn decode(text: &str) -> io::Result<Vec<u8>> {
    STANDARD
        .decode(text)
        .map_err(|_| error("Invalid protection header"))
}
fn password_key(password: &str, salt: &[u8]) -> io::Result<Zeroizing<[u8; 32]>> {
    if salt.len() != 16 {
        return Err(error("Invalid password salt"));
    }
    let mut key = Zeroizing::new([0; 32]);
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(ITERATIONS).unwrap(),
        salt,
        password.as_bytes(),
        &mut *key,
    );
    Ok(key)
}
fn wrap_password(header: &mut Header, master: &[u8]) -> io::Result<()> {
    let password =
        crate::vault_ui::password(true).ok_or_else(|| error("Password setup cancelled"))?;
    let salt = random::<16>()?;
    let nonce = random::<12>()?;
    let key = password_key(&password, &salt)?;
    header.mode = "password".into();
    header.salt = STANDARD.encode(salt);
    header.nonce = STANDARD.encode(nonce);
    header.wrapped = STANDARD.encode(&*small(&*key, nonce, header.id.as_bytes(), master, false)?);
    Ok(())
}
fn unwrap_password(header: &Header) -> io::Result<Zeroizing<Vec<u8>>> {
    let password = crate::vault_ui::password(false).ok_or_else(|| error("Unlock cancelled"))?;
    let key = password_key(&password, &decode(&header.salt)?)?;
    let nonce: [u8; 12] = decode(&header.nonce)?
        .try_into()
        .map_err(|_| error("Invalid password nonce"))?;
    small(
        &*key,
        nonce,
        header.id.as_bytes(),
        &decode(&header.wrapped)?,
        true,
    )
}
#[cfg(windows)]
fn dpapi(data: &[u8], opening: bool) -> io::Result<Zeroizing<Vec<u8>>> {
    use std::{ffi::c_void, ptr};
    #[repr(C)]
    struct Blob {
        length: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(pointer: *mut c_void) -> *mut c_void;
    }
    let input = Blob {
        length: data.len().try_into().map_err(|_| error("Key too large"))?,
        data: data.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        length: 0,
        data: ptr::null_mut(),
    };
    let ok = unsafe {
        if opening {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = unsafe {
        Zeroizing::new(std::slice::from_raw_parts(output.data, output.length as usize).to_vec())
    };
    unsafe {
        use zeroize::Zeroize;
        std::slice::from_raw_parts_mut(output.data, output.length as usize).zeroize();
        LocalFree(output.data as _);
    }
    Ok(result)
}
#[cfg(not(windows))]
fn key_entry(id: &str) -> io::Result<keyring::Entry> {
    keyring::Entry::new("com.github.OnlySecci.EditorTool", id).map_err(error)
}
fn protect_os(header: &mut Header, master: &[u8]) -> io::Result<()> {
    #[cfg(windows)]
    {
        header.mode = "dpapi".into();
        header.wrapped = STANDARD.encode(&*dpapi(master, false)?);
    }
    #[cfg(not(windows))]
    {
        header.mode = "keychain".into();
        key_entry(&header.id)?.set_secret(master).map_err(error)?;
    }
    let restored = load_master(header)?;
    if restored.as_slice() != master {
        return Err(error("Key storage verification failed"));
    }
    Ok(())
}
fn load_master(header: &Header) -> io::Result<Zeroizing<Vec<u8>>> {
    match header.mode.as_str() {
        "password" => unwrap_password(header),
        #[cfg(windows)]
        "dpapi" => dpapi(&decode(&header.wrapped)?, true),
        #[cfg(not(windows))]
        "keychain" => key_entry(&header.id)?
            .get_secret()
            .map(Zeroizing::new)
            .map_err(error),
        _ => Err(error(
            "This key store is unavailable on this computer; restore an encrypted recovery key",
        )),
    }
}
fn read_header(path: &Path) -> io::Result<Header> {
    let mut bytes = Vec::new();
    File::open(path)?.take(16385).read_to_end(&mut bytes)?;
    if bytes.len() > 16384 {
        return Err(error("Protection header too large"));
    }
    let header: Header = serde_json::from_slice(&bytes).map_err(error)?;
    if header.version != 1
        || header.id.len() != 64
        || !header.id.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(error("Invalid protection header"));
    }
    Ok(header)
}
fn check(header: &Header, master: &[u8]) -> io::Result<()> {
    if master.len() != 32 {
        return Err(error("Invalid master key"));
    }
    let value = decode(&header.check)?;
    if value.len() < 28 {
        return Err(error("Invalid key verification data"));
    }
    let nonce: [u8; 12] = value[..12].try_into().unwrap();
    if small(master, nonce, header.id.as_bytes(), &value[12..], true)?.as_slice()
        != if header.migration_complete {
            b"EditorTool local data: ready".as_slice()
        } else {
            b"EditorTool local data: migrating".as_slice()
        }
    {
        return Err(error("Key verification failed"));
    }
    Ok(())
}
fn portable(root: &Path) -> bool {
    root.file_name().is_some_and(|n| n == "Data")
        && root
            .parent()
            .is_some_and(|p| p.join("portable.txt").exists())
}
fn encrypted_example(root: &Path) -> io::Result<Option<PathBuf>> {
    let mut paths = vec![
        root.join("session"),
        root.join("history"),
        root.join("sharing"),
    ];
    let mut count = 0;
    while let Some(path) = paths.pop() {
        if !path.exists() {
            continue;
        }
        count += 1;
        if count > 100_000 {
            return Err(error("Too many files while locating protected data"));
        }
        let metadata = fs::symlink_metadata(&path)?;
        if crate::private_fs::is_link(&metadata) {
            return Err(error("Link in protected storage rejected"));
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                paths.push(entry?.path());
            }
            continue;
        }
        if !sensitive(&path) || path.extension().is_some_and(|e| e == "pending") {
            continue;
        }
        let mut magic = vec![0; MAGIC.len()];
        let n = File::open(&path)?.read(&mut magic)?;
        if n == MAGIC.len() && magic == MAGIC {
            return Ok(Some(path));
        }
    }
    Ok(None)
}
fn recover_profile(root: &Path, old: Option<&Header>) -> io::Result<(Header, Zeroizing<Vec<u8>>)> {
    let action=crate::chrome::decision(None,"Protected data is locked","The protection key or its header is unavailable. Existing documents and device identities are preserved.","Restore an encrypted recovery key, or continue with protected data locked. No replacement master key will be generated.",&["Continue with data locked","Restore recovery key…"],0);
    if action != Some(1) {
        return Err(error("Protected data remains locked"));
    }
    let mut chooser =
        crate::file_dialog::Chooser::new(fltk::dialog::NativeFileChooserType::BrowseFile);
    chooser.set_title("Restore encrypted recovery key");
    chooser.show();
    let mut header = read_header(&chooser.filename())?;
    if header.mode != "password" {
        return Err(error("Choose a password-encrypted recovery file"));
    }
    let master = unwrap_password(&header)?;
    check(&header, &master)?;
    if let Some(path) = encrypted_example(root)? {
        let mut key = Zeroizing::new([0; 32]);
        key.copy_from_slice(&master);
        *KEY.lock().unwrap() = Some(key);
        let result = (|| {
            let mut reader = Reader::new(&path, &path, crate::limits::DOCUMENT)?;
            io::copy(&mut reader, &mut io::sink())?;
            Ok::<(), io::Error>(())
        })();
        *KEY.lock().unwrap() = None;
        result?;
    } else if old.is_some_and(|h| h.id != header.id) {
        return Err(error("Recovery key belongs to a different profile"));
    }
    if !portable(root) {
        let original = header.clone();
        if protect_os(&mut header, &master).is_err() {
            header = original;
        }
    }
    crate::session::atomic_write(
        &root.join("security/vault.json"),
        &serde_json::to_vec(&header).map_err(error)?,
    )?;
    crate::diagnostics::event("security", "profile_key_recovered");
    Ok((header, master))
}

pub fn initialize(root: &Path) -> io::Result<()> {
    ROOT.set(root.to_path_buf())
        .map_err(|_| error("Protection already initialized"))?;
    let directory = root.join("security");
    crate::private_fs::directory(&directory)?;
    let path = directory.join("vault.json");
    let (mut header, master) = if path.exists() {
        match read_header(&path) {
            Ok(header) => {
                let mut unlocked = None;
                for attempt in 0..3 {
                    match load_master(&header).and_then(|key| {
                        check(&header, &key)?;
                        Ok(key)
                    }) {
                        Ok(key) => {
                            unlocked = Some(key);
                            break;
                        }
                        Err(_) => {
                            if attempt == 2
                                || crate::chrome::decision(
                                    None,
                                    "Unlock failed",
                                    "The key store is locked or the password is incorrect.",
                                    "Existing protected data is preserved.",
                                    &["Continue with data locked", "Retry unlocking"],
                                    0,
                                ) != Some(1)
                            {
                                break;
                            }
                        }
                    }
                }
                if let Some(key) = unlocked {
                    (header, key)
                } else {
                    recover_profile(root, Some(&header))?
                }
            }
            Err(_) => recover_profile(root, None)?,
        }
    } else if encrypted_example(root)?.is_some() || root.join("sharing/identity.v153.json").exists()
    {
        recover_profile(root, None)?
    } else {
        let master = Zeroizing::new(random::<32>()?.to_vec());
        let mut header = Header {
            version: 1,
            id: crate::private_fs::random_token()?,
            mode: String::new(),
            salt: String::new(),
            nonce: String::new(),
            wrapped: String::new(),
            check: String::new(),
            migration_complete: false,
        };
        let portable = portable(root);
        if portable || protect_os(&mut header, &master).is_err() {
            let action=crate::chrome::decision(None,"Protect local data",if portable {"Portable profiles use a protection password."}else{"The operating-system key store is unavailable. Use a password to protect local data."},"Cancelling leaves existing data unchanged and locked. Send/Sync and session saving will remain unavailable until protection is configured.",&["Continue with data locked","Set protection password…"],1);
            if action != Some(1) {
                return Err(error("Protection setup cancelled"));
            }
            wrap_password(&mut header, &master)?;
        }
        let nonce = random::<12>()?;
        let mut check_bytes = nonce.to_vec();
        check_bytes.extend_from_slice(&small(
            &master,
            nonce,
            header.id.as_bytes(),
            b"EditorTool local data: migrating",
            false,
        )?);
        header.check = STANDARD.encode(check_bytes);
        if let Err(error) =
            crate::session::atomic_write(&path, &serde_json::to_vec(&header).map_err(error)?)
        {
            *KEY.lock().unwrap() = None;
            *MODE.lock().unwrap() = "Locked: migration incomplete";
            return Err(error);
        }
        // Confirm durable storage before touching any legacy plaintext.
        let stored = read_header(&path)?;
        check(&stored, &master)?;
        if serde_json::to_vec(&stored).map_err(error)?
            != serde_json::to_vec(&header).map_err(error)?
        {
            return Err(error(
                "Durable key header verification failed; existing data preserved",
            ));
        }
        (header, master)
    };
    let mut key = Zeroizing::new([0; 32]);
    key.copy_from_slice(&master);
    *KEY.lock().unwrap() = Some(key);
    *MODE.lock().unwrap() = match header.mode.as_str() {
        "dpapi" => "Windows DPAPI",
        "keychain" => {
            if cfg!(target_os = "macos") {
                "macOS Keychain"
            } else {
                "Linux Secret Service"
            }
        }
        _ => "Password protected",
    };
    if !header.migration_complete {
        if let Err(error) = migrate(root) {
            *KEY.lock().unwrap() = None;
            *MODE.lock().unwrap() = "Locked: migration incomplete";
            return Err(error);
        }
        header.migration_complete = true;
        let nonce = random::<12>()?;
        let mut check_bytes = nonce.to_vec();
        check_bytes.extend_from_slice(&small(
            &master,
            nonce,
            header.id.as_bytes(),
            b"EditorTool local data: ready",
            false,
        )?);
        header.check = STANDARD.encode(check_bytes);
        if let Err(error) =
            crate::session::atomic_write(&path, &serde_json::to_vec(&header).map_err(error)?)
        {
            *KEY.lock().unwrap() = None;
            *MODE.lock().unwrap() = "Locked: migration incomplete";
            return Err(error);
        }
        crate::diagnostics::event("security", "local_data_migration_completed");
    }
    Ok(())
}
pub fn status() -> String {
    format!(
        "{} · AES-256-GCM authenticated local encryption",
        *MODE.lock().unwrap()
    )
}
pub fn unlocked() -> bool {
    KEY.lock().unwrap().is_some()
}
pub fn sensitive(path: &Path) -> bool {
    let Some(root) = ROOT.get() else {
        return false;
    };
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut components = relative.components();
    let first = components.next().map(|c| c.as_os_str().to_string_lossy());
    matches!(first.as_deref(), Some("session" | "history" | "sharing"))
        && relative != Path::new("session/session.lock")
        && relative != Path::new("session/instance.json")
}
fn file_key(path: &Path, salt: &[u8]) -> io::Result<aead::LessSafeKey> {
    let key = KEY.lock().unwrap();
    let key = key
        .as_ref()
        .ok_or_else(|| error("Protected data is locked"))?;
    let relative = path
        .strip_prefix(ROOT.get().unwrap())
        .map_err(error)?
        .to_string_lossy()
        .replace('\\', "/");
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, salt);
    let prk = salt.extract(&**key);
    let info = [b"EditorTool local data v1".as_slice(), relative.as_bytes()];
    let okm = prk
        .expand(&info, hkdf::HKDF_SHA256)
        .map_err(|_| error("Key derivation failed"))?;
    let mut derived = Zeroizing::new([0; 32]);
    okm.fill(&mut *derived)
        .map_err(|_| error("Key derivation failed"))?;
    Ok(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, &*derived)
            .map_err(|_| error("Invalid encryption key"))?,
    ))
}
pub struct Writer {
    file: File,
    key: Option<aead::LessSafeKey>,
    buffer: Zeroizing<Vec<u8>>,
    index: u32,
}
impl Writer {
    pub fn new(mut file: File, context: &Path) -> io::Result<Self> {
        let key = if sensitive(context) {
            let salt = random::<32>()?;
            let key = file_key(context, &salt)?;
            file.write_all(MAGIC)?;
            file.write_all(&salt)?;
            Some(key)
        } else {
            None
        };
        Ok(Self {
            file,
            key,
            buffer: Zeroizing::new(Vec::with_capacity(CHUNK)),
            index: 0,
        })
    }
    fn chunk(&mut self, final_chunk: bool) -> io::Result<()> {
        let Some(key) = &self.key else {
            return Ok(());
        };
        if self.buffer.is_empty() && !final_chunk {
            return Ok(());
        }
        let length = self.buffer.len() as u32;
        let mut nonce = [0; 12];
        nonce[8..].copy_from_slice(&self.index.to_be_bytes());
        let mut aad = [0; 8];
        aad[..4].copy_from_slice(&self.index.to_be_bytes());
        aad[4..].copy_from_slice(&length.to_be_bytes());
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut *self.buffer,
        )
        .map_err(|_| error("Encryption failed"))?;
        self.file.write_all(&length.to_be_bytes())?;
        self.file.write_all(&self.buffer)?;
        self.buffer.clear();
        self.index = self
            .index
            .checked_add(1)
            .ok_or_else(|| error("Encrypted file is too large"))?;
        Ok(())
    }
    pub fn finish(mut self) -> io::Result<()> {
        self.chunk(false)?;
        self.chunk(true)?;
        self.file.sync_all()
    }
}
impl Write for Writer {
    fn write(&mut self, mut bytes: &[u8]) -> io::Result<usize> {
        if self.key.is_none() {
            return self.file.write(bytes);
        }
        let length = bytes.len();
        while !bytes.is_empty() {
            let count = bytes.len().min(CHUNK - self.buffer.len());
            self.buffer.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            if self.buffer.len() == CHUNK {
                self.chunk(false)?;
            }
        }
        Ok(length)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.chunk(false)?;
        self.file.flush()
    }
}
pub fn write(file: File, path: &Path, data: &[u8]) -> io::Result<()> {
    let mut writer = Writer::new(file, path)?;
    writer.write_all(data)?;
    writer.finish()
}
/// Decrypt one bounded chunk at a time, including an authenticated end marker.
pub struct Reader {
    file: File,
    key: Option<aead::LessSafeKey>,
    buffer: Zeroizing<Vec<u8>>,
    position: usize,
    index: u32,
    total: u64,
    maximum: u64,
    finished: bool,
}
impl Reader {
    pub fn new(path: &Path, context: &Path, max: u64) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let overhead = (max / CHUNK as u64 + 2) * 20 + 64;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > max.saturating_add(overhead) {
            return Err(error("File exceeds its size limit"));
        }
        let key = if sensitive(context) {
            let mut header = vec![0; MAGIC.len() + 32];
            file.read_exact(&mut header)?;
            if !header.starts_with(MAGIC) {
                return Err(error(
                    "Unmigrated protected data; restart to complete migration",
                ));
            }
            Some(file_key(context, &header[MAGIC.len()..])?)
        } else {
            if metadata.len() > max {
                return Err(error("File exceeds its size limit"));
            }
            None
        };
        Ok(Self {
            file,
            key,
            buffer: Zeroizing::new(Vec::new()),
            position: 0,
            index: 0,
            total: 0,
            maximum: max,
            finished: false,
        })
    }
    fn next(&mut self) -> io::Result<()> {
        let key = self.key.as_ref().unwrap();
        let mut size = [0; 4];
        self.file.read_exact(&mut size)?;
        let size = u32::from_be_bytes(size) as usize;
        if size > CHUNK || self.total.saturating_add(size as u64) > self.maximum {
            return Err(error("Encrypted data exceeds its size limit"));
        }
        self.buffer.resize(size + 16, 0);
        self.file.read_exact(&mut self.buffer)?;
        let mut nonce = [0; 12];
        nonce[8..].copy_from_slice(&self.index.to_be_bytes());
        let mut aad = [0; 8];
        aad[..4].copy_from_slice(&self.index.to_be_bytes());
        aad[4..].copy_from_slice(&(size as u32).to_be_bytes());
        key.open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut self.buffer,
        )
        .map_err(|_| error("Protected data authentication failed"))?;
        self.buffer.truncate(size);
        self.position = 0;
        if size == 0 {
            let mut trailing = [0];
            if self.file.read(&mut trailing)? != 0 {
                return Err(error("Unexpected trailing encrypted data"));
            }
            self.finished = true;
        }
        self.total += size as u64;
        self.index = self
            .index
            .checked_add(1)
            .ok_or_else(|| error("Too many encrypted chunks"))?;
        Ok(())
    }
}
impl Read for Reader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.finished {
            return Ok(0);
        }
        if self.key.is_none() {
            let remaining = self.maximum.saturating_sub(self.total);
            let count = output.len().min(remaining.saturating_add(1) as usize);
            let n = self.file.read(&mut output[..count])?;
            self.total += n as u64;
            if self.total > self.maximum {
                return Err(error("File exceeds its size limit"));
            }
            return Ok(n);
        }
        if self.position == self.buffer.len() {
            self.next()?;
        }
        if self.finished {
            return Ok(0);
        }
        let count = output.len().min(self.buffer.len() - self.position);
        output[..count].copy_from_slice(&self.buffer[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}
pub fn read(path: &Path, context: &Path, max: u64) -> io::Result<Vec<u8>> {
    let mut reader = Reader::new(path, context, max)?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn migrate(root: &Path) -> io::Result<()> {
    let mut pending = vec![
        root.join("session"),
        root.join("history"),
        root.join("sharing"),
    ];
    let mut count = 0;
    while let Some(path) = pending.pop() {
        if !path.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if crate::private_fs::is_link(&metadata) {
            return Err(error("Link in protected storage rejected"));
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
            continue;
        }
        if !sensitive(&path) {
            continue;
        }
        count += 1;
        if count > 100_000 {
            return Err(error("Too many files for safe protection migration"));
        }
        let mut file = File::open(&path)?;
        let mut magic = vec![0; MAGIC.len()];
        let n = file.read(&mut magic)?;
        if n == MAGIC.len() && magic == MAGIC {
            continue;
        }
        drop(file);
        if metadata.len() > crate::limits::DOCUMENT {
            return Err(error(
                "Legacy file exceeds migration limit; existing data preserved",
            ));
        }
        let bytes = Zeroizing::new(fs::read(&path)?);
        crate::session::atomic_write(&path, &bytes)?;
    }
    Ok(())
}
pub fn export_recovery() -> io::Result<()> {
    let root = ROOT.get().ok_or_else(|| error("Protection unavailable"))?;
    let mut header = read_header(&root.join("security/vault.json"))?;
    let master = KEY
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .ok_or_else(|| error("Protected data is locked"))?;
    wrap_password(&mut header, &*master)?;
    let mut chooser =
        crate::file_dialog::Chooser::new(fltk::dialog::NativeFileChooserType::BrowseSaveFile);
    chooser.set_title("Export password-encrypted recovery key");
    chooser.set_preset_file("EditorTool-Recovery.json");
    chooser.set_option(fltk::dialog::NativeFileChooserOptions::SaveAsConfirm);
    chooser.show();
    let path = chooser.filename();
    if !path.as_os_str().is_empty() {
        crate::storage::write_document(&path, &serde_json::to_vec_pretty(&header).map_err(error)?)?;
    }
    Ok(())
}
