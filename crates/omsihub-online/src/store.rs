//! Where the session token is kept: the OS's credential store (Windows' Credential Manager,
//! macOS's Keychain, Linux's kernel keyring - the `keyring` crate), never a file, a log or the
//! company's save. Where there is no such store (Android, iOS) the token lives in memory only
//! and the player signs in again at the next start.

use std::sync::Mutex;

/// The entry's names in the OS's store.
pub const SERVICE: &str = "Omsi-Hub online";
pub const ACCOUNT: &str = "session";

/// A place for the one secret.
pub trait SecretStore: Send {
    fn load(&self) -> Option<String>;
    /// Whether it was kept.
    fn save(&self, secret: &str) -> bool;
    fn clear(&self);
    /// Whether what it keeps outlives the program.
    fn lasting(&self) -> bool;
}

/// In memory only (the tests', and where the OS has no store).
#[derive(Default)]
pub struct MemoryStore(Mutex<Option<String>>);

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// One that holds `secret` already.
    pub fn with(secret: &str) -> Self {
        Self(Mutex::new(Some(secret.to_string())))
    }
}

impl SecretStore for MemoryStore {
    fn load(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn save(&self, secret: &str) -> bool {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(secret.to_string());
        true
    }
    fn clear(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    fn lasting(&self) -> bool {
        false
    }
}

/// The OS's credential store.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub struct OsStore;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl OsStore {
    fn entry() -> Option<keyring::Entry> {
        keyring::Entry::new(SERVICE, ACCOUNT).ok()
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl SecretStore for OsStore {
    fn load(&self) -> Option<String> {
        Self::entry()?.get_password().ok().filter(|t| !t.is_empty())
    }
    fn save(&self, secret: &str) -> bool {
        Self::entry().is_some_and(|e| e.set_password(secret).is_ok())
    }
    fn clear(&self) {
        if let Some(e) = Self::entry() {
            let _ = e.delete_credential();
        }
    }
    fn lasting(&self) -> bool {
        true
    }
}

/// The OS's store where there is one, else memory.
pub fn os_store() -> Box<dyn SecretStore> {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    return Box::new(OsStore);
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return Box::new(MemoryStore::new());
}
