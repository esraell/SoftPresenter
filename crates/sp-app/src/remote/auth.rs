//! Signing in to Remote Control: the password, the devices remembered, and the guard that
//! slows down guessing.
//!
//! - The password is kept only as an argon2id hash with a random salt (the PHC string argon2
//!   writes), never as itself, and is never logged.
//! - A device that signs in is given a long random token from the system's random source, as a
//!   cookie; only the token's SHA-256 is kept, with the device's name (from its browser), when
//!   it was first and last seen, and from which address.
//! - Forgetting a device forgets its token; a new password forgets every device.
//! - Wrong passwords from an address are answered more and more slowly, then not at all for a
//!   while; so are wrong passwords from everywhere together, past a limit a minute.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

/// The shortest password Settings takes.
pub const SHORTEST_PASSWORD: usize = 10;
/// The longest: more is refused before it is hashed.
pub const LONGEST_PASSWORD: usize = 256;
/// The most devices remembered at once; one more must wait until one is forgotten.
pub const MOST_DEVICES: usize = 50;
/// A device not seen for this long is forgotten (it signs in again with the password).
pub const FORGET_UNUSED_AFTER: u64 = 180 * 24 * 60 * 60;

/// What is kept in `remote/settings.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Saved {
    /// Switched on: listens again when the app next opens.
    #[serde(default)]
    pub on: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    /// The password's argon2id hash (a PHC string), once one is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default)]
    pub devices: Vec<Device>,
}

/// The port Remote Control listens on unless another is chosen.
pub const DEFAULT_PORT: u16 = 4316;

fn default_port() -> u16 {
    DEFAULT_PORT
}

/// A device that signed in, and is let in again without the password until it is forgotten.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device {
    /// A short random id, to name it in Settings and in the log (never the token).
    pub id: String,
    /// What it is, from its browser: "Safari on an iPad".
    pub name: String,
    /// The SHA-256 of its token, in hex.
    pub token: String,
    /// Seconds since 1970.
    pub first_seen: u64,
    pub last_seen: u64,
    /// The address it last came from.
    pub ip: String,
}

/// The settings and devices, kept in a folder only this user may enter.
pub struct Store {
    folder: PathBuf,
    pub saved: Saved,
    /// Devices seen since the file was last written: written at most once a minute for that.
    seen_unsaved: bool,
    last_written: Instant,
}

impl Store {
    /// Reads the store from `folder` (made, and kept private, if it isn't there). A file that
    /// can't be read is left alone and a fresh store used: Remote Control off, no password.
    pub fn open(folder: &Path) -> Store {
        let saved = std::fs::read(folder.join("settings.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .unwrap_or_else(|| Saved { port: DEFAULT_PORT, ..Saved::default() });
        let mut store = Store { folder: folder.to_owned(), saved, seen_unsaved: false, last_written: Instant::now() };
        store.forget_unused(now_seconds());
        store
    }

    /// Forgets the devices not seen for [`FORGET_UNUSED_AFTER`]; true if any were.
    pub fn forget_unused(&mut self, now: u64) -> bool {
        let before = self.saved.devices.len();
        self.saved.devices.retain(|device| now.saturating_sub(device.last_seen) < FORGET_UNUSED_AFTER);
        let gone = self.saved.devices.len() != before;
        if gone {
            let _ = self.save();
        }
        gone
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Writes the store now.
    pub fn save(&mut self) -> Result<(), String> {
        let text = serde_json::to_vec_pretty(&self.saved).map_err(|e| e.to_string())?;
        write_private(&self.folder, "settings.json", &text)?;
        self.seen_unsaved = false;
        self.last_written = Instant::now();
        Ok(())
    }

    /// Whether a device was seen and the file hasn't been written for a minute.
    pub fn seen_due(&self) -> bool {
        self.seen_unsaved && self.last_written.elapsed() >= Duration::from_secs(60)
    }

    /// Writes the store if `seen_due`.
    pub fn save_seen(&mut self) {
        if self.seen_due() {
            let _ = self.save();
        }
    }

    /// The device a token belongs to, if one does. Every device's hash is compared in full,
    /// in time that doesn't depend on where they differ.
    pub fn device_of(&self, token: &str) -> Option<&Device> {
        let wanted = token_hash(token);
        let mut found = None;
        for device in &self.saved.devices {
            if same(device.token.as_bytes(), wanted.as_bytes()) {
                found = Some(device);
            }
        }
        found
    }

    /// Notes that a device was seen now, from this address.
    pub fn seen(&mut self, id: &str, ip: IpAddr) {
        if let Some(device) = self.saved.devices.iter_mut().find(|d| d.id == id) {
            device.last_seen = now_seconds();
            device.ip = ip.to_string();
            self.seen_unsaved = true;
        }
    }

    /// Remembers a device that just signed in with the password kept as `verified` (refused if
    /// the password has changed since it was checked); returns its token (for its cookie) and
    /// its id.
    pub fn remember(&mut self, name: &str, ip: IpAddr, verified: &str) -> Result<(String, String), String> {
        if self.saved.password.as_deref() != Some(verified) {
            return Err("The password changed a moment ago: sign in with the new one.".to_owned());
        }
        if self.saved.devices.len() >= MOST_DEVICES {
            return Err(format!(
                "{MOST_DEVICES} devices are remembered already. Forget one in Settings › Remote Control first."
            ));
        }
        let token = random_word(32)?;
        let id = random_word(9)?;
        let now = now_seconds();
        self.saved.devices.push(Device {
            id: id.clone(),
            name: name.to_owned(),
            token: token_hash(&token),
            first_seen: now,
            last_seen: now,
            ip: ip.to_string(),
        });
        self.save()?;
        Ok((token, id))
    }

    /// Forgets one device, by id; true if it was remembered.
    pub fn forget(&mut self, id: &str) -> bool {
        let before = self.saved.devices.len();
        self.saved.devices.retain(|device| device.id != id);
        let gone = self.saved.devices.len() != before;
        if gone {
            let _ = self.save();
        }
        gone
    }

    /// Forgets every device.
    pub fn forget_all(&mut self) {
        self.saved.devices.clear();
        let _ = self.save();
    }

    /// Sets a new password (already hashed): every device is forgotten with the old one.
    pub fn set_password(&mut self, hash: String) -> Result<(), String> {
        self.saved.password = Some(hash);
        self.saved.devices.clear();
        self.save()
    }
}

/// Writes a file in a private folder, readable by this user only, all at once: a reader never
/// sees half of it.
pub fn write_private(folder: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    crate::private_folder::keep_private(folder)?;
    let target = folder.join(name);
    let partial = folder.join(format!(".{name}.partial"));
    let _ = std::fs::remove_file(&partial);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = (|| {
        use std::io::Write;
        let mut file = options.open(&partial)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&partial, &target)
    })();
    written.map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        format!("Could not write {}: {e}", target.display())
    })
}

/// Seconds since 1970.
pub fn now_seconds() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The system's random bytes, `count` of them, as a word for a cookie or an id (URL-safe
/// base64, no padding).
pub fn random_word(count: usize) -> Result<String, String> {
    let mut bytes = vec![0u8; count];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes)
        .map_err(|_| "The system's random source didn't answer.".to_owned())?;
    Ok(base64url(&bytes))
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 2);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for at in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * at)) as usize & 63] as char);
        }
    }
    out
}

/// A token's SHA-256, in hex.
pub fn token_hash(token: &str) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, token.as_bytes()).as_ref())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether two byte strings are the same, in time that depends only on their lengths.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// Hashes a password for keeping: argon2id with argon2's own default costs (19 MiB, two passes)
/// and a salt of 16 random bytes. Slow on purpose: never on the window's thread.
pub fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let chars = password.chars().count();
    if chars < SHORTEST_PASSWORD {
        return Err(format!("A password needs at least {SHORTEST_PASSWORD} letters, figures or signs."));
    }
    if chars > LONGEST_PASSWORD {
        return Err(format!("A password may be at most {LONGEST_PASSWORD} long."));
    }
    let mut salt = [0u8; 16];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut salt)
        .map_err(|_| "The system's random source didn't answer.".to_owned())?;
    let salt = SaltString::encode_b64(&salt).map_err(|e| e.to_string())?;
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| e.to_string())
}

/// Whether a password is the one kept as `hash`. Slow on purpose, as `hash_password` is.
pub fn password_is(hash: &str, password: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    if password.len() > LONGEST_PASSWORD * 4 {
        return false;
    }
    PasswordHash::new(hash)
        .is_ok_and(|parsed| argon2::Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// A device's name from its browser's user agent: "Safari on an iPad", "Chrome on Windows". Only
/// these few words are kept, never the whole string.
pub fn device_name(agent: &str) -> String {
    let has = |word: &str| agent.contains(word);
    let browser = if has("Edg/") || has("EdgA/") || has("EdgiOS/") {
        "Edge"
    } else if has("OPR/") {
        "Opera"
    } else if has("Firefox/") || has("FxiOS/") {
        "Firefox"
    } else if has("Chrome/") || has("CriOS/") {
        "Chrome"
    } else if has("Safari/") {
        "Safari"
    } else {
        "A browser"
    };
    let system = if has("iPad") {
        Some("an iPad")
    } else if has("iPhone") {
        Some("an iPhone")
    } else if has("Android") {
        Some("Android")
    } else if has("Windows") {
        Some("Windows")
    } else if has("CrOS") {
        Some("a Chromebook")
    } else if has("Macintosh") || has("Mac OS X") {
        Some("a Mac")
    } else if has("Linux") {
        Some("Linux")
    } else {
        None
    };
    match system {
        Some(system) => format!("{browser} on {system}"),
        None => browser.to_owned(),
    }
}

/// Wrong passwords from one address, and how long it must wait.
#[derive(Clone, Copy, Debug)]
struct Tries {
    wrong: u32,
    wait_until: Instant,
    last: Instant,
}

/// Slows down guessing. The first few wrong passwords from an address are answered at once;
/// then each must wait twice as long as the one before (1 s, 2 s, 4 s… up to a minute); ten in
/// a row shut the address out for a quarter of an hour. Past 30 wrong passwords a minute from
/// everywhere together, every address waits out the rest of that minute. A right password
/// clears its address.
pub struct Guard {
    tries: HashMap<IpAddr, Tries>,
    /// Wrong passwords from everywhere in the minute that began then.
    minute: (Instant, u32),
    /// Wrong passwords from everywhere lately (one less for each half minute without one), and
    /// when the last came: past a few, every sign-in waits, longer the more there are.
    everyone: (f64, Instant),
    /// When the wrong passwords of the last hour came, for Settings and the status line.
    lately: std::collections::VecDeque<Instant>,
}

/// Wrong passwords answered at once before waits begin.
const FREE_TRIES: u32 = 3;
/// Wrong passwords in a row that shut an address out.
const SHUT_OUT_AFTER: u32 = 10;
const SHUT_OUT_FOR: Duration = Duration::from_secs(15 * 60);
const LONGEST_WAIT: Duration = Duration::from_secs(60);
/// Wrong passwords a minute from everywhere together.
const MOST_A_MINUTE: u32 = 30;
/// Addresses remembered at once; the one heard from longest ago goes first.
const MOST_ADDRESSES: usize = 1024;
/// Wrong passwords from everywhere, lately, before every sign-in waits; and the longest wait
/// that grows to.
const EVERYONE_FREE: f64 = 10.0;
const EVERYONE_LONGEST: Duration = Duration::from_secs(120);

impl Default for Guard {
    fn default() -> Guard {
        Guard {
            tries: HashMap::new(),
            minute: (Instant::now(), 0),
            everyone: (0.0, Instant::now()),
            lately: std::collections::VecDeque::new(),
        }
    }
}

impl Guard {
    /// How long an address must wait before its password is even looked at; `None` when it may
    /// try now.
    pub fn wait(&self, ip: IpAddr, now: Instant) -> Option<Duration> {
        let minute = (self.minute.1 >= MOST_A_MINUTE && now < self.minute.0 + Duration::from_secs(60))
            .then(|| self.minute.0 + Duration::from_secs(60) - now);
        let own = self.tries.get(&ip).filter(|t| t.wait_until > now).map(|t| t.wait_until - now);
        let (_, last) = self.everyone;
        let everyone =
            Some(last + everyone_wait(self.everyone_now(now))).filter(|until| *until > now).map(|until| until - now);
        [own, minute, everyone].into_iter().flatten().max()
    }

    /// Wrong passwords from everywhere lately, as of `now`.
    fn everyone_now(&self, now: Instant) -> f64 {
        let (count, last) = self.everyone;
        (count - now.saturating_duration_since(last).as_secs_f64() / 30.0).max(0.0)
    }

    /// Wrong passwords in the hour before `now`.
    pub fn wrong_lately(&self, now: Instant) -> usize {
        self.lately.iter().filter(|at| now.saturating_duration_since(**at) < Duration::from_secs(3600)).count()
    }

    /// A wrong password from an address: returns how long it must now wait, if at all.
    pub fn wrong(&mut self, ip: IpAddr, now: Instant) -> Option<Duration> {
        if now >= self.minute.0 + Duration::from_secs(60) {
            self.minute = (now, 0);
        }
        self.minute.1 += 1;
        self.everyone = (self.everyone_now(now) + 1.0, now);
        self.lately.push_back(now);
        while self.lately.front().is_some_and(|at| now.saturating_duration_since(*at) >= Duration::from_secs(3600)) {
            self.lately.pop_front();
        }
        if self.lately.len() > 10_000 {
            self.lately.pop_front();
        }
        if !self.tries.contains_key(&ip) && self.tries.len() >= MOST_ADDRESSES {
            let oldest = self.tries.iter().min_by_key(|(_, t)| t.last).map(|(ip, _)| *ip);
            if let Some(oldest) = oldest {
                self.tries.remove(&oldest);
            }
        }
        let tries = self.tries.entry(ip).or_insert(Tries { wrong: 0, wait_until: now, last: now });
        tries.wrong += 1;
        tries.last = now;
        let wait = if tries.wrong >= SHUT_OUT_AFTER {
            SHUT_OUT_FOR
        } else if tries.wrong > FREE_TRIES {
            Duration::from_secs(1u64 << (tries.wrong - FREE_TRIES - 1).min(6)).min(LONGEST_WAIT)
        } else {
            Duration::ZERO
        };
        tries.wait_until = now + wait;
        self.wait(ip, now)
    }

    /// The right password from an address: it starts afresh.
    pub fn right(&mut self, ip: IpAddr) {
        self.tries.remove(&ip);
    }
}

/// How long every sign-in waits after the last wrong password, for so many lately from
/// everywhere: nothing up to [`EVERYONE_FREE`], then 1 s, doubling every five more, up to
/// [`EVERYONE_LONGEST`].
fn everyone_wait(lately: f64) -> Duration {
    if lately <= EVERYONE_FREE {
        return Duration::ZERO;
    }
    let doublings = ((lately - EVERYONE_FREE) / 5.0).floor().min(10.0);
    Duration::from_secs_f64(2f64.powf(doublings)).min(EVERYONE_LONGEST)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("sp-remote-auth-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn a_password_is_kept_only_as_a_salted_argon2id_hash() {
        let hash = hash_password("correct horse").expect("hashed");
        assert!(hash.starts_with("$argon2id$"), "{hash}");
        assert!(!hash.contains("correct horse"));
        assert!(password_is(&hash, "correct horse"));
        assert!(!password_is(&hash, "correct hors"));
        assert!(!password_is("not a hash", "correct horse"));
        // The same password twice has two salts, so two hashes.
        assert_ne!(hash, hash_password("correct horse").expect("hashed"));
        assert!(hash_password("short").is_err());
        assert!(hash_password("nine char").is_err(), "at least ten");
        assert!(hash_password("ten chars!").is_ok());
        assert!(hash_password(&"x".repeat(LONGEST_PASSWORD + 1)).is_err());
    }

    #[test]
    fn a_device_is_found_by_its_token_and_forgotten_with_it_or_a_new_password() {
        let folder = folder("devices");
        let ip: IpAddr = "192.168.1.20".parse().unwrap();
        let mut store = Store::open(&folder);
        assert_eq!(store.saved.port, DEFAULT_PORT);
        let first = hash_password("first password").expect("hashed");
        store.set_password(first.clone()).expect("set");
        let (token, id) = store.remember("Safari on an iPad", ip, &first).expect("remembered");
        assert!(token.len() >= 43, "a 256-bit token: {token}");
        assert_eq!(store.device_of(&token).map(|d| d.id.clone()), Some(id.clone()));
        assert!(store.device_of("a token nobody was given").is_none());
        // Only the token's hash is written down.
        let written = std::fs::read_to_string(folder.join("settings.json")).expect("written");
        assert!(!written.contains(&token));
        assert!(written.contains(&token_hash(&token)));
        // Read again, it still lets the device in.
        let mut again = Store::open(&folder);
        assert!(again.device_of(&token).is_some());
        assert!(again.forget(&id));
        assert!(again.device_of(&token).is_none());
        // A new password forgets every device.
        let (token, _) = again.remember("Chrome on Windows", ip, &first).expect("remembered");
        let second = hash_password("another password").expect("hashed");
        again.set_password(second.clone()).expect("set");
        assert!(again.device_of(&token).is_none());
        // A sign-in checked against the old password, finishing after the change, is refused.
        assert!(again.remember("Chrome on Windows", ip, &first).is_err());
        assert!(again.remember("Chrome on Windows", ip, &second).is_ok());
        again.forget_all();
        assert!(Store::open(&folder).saved.devices.is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(folder.join("settings.json")).expect("there").permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
            let mode = std::fs::metadata(&folder).expect("there").permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn wrong_passwords_wait_longer_and_longer_then_shut_the_address_out() {
        let mut guard = Guard::default();
        let ip: IpAddr = "10.0.0.5".parse().unwrap();
        let other: IpAddr = "10.0.0.6".parse().unwrap();
        let now = Instant::now();
        assert_eq!(guard.wait(ip, now), None);
        for _ in 0..FREE_TRIES {
            assert_eq!(guard.wrong(ip, now), None);
        }
        assert_eq!(guard.wrong(ip, now), Some(Duration::from_secs(1)));
        assert_eq!(guard.wrong(ip, now), Some(Duration::from_secs(2)));
        assert_eq!(guard.wrong(ip, now), Some(Duration::from_secs(4)));
        assert_eq!(guard.wait(other, now), None, "another address is not held up");
        for _ in 0..(SHUT_OUT_AFTER - FREE_TRIES - 3) {
            guard.wrong(ip, now);
        }
        assert_eq!(guard.wait(ip, now), Some(SHUT_OUT_FOR));
        assert_eq!(guard.wait(ip, now + SHUT_OUT_FOR), None);
        guard.right(ip);
        assert_eq!(guard.wait(ip, now), None);
    }

    #[test]
    fn too_many_wrong_passwords_from_everywhere_hold_everyone_for_the_minute() {
        let mut guard = Guard::default();
        let now = Instant::now();
        for n in 0..MOST_A_MINUTE {
            guard.wrong(IpAddr::from([10, 0, (n / 200) as u8, (n % 200) as u8]), now);
        }
        let newcomer: IpAddr = "10.9.9.9".parse().unwrap();
        assert!(guard.wait(newcomer, now + Duration::from_secs(1)).is_some());
        assert_eq!(guard.wait(newcomer, now + Duration::from_secs(61)), None);
    }

    #[test]
    fn wrong_passwords_from_everywhere_slow_every_sign_in_more_and_more() {
        let mut guard = Guard::default();
        let now = Instant::now();
        let fresh: IpAddr = "10.9.9.9".parse().unwrap();
        // Two wrong ones from each of many addresses (none waits on its own account).
        let mut n = 0u32;
        let mut wrong = |guard: &mut Guard, times: u32| {
            for _ in 0..times {
                n += 1;
                guard.wrong(IpAddr::from([10, 1, (n / 2 / 200) as u8, (n / 2 % 200) as u8]), now);
            }
        };
        wrong(&mut guard, 10);
        assert_eq!(guard.wait(fresh, now), None, "a few are let be");
        wrong(&mut guard, 1);
        assert_eq!(guard.wait(fresh, now), Some(Duration::from_secs(1)));
        wrong(&mut guard, 5);
        assert_eq!(guard.wait(fresh, now), Some(Duration::from_secs(2)));
        wrong(&mut guard, 5);
        assert_eq!(guard.wait(fresh, now), Some(Duration::from_secs(4)));
        // Quiet for a while, it eases off.
        assert_eq!(guard.wait(fresh, now + Duration::from_secs(600)), None);
        assert_eq!(guard.wrong_lately(now), 21);
        assert_eq!(guard.wrong_lately(now + Duration::from_secs(3601)), 0);
        assert_eq!(everyone_wait(1000.0), EVERYONE_LONGEST);
    }

    #[test]
    fn a_device_unused_for_half_a_year_is_forgotten() {
        let folder = folder("unused");
        let ip: IpAddr = "192.168.1.30".parse().unwrap();
        let mut store = Store::open(&folder);
        let hash = hash_password("a long password").expect("hashed");
        store.set_password(hash.clone()).expect("set");
        let (old, _) = store.remember("Safari on an iPad", ip, &hash).expect("remembered");
        let (recent, _) = store.remember("Chrome on Windows", ip, &hash).expect("remembered");
        let now = now_seconds();
        store.saved.devices[0].last_seen = now - FORGET_UNUSED_AFTER - 1;
        assert!(store.forget_unused(now));
        assert!(store.device_of(&old).is_none());
        assert!(store.device_of(&recent).is_some());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_guard_remembers_a_bounded_number_of_addresses() {
        let mut guard = Guard::default();
        let now = Instant::now();
        for n in 0..(MOST_ADDRESSES as u32 + 10) {
            guard.wrong(IpAddr::from(n.to_be_bytes()), now + Duration::from_millis(u64::from(n)));
        }
        assert_eq!(guard.tries.len(), MOST_ADDRESSES);
    }

    #[test]
    fn a_device_is_named_by_its_browser_and_system_only() {
        let ipad = "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
        assert_eq!(device_name(ipad), "Safari on an iPad");
        let chrome = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";
        assert_eq!(device_name(chrome), "Chrome on Windows");
        let edge =
            "Mozilla/5.0 (Windows NT 10.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36 Edg/120.0";
        assert_eq!(device_name(edge), "Edge on Windows");
        assert_eq!(device_name("<script>alert(1)</script>"), "A browser");
    }

    #[test]
    fn random_words_are_url_safe_and_differ() {
        let a = random_word(32).expect("random");
        let b = random_word(32).expect("random");
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_eq!(base64url(b"hi"), "aGk");
        assert_eq!(base64url(b"any carnal pleas"), "YW55IGNhcm5hbCBwbGVhcw");
    }
}
