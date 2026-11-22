//! Remote Control's certificate: made on this computer the first time Remote Control is
//! switched on (an ECDSA P-256 key, signed by itself), kept in the private `remote` folder with
//! the key readable by this user only, and made again on request.
//!
//! Nobody vouches for it, so a browser warns the first time it is opened. Its SHA-256
//! fingerprint, shown in Settings › Remote Control, is what the operator checks against the
//! one the browser shows: the same fingerprint means nobody is in between.
//!
//! It names no address, so it stays the same when the computer's address changes.

use std::path::Path;

use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

const CERT_FILE: &str = "cert.der";
const KEY_FILE: &str = "key.der";

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

/// The certificate kept in `folder`, or a new one made and kept there when there is none (or
/// what is there can't be read).
pub fn load_or_make(folder: &Path) -> Result<Cert, String> {
    match load(folder) {
        Some(cert) => Ok(cert),
        None => make(folder),
    }
}

fn load(folder: &Path) -> Option<Cert> {
    let cert = std::fs::read(folder.join(CERT_FILE)).ok()?;
    let key = std::fs::read(folder.join(KEY_FILE)).ok()?;
    if cert.is_empty() || key.is_empty() {
        return None;
    }
    Some(from_der(cert, key))
}

fn from_der(cert: Vec<u8>, key: Vec<u8>) -> Cert {
    let fingerprint = fingerprint(&cert);
    Cert {
        chain: vec![CertificateDer::from(cert)],
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        fingerprint,
    }
}

/// Makes a new certificate and key and keeps them in `folder`, in place of any there.
pub fn make(folder: &Path) -> Result<Cert, String> {
    let key = rcgen::KeyPair::generate().map_err(|e| format!("Could not make a key: {e}"))?;
    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_owned()])
        .map_err(|e| format!("Could not make a certificate: {e}"))?;
    let mut name = rcgen::DistinguishedName::new();
    name.push(rcgen::DnType::CommonName, "SoftPresenter Remote Control");
    params.distinguished_name = name;
    // From yesterday (a computer whose clock is a little behind still takes it) for ten years.
    let today = chrono::Local::now().date_naive();
    let day = |date: chrono::NaiveDate| {
        use chrono::Datelike;
        rcgen::date_time_ymd(date.year(), date.month() as u8, date.day() as u8)
    };
    params.not_before = day(today - chrono::Days::new(1));
    params.not_after = day(today + chrono::Days::new(3650));
    let cert = params.self_signed(&key).map_err(|e| format!("Could not sign the certificate: {e}"))?;
    let (cert, key) = (cert.der().to_vec(), key.serialize_der());
    crate::remote::auth::write_private(folder, KEY_FILE, &key)?;
    crate::remote::auth::write_private(folder, CERT_FILE, &cert)?;
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

    #[test]
    fn a_certificate_is_made_once_kept_private_and_made_again_on_request() {
        let folder = std::env::temp_dir().join(format!("sp-remote-cert-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let first = load_or_make(&folder).expect("made");
        assert_eq!(first.fingerprint.len(), 32 * 3 - 1);
        let again = load_or_make(&folder).expect("read");
        assert_eq!(again.fingerprint, first.fingerprint, "the one kept is used again");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(folder.join(KEY_FILE)).expect("kept").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "the key is this user's alone");
        }
        let new = make(&folder).expect("made again");
        assert_ne!(new.fingerprint, first.fingerprint);
        assert_eq!(load_or_make(&folder).expect("read").fingerprint, new.fingerprint);
        // rustls takes it.
        let config = tokio_rustls::rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(
            tokio_rustls::rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(new.chain.clone(), new.key.clone_key());
        assert!(config.is_ok());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
