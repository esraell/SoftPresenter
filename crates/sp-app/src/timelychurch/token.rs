//! Where the device token is kept: the system's secure store (the Mac's Keychain; Windows'
//! Credential Manager and Linux's Secret Service once `crate::secrets` lands), under the church
//! it was made for. It is never written to a settings file, the log, a command's reply or a
//! song pack. A test copy of the app (started with `SOFTPRESENTER_DATA`) keeps its own token in
//! its own folder instead, so it never reads, changes or asks for the owner's.
//!
//! Each call may wait for the system (macOS may first ask whether the app may have the token),
//! so they are made only on threads of their own.

/// The secure store's entry the token is kept under: the app's, for TimelyChurch.
const SERVICE: &str = "SoftPresenter TimelyChurch";

/// A test copy's own token file, in its own folder; `None` for the owner's app.
fn test_file() -> Option<std::path::PathBuf> {
    std::env::var_os("SOFTPRESENTER_DATA")
        .filter(|dir| !dir.is_empty())
        .map(|dir| std::path::PathBuf::from(dir).join(super::store::FOLDER).join("test-token"))
}

/// The account a church's token is kept under: its address as a key holds it, so two churches
/// on one computer never share one.
fn account(address: &str) -> String {
    format!("device token for {}", super::church_part(address))
}

/// The token kept for the church at this address, if there is one.
pub fn load(address: &str) -> Option<String> {
    if let Some(file) = test_file() {
        return std::fs::read_to_string(file).ok().map(|token| token.trim().to_owned()).filter(|t| !t.is_empty());
    }
    secrets::get(SERVICE, &account(address)).ok().flatten().filter(|token| !token.is_empty())
}

/// Keeps the token for the church at this address. The error is a sentence.
pub fn save(address: &str, token: &str) -> Result<(), String> {
    let not_kept = |error: String| {
        crate::i18n::tf(
            "integrations.timelychurch.not_kept",
            "The connection could not be kept on this computer: {error}",
            &[("error", &error)],
        )
    };
    if let Some(file) = test_file() {
        if let Some(folder) = file.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        std::fs::write(&file, token).map_err(|e| not_kept(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
        }
        return Ok(());
    }
    secrets::set(SERVICE, &account(address), token).map_err(not_kept)
}

/// Forgets the token for the church at this address (here only: TimelyChurch lists the
/// computer until it is disconnected there).
pub fn forget(address: &str) {
    if let Some(file) = test_file() {
        let _ = std::fs::remove_file(file);
    } else {
        let _ = secrets::delete(SERVICE, &account(address));
    }
}

/// A stand-in for `crate::secrets`, the secret store being written now (the Keychain, Windows
/// Credential Manager, Linux Secret Service), with the same three calls. To be replaced: once
/// `secrets.rs` is in, this module goes and `use crate::secrets;` takes its place. Until then
/// only a Mac keeps a token, in its Keychain, as VoxLive's key is kept.
mod secrets {
    /// Tests keep tokens in memory: they never touch the computer's own store.
    #[cfg(test)]
    static KEPT: std::sync::Mutex<Vec<(String, String, String)>> = std::sync::Mutex::new(Vec::new());

    #[cfg(test)]
    pub fn get(service: &str, account: &str) -> Result<Option<String>, String> {
        let kept = KEPT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(kept.iter().find(|(s, a, _)| s == service && a == account).map(|(_, _, secret)| secret.clone()))
    }

    #[cfg(test)]
    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        delete(service, account)?;
        let mut kept = KEPT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        kept.push((service.to_owned(), account.to_owned(), secret.to_owned()));
        Ok(())
    }

    #[cfg(test)]
    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        let mut kept = KEPT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        kept.retain(|(s, a, _)| !(s == service && a == account));
        Ok(())
    }

    #[cfg(all(target_os = "macos", not(test)))]
    pub fn get(service: &str, account: &str) -> Result<Option<String>, String> {
        match security_framework::passwords::get_generic_password(service, account) {
            Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
            Err(_) => Ok(None),
        }
    }

    #[cfg(all(target_os = "macos", not(test)))]
    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        security_framework::passwords::set_generic_password(service, account, secret.as_bytes())
            .map_err(|e| e.to_string())
    }

    #[cfg(all(target_os = "macos", not(test)))]
    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        security_framework::passwords::delete_generic_password(service, account).map_err(|e| e.to_string())
    }

    #[cfg(all(not(target_os = "macos"), not(test)))]
    pub fn get(_service: &str, _account: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    #[cfg(all(not(target_os = "macos"), not(test)))]
    pub fn set(_service: &str, _account: &str, _secret: &str) -> Result<(), String> {
        Err(crate::i18n::t(
            "integrations.timelychurch.mac_only",
            "keeping the connection needs a Mac until this build can use the system's secure store",
        )
        .to_owned())
    }

    #[cfg(all(not(target_os = "macos"), not(test)))]
    pub fn delete(_service: &str, _account: &str) -> Result<(), String> {
        Ok(())
    }
}
