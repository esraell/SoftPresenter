//! Remote Control's certificate: made on this computer (an ECDSA P-256 key, signed by itself),
//! kept in the private `remote` folder with the key readable by this user only, and made again
//! on request, before it runs out, and when the computer's names change.
//!
//! Nobody vouches for it, so a browser warns the first time it is opened. Its SHA-256
//! fingerprint, shown in Settings › Remote Control, is what the operator checks against the one
//! the browser shows: the same fingerprint means nobody is in between.
//!
//! It is made as Apple's systems want a server's certificate to be, so an iPad or iPhone can
//! take it: for at most 397 days (it is made again a month before it runs out), for serving
//! (the server-authentication purpose), and naming the addresses it is opened at: `localhost`,
//! 127.0.0.1, the computer's address on its network and its own name there (`church-mac.local`).
//! When the address or the name changes, it is made again, and Settings shows the new
//! fingerprint.

use std::net::IpAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio_rustls::rustls;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

const CERT_FILE: &str = "cert.der";
const KEY_FILE: &str = "key.der";
const ABOUT_FILE: &str = "cert.json";

/// How long a certificate is made for, and how long before it runs out it is made again.
pub const LASTS_DAYS: u64 = 396;
pub const RENEW_DAYS_BEFORE: u64 = 30;

/// The certificate and its key, as rustls takes them, and the fingerprint to show.
pub struct Cert {
    pub chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
    /// SHA-256 of the certificate, as browsers show it: "AB:CD:…".
    pub fingerprint: String,
}

impl Clone for Cert {
    fn clone(&self) -> Cert {
        Cert { chain: self.chain.clone(), key: self.key.clone_key(), fingerprint: self.fingerprint.clone() }
    }
}

/// What a certificate was made for, kept beside it: the names in it and when it runs out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Names {
    /// Host names (`localhost`, `church-mac.local`) and addresses (`127.0.0.1`), as written.
    pub names: Vec<String>,
    /// When it runs out, in seconds since 1970.
    #[serde(default)]
    pub until: u64,
}

impl Names {
    /// The names this computer has now: `localhost`, 127.0.0.1, its address on the network
    /// (`lan`) and its own name there.
    pub fn now(lan: Option<IpAddr>, own_name: Option<String>) -> Names {
        let mut names = vec!["localhost".to_owned(), "127.0.0.1".to_owned()];
        if let Some(IpAddr::V4(v4)) = lan.filter(|ip| !ip.is_loopback() && !ip.is_unspecified()) {
            names.push(v4.to_string());
        }
        if let Some(name) = own_name {
            names.push(name);
        }
        Names { names, until: 0 }
    }

    /// The computer's own name on the network, if the certificate names one (for the Host
    /// check).
    pub fn local_name(&self) -> Option<String> {
        self.names.iter().find(|name| name.ends_with(".local")).cloned()
    }
}

/// The computer's own name on the local network (`church-mac.local`), as Bonjour has it on a
/// Mac; on other systems its host name with `.local`. Only letters, figures and dashes are
/// kept; `None` when there is none.
pub fn own_name() -> Option<String> {
    let raw = if cfg!(target_os = "macos") {
        std::process::Command::new("/usr/sbin/scutil")
            .args(["--get", "LocalHostName"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        std::env::var("COMPUTERNAME")
            .ok()
            .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
            .map(|name| name.trim().split('.').next().unwrap_or("").to_owned())
    }?;
    let name: String = raw.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(63).collect();
    let name = name.trim_matches('-').to_ascii_lowercase();
    (!name.is_empty()).then(|| format!("{name}.local"))
}

/// The certificate kept in `folder` when it can be used as it is: there, whole, its key its
/// own, made for these `names`, and not running out within a month. Otherwise a new one, made
/// and kept; the flag says a new one was made.
pub fn load_or_make(folder: &Path, names: &Names) -> Result<(Cert, bool), String> {
    match load(folder, names) {
        Some(cert) => Ok((cert, false)),
        None => make(folder, names).map(|cert| (cert, true)),
    }
}

/// Whether the certificate kept would be made again now (see `load_or_make`).
pub fn needs_making(folder: &Path, names: &Names) -> bool {
    load(folder, names).is_none()
}

fn load(folder: &Path, names: &Names) -> Option<Cert> {
    let cert = std::fs::read(folder.join(CERT_FILE)).ok()?;
    let key = std::fs::read(folder.join(KEY_FILE)).ok()?;
    let about: Names = serde_json::from_slice(&std::fs::read(folder.join(ABOUT_FILE)).ok()?).ok()?;
    if cert.is_empty() || key.is_empty() || about.names != names.names {
        return None;
    }
    let renew_at = about.until.saturating_sub(RENEW_DAYS_BEFORE * 24 * 60 * 60);
    if crate::remote::auth::now_seconds() >= renew_at {
        return None;
    }
    let cert = from_der(cert, key);
    // The key and the certificate must belong together (rustls checks): a half-written pair
    // from an earlier run is made again.
    usable(&cert).then_some(cert)
}

fn from_der(cert: Vec<u8>, key: Vec<u8>) -> Cert {
    let fingerprint = fingerprint(&cert);
    Cert {
        chain: vec![CertificateDer::from(cert)],
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        fingerprint,
    }
}

/// Whether rustls takes the pair: the certificate's key is the key kept.
fn usable(cert: &Cert) -> bool {
    rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .and_then(|config| config.with_no_client_auth().with_single_cert(cert.chain.clone(), cert.key.clone_key()))
        .is_ok()
}

/// Makes a new certificate and key for `names` and keeps them in `folder`, in place of any
/// there: each written whole to a file of its own beside it, then put in place.
pub fn make(folder: &Path, names: &Names) -> Result<Cert, String> {
    let key = rcgen::KeyPair::generate().map_err(|e| format!("Could not make a key: {e}"))?;
    let mut params =
        rcgen::CertificateParams::new(names.names.clone()).map_err(|e| format!("Could not make a certificate: {e}"))?;
    let mut name = rcgen::DistinguishedName::new();
    name.push(rcgen::DnType::CommonName, "SoftPresenter Remote Control");
    params.distinguished_name = name;
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
    // From yesterday (a computer whose clock is a little behind still takes it), for 396 days.
    let today = chrono::Local::now().date_naive();
    let day = |date: chrono::NaiveDate| {
        use chrono::Datelike;
        rcgen::date_time_ymd(date.year(), date.month() as u8, date.day() as u8)
    };
    let until = today + chrono::Days::new(LASTS_DAYS - 1);
    params.not_before = day(today - chrono::Days::new(1));
    params.not_after = day(until);
    let cert = params.self_signed(&key).map_err(|e| format!("Could not sign the certificate: {e}"))?;
    let (cert, key) = (cert.der().to_vec(), key.serialize_der());
    let until_seconds = until.and_hms_opt(0, 0, 0).map_or(0, |at| at.and_utc().timestamp().max(0) as u64);
    let about = Names { names: names.names.clone(), until: until_seconds };
    let about = serde_json::to_vec_pretty(&about).map_err(|e| e.to_string())?;
    // The note last, so a pair cut short by a crash is never taken for whole (and the pair is
    // checked on loading anyway).
    let _ = std::fs::remove_file(folder.join(ABOUT_FILE));
    crate::remote::auth::write_private(folder, KEY_FILE, &key)?;
    crate::remote::auth::write_private(folder, CERT_FILE, &cert)?;
    crate::remote::auth::write_private(folder, ABOUT_FILE, &about)?;
    Ok(from_der(cert, key))
}

/// The SHA-256 of a certificate, in pairs of capital hex figures with colons between.
pub fn fingerprint(der: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, der);
    digest.as_ref().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(lan: &str) -> Names {
        Names::now(Some(lan.parse().unwrap()), Some("church-mac.local".to_owned()))
    }

    #[test]
    fn a_certificate_is_made_once_kept_private_and_made_again_when_it_must_be() {
        let folder = std::env::temp_dir().join(format!("sp-remote-cert-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let here = names("192.168.1.20");
        assert_eq!(here.names, ["localhost", "127.0.0.1", "192.168.1.20", "church-mac.local"]);
        assert_eq!(here.local_name().as_deref(), Some("church-mac.local"));
        let (first, made) = load_or_make(&folder, &here).expect("made");
        assert!(made);
        assert_eq!(first.fingerprint.len(), 32 * 3 - 1);
        let (again, made) = load_or_make(&folder, &here).expect("read");
        assert!(!made);
        assert_eq!(again.fingerprint, first.fingerprint, "the one kept is used again");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(folder.join(KEY_FILE)).expect("kept").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "the key is this user's alone");
        }
        // It runs out in under 397 days.
        let about: Names = serde_json::from_slice(&std::fs::read(folder.join(ABOUT_FILE)).unwrap()).unwrap();
        let days = about.until.saturating_sub(crate::remote::auth::now_seconds()) / 86_400;
        assert!((390..397).contains(&days), "{days} days");
        // A new address on the network: made again.
        assert!(needs_making(&folder, &names("192.168.1.21")));
        let (moved, made) = load_or_make(&folder, &names("192.168.1.21")).expect("made again");
        assert!(made && moved.fingerprint != first.fingerprint);
        // Near its end: made again.
        let mut near = about;
        near.names = names("192.168.1.21").names;
        near.until = crate::remote::auth::now_seconds() + 5 * 86_400;
        crate::remote::auth::write_private(&folder, ABOUT_FILE, &serde_json::to_vec(&near).unwrap()).unwrap();
        assert!(needs_making(&folder, &names("192.168.1.21")));
        // A key that isn't the certificate's (a pair cut short): made again.
        let (_, _) = load_or_make(&folder, &names("192.168.1.21")).expect("made");
        let other = rcgen::KeyPair::generate().unwrap().serialize_der();
        crate::remote::auth::write_private(&folder, KEY_FILE, &other).unwrap();
        assert!(needs_making(&folder, &names("192.168.1.21")));
        // On request: a new one.
        let new = make(&folder, &names("192.168.1.21")).expect("made again");
        assert!(usable(&new));
        assert_eq!(load_or_make(&folder, &names("192.168.1.21")).expect("read").0.fingerprint, new.fingerprint);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_computers_own_name_is_letters_figures_and_dashes() {
        if let Some(name) = own_name() {
            assert!(name.ends_with(".local"));
            assert!(name.trim_end_matches(".local").chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        }
    }
}
