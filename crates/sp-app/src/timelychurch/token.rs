//! Where the device token is kept: the app's store for secrets (`crate::secrets`: the Keychain
//! on a Mac, Credential Manager on Windows, the Secret Service on Linux; a test copy started with
//! `SOFTPRESENTER_DATA` keeps its own in its own folder), under the church it was made for. It is
//! never written to a settings file, the log, a command's reply or a song pack.
//!
//! Each call may wait for the system (macOS may first ask whether the app may have the token),
//! so they are made only on threads of their own.

/// The store's entry the token is kept under: the app's, for TimelyChurch.
const SERVICE: &str = "SoftPresenter TimelyChurch";

/// The account a church's token is kept under: its address as a key holds it, so two churches
/// on one computer never share one.
fn account(address: &str) -> String {
    super::church_part(address).chars().take(100).collect()
}

/// The token kept for the church at this address, if there is one.
pub fn load(address: &str) -> Option<String> {
    store::get(SERVICE, &account(address)).ok().flatten().filter(|token| !token.is_empty())
}

/// Keeps the token for the church at this address. The error is a sentence.
pub fn save(address: &str, token: &str) -> Result<(), String> {
    store::set(SERVICE, &account(address), token).map_err(|error| {
        crate::i18n::tf(
            "integrations.timelychurch.not_kept",
            "The connection could not be kept on this computer: {error}",
            &[("error", &error)],
        )
    })
}

/// Forgets the token for the church at this address (here only: TimelyChurch lists the
/// computer until it is disconnected there).
pub fn forget(address: &str) {
    let _ = store::delete(SERVICE, &account(address));
}

#[cfg(not(test))]
use crate::secrets as store;

/// Tests keep tokens in memory: they never touch the computer's own store.
#[cfg(test)]
mod store {
    static KEPT: std::sync::Mutex<Vec<(String, String, String)>> = std::sync::Mutex::new(Vec::new());

    pub fn get(service: &str, account: &str) -> Result<Option<String>, String> {
        let kept = KEPT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(kept.iter().find(|(s, a, _)| s == service && a == account).map(|(_, _, secret)| secret.clone()))
    }

    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        delete(service, account)?;
        let mut kept = KEPT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        kept.push((service.to_owned(), account.to_owned(), secret.to_owned()));
        Ok(())
    }

    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        let mut kept = KEPT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        kept.retain(|(s, a, _)| !(s == service && a == account));
        Ok(())
    }
}
