//! Where the app keeps keys and tokens: the one store for anything that must never sit in a
//! plain file or the log (VoxLive's API key, Canva Sync's sign-in, a device token).
//!
//! - **A Mac:** the Keychain, one generic password for each service and account.
//! - **Windows:** the signed-in user's Credential Manager (through `sp_winapi`), which Windows
//!   encrypts with the user's sign-in. One entry holds 2,560 bytes, so a longer secret is laid
//!   over several entries (`split`).
//! - **Linux:** the Secret Service (GNOME Keyring, KWallet), reached through libsecret's
//!   `secret-tool` command, with the secret handed to it on its standard input, never as an
//!   argument another program could list. Where there is no `secret-tool` or no keyring
//!   answers, keeping a secret is refused in plain words: it is never written to a plain file
//!   instead.
//! - **A test copy only** (one started with `SOFTPRESENTER_DATA`): a file of its own in its own
//!   data folder, which only its user may read (mode 600 on a Mac and on Linux). A test copy
//!   never reads, changes or asks for anything in the owner's store. This is for trying the app
//!   and for tests, not a place a church's keys are ever kept.
//!
//! Nothing here logs or prints a secret, and no error carries one: an error is a sentence that
//! says what could not be done.

use std::path::{Path, PathBuf};

use crate::i18n::{t, tf};

/// The longest secret kept, in bytes: far more than a key or a sign-in is, and small enough for
/// every system's store.
const LONGEST: usize = 32 * 1024;

/// What is kept under `service` and `account`, or `None` when nothing is. The error is a
/// sentence.
pub fn get(service: &str, account: &str) -> Result<Option<String>, String> {
    Store::for_app().get(service, account)
}

/// Keeps `secret` under `service` and `account`, in place of what was there. The error is a
/// sentence.
pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
    Store::for_app().set(service, account, secret)
}

/// Forgets what is kept under `service` and `account`; nothing kept there is no error.
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    Store::for_app().delete(service, account)
}

/// A place secrets are kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Store {
    /// The system's own: the Keychain, Credential Manager, or the Secret Service.
    System,
    /// A test copy's own files in this folder. Only for a test copy and for tests.
    TestCopy(PathBuf),
}

impl Store {
    /// The app's: the system's store, or, for a test copy (one started with `SOFTPRESENTER_DATA`
    /// naming its data folder), files in that folder, so that it never touches the owner's.
    pub fn for_app() -> Store {
        match std::env::var_os("SOFTPRESENTER_DATA").filter(|folder| !folder.is_empty()) {
            Some(folder) => Store::TestCopy(PathBuf::from(folder)),
            None => Store::System,
        }
    }

    /// What is kept under `service` and `account`, or `None` when nothing is.
    pub fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        named(service, account)?;
        match self {
            Store::System => system::get(service, account),
            Store::TestCopy(folder) => test_copy::get(folder, service, account),
        }
    }

    /// Keeps `secret` under `service` and `account`, in place of what was there.
    pub fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
        named(service, account)?;
        if secret.is_empty() || secret.contains('\0') || secret.len() > LONGEST {
            return Err(
                t("integrations.kept.not_a_secret", "That can't be kept: it is empty or far too long.").to_owned()
            );
        }
        match self {
            Store::System => system::set(service, account, secret),
            Store::TestCopy(folder) => test_copy::set(folder, service, account, secret),
        }
    }

    /// Forgets what is kept under `service` and `account`; nothing kept there is no error.
    pub fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        named(service, account)?;
        match self {
            Store::System => system::delete(service, account),
            Store::TestCopy(folder) => test_copy::delete(folder, service, account),
        }
    }
}

/// Refuses a service or account that could not name an entry in every store: empty, very long,
/// with a character that isn't printed, or starting with a dash (which `secret-tool` would take
/// for one of its options). The names are the app's own, so this is a mistake in the code.
fn named(service: &str, account: &str) -> Result<(), String> {
    let fine = |name: &str| {
        !name.is_empty() && name.len() <= 100 && !name.starts_with('-') && !name.chars().any(char::is_control)
    };
    if fine(service) && fine(account) {
        Ok(())
    } else {
        Err("That is not a name a secret can be kept under.".to_owned())
    }
}

/// What is said when what a store holds can't be read as text.
fn damaged() -> String {
    t("integrations.kept.damaged", "What was kept is damaged; connect again.").to_owned()
}

/// A test copy's secrets: one file each in its data folder, for its user only. Never used for
/// the owner's app.
mod test_copy {
    use std::io::Write;

    use super::*;

    /// The file a secret is in. The two kept before this module keep their names; any other is
    /// named from its service and account, letters and digits only.
    pub fn file_name(service: &str, account: &str) -> String {
        match (service, account) {
            ("SoftPresenter VoxLive", "church API key") => "voxlive-test-key".to_owned(),
            ("SoftPresenter Canva Sync", "church sign-in") => "canva-test-sign-in.json".to_owned(),
            _ => {
                let mut name = String::new();
                for letter in format!("{service} {account}").chars().flat_map(char::to_lowercase) {
                    if letter.is_alphanumeric() {
                        name.push(letter);
                    } else if !name.ends_with('-') {
                        name.push('-');
                    }
                }
                format!("{}.test-secret", name.trim_matches('-'))
            }
        }
    }

    pub fn get(folder: &Path, service: &str, account: &str) -> Result<Option<String>, String> {
        match std::fs::read(folder.join(file_name(service, account))) {
            Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| damaged()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(tf(
                "integrations.kept.test.unreadable",
                "What this test copy kept could not be read: {error}",
                &[("error", &e.to_string())],
            )),
        }
    }

    pub fn set(folder: &Path, service: &str, account: &str, secret: &str) -> Result<(), String> {
        let file = folder.join(file_name(service, account));
        let partial = folder.join(format!("{}.part", file_name(service, account)));
        let _ = std::fs::create_dir_all(folder);
        write_private(&partial, secret.as_bytes()).and_then(|()| std::fs::rename(&partial, &file)).map_err(|e| {
            tf(
                "integrations.kept.test.not_kept",
                "It could not be kept in this test copy's folder: {error}",
                &[("error", &e.to_string())],
            )
        })
    }

    pub fn delete(folder: &Path, service: &str, account: &str) -> Result<(), String> {
        match std::fs::remove_file(folder.join(file_name(service, account))) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(tf(
                "integrations.kept.test.not_removed",
                "What this test copy kept could not be removed: {error}",
                &[("error", &e.to_string())],
            )),
            _ => Ok(()),
        }
    }

    /// Writes a new file only its owner may read. Anything left at that name (a half-written
    /// file, a link) is removed first, and the file is made new, so it is never written through.
    fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }
}

/// The Mac's Keychain: a generic password for each service and account.
#[cfg(target_os = "macos")]
mod system {
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

    use super::*;

    /// The Keychain's answer when there is no such item (`errSecItemNotFound`).
    const NOT_FOUND: i32 = -25300;

    pub fn get(service: &str, account: &str) -> Result<Option<String>, String> {
        match get_generic_password(service, account) {
            Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| damaged()),
            Err(e) if e.code() == NOT_FOUND => Ok(None),
            Err(e) => Err(tf(
                "integrations.kept.mac.unreadable",
                "The Keychain could not be read: {error}",
                &[("error", &e.to_string())],
            )),
        }
    }

    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        set_generic_password(service, account, secret.as_bytes()).map_err(|e| {
            tf(
                "integrations.kept.mac.not_kept",
                "It could not be kept in the Keychain: {error}",
                &[("error", &e.to_string())],
            )
        })
    }

    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        match delete_generic_password(service, account) {
            Err(e) if e.code() != NOT_FOUND => Err(tf(
                "integrations.kept.mac.not_removed",
                "It could not be removed from the Keychain: {error}",
                &[("error", &e.to_string())],
            )),
            _ => Ok(()),
        }
    }
}

/// Windows' Credential Manager, for the signed-in user.
#[cfg(windows)]
mod system {
    use super::split::{Entries, Failure};
    use super::*;

    /// What Credential Manager shows beside each entry.
    const COMMENT: &str = "Kept by SoftPresenter";

    /// The user's Credential Manager; the user name shown beside an entry is its account.
    struct Manager<'a> {
        account: &'a str,
    }

    impl Entries for Manager<'_> {
        fn room(&self) -> usize {
            sp_winapi::credentials::ROOM
        }
        fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
            sp_winapi::credentials::read(name)
        }
        fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
            sp_winapi::credentials::write(name, self.account, COMMENT, bytes)
        }
        fn remove(&self, name: &str) -> Result<bool, String> {
            sp_winapi::credentials::remove(name)
        }
    }

    pub fn get(service: &str, account: &str) -> Result<Option<String>, String> {
        match split::get(&Manager { account }, &entry_name(service, account)) {
            Ok(Some(bytes)) => String::from_utf8(bytes).map(Some).map_err(|_| damaged()),
            Ok(None) => Ok(None),
            Err(Failure::Damaged) => Err(damaged()),
            Err(Failure::Store(error)) => Err(tf(
                "integrations.kept.windows.unreadable",
                "Windows' Credential Manager could not be read: {error}",
                &[("error", &error)],
            )),
        }
    }

    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        split::set(&Manager { account }, &entry_name(service, account), secret.as_bytes()).map_err(|failure| {
            match failure {
                Failure::Damaged => damaged(),
                Failure::Store(error) => tf(
                    "integrations.kept.windows.not_kept",
                    "It could not be kept in Windows' Credential Manager: {error}",
                    &[("error", &error)],
                ),
            }
        })
    }

    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        split::delete(&Manager { account }, &entry_name(service, account)).map_err(|failure| match failure {
            Failure::Damaged => damaged(),
            Failure::Store(error) => tf(
                "integrations.kept.windows.not_removed",
                "It could not be removed from Windows' Credential Manager: {error}",
                &[("error", &error)],
            ),
        })
    }
}

/// Linux (and any other Unix but a Mac): the Secret Service, through `secret-tool`.
#[cfg(all(unix, not(target_os = "macos")))]
mod system {
    use super::secret_tool::{self, Failure};
    use super::*;

    /// The command, found on the PATH.
    const PROGRAM: &str = "secret-tool";

    /// A failure in words. `refused` makes the sentence for a keyring that answered with an
    /// error (its own words are added).
    fn words(failure: Failure, refused: fn(&str) -> String) -> String {
        match failure {
            Failure::NoTool => t(
                "integrations.kept.linux.no_tool",
                "This computer has no keyring SoftPresenter can use, so nothing was kept. It needs the secret-tool command (the libsecret-tools package on Debian and Ubuntu, libsecret on Fedora and Arch) and a keyring such as GNOME Keyring or KWallet.",
            )
            .to_owned(),
            Failure::Slow => t(
                "integrations.kept.linux.slow",
                "The system keyring took too long to answer; it may be waiting to be unlocked.",
            )
            .to_owned(),
            Failure::Damaged => damaged(),
            Failure::Refused(error) => refused(&error),
        }
    }

    pub fn get(service: &str, account: &str) -> Result<Option<String>, String> {
        secret_tool::get(PROGRAM.as_ref(), service, account, secret_tool::PATIENCE).map_err(|failure| {
            words(failure, |error| {
                tf(
                    "integrations.kept.linux.unreadable",
                    "The system keyring could not be read: {error}",
                    &[("error", error)],
                )
            })
        })
    }

    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        secret_tool::set(PROGRAM.as_ref(), service, account, secret, secret_tool::PATIENCE).map_err(|failure| {
            words(failure, |error| {
                tf(
                    "integrations.kept.linux.not_kept",
                    "It could not be kept in the system keyring: {error}",
                    &[("error", error)],
                )
            })
        })
    }

    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        secret_tool::delete(PROGRAM.as_ref(), service, account, secret_tool::PATIENCE).map_err(|failure| {
            words(failure, |error| {
                tf(
                    "integrations.kept.linux.not_removed",
                    "It could not be removed from the system keyring: {error}",
                    &[("error", error)],
                )
            })
        })
    }
}

/// The name of a secret's entry in Windows' Credential Manager.
#[cfg(any(windows, test))]
fn entry_name(service: &str, account: &str) -> String {
    format!("{service}: {account}")
}

/// A secret laid over the entries of a store whose entries are small (Windows' Credential
/// Manager: 2,560 bytes each).
///
/// The entry under the secret's own name is its head. A head that starts with `0` holds the
/// whole secret after that byte. A head that starts with `1` holds a set's letter (`a` or `b`)
/// and a count, and the secret is that many numbered parts of that set, end to end. A longer
/// secret is replaced by writing the other set's parts first and the head last, so the head
/// always points at a whole secret, whatever stops the app part way; the set no longer pointed
/// at is then removed.
#[cfg(any(windows, test))]
mod split {
    /// A store of small named entries.
    pub trait Entries {
        /// The most bytes one entry holds.
        fn room(&self) -> usize;
        fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String>;
        fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String>;
        /// `Ok(false)` when there was no such entry.
        fn remove(&self, name: &str) -> Result<bool, String>;
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum Failure {
        /// The store's own error, in its words (never the secret).
        Store(String),
        /// What is kept isn't in the form written here, or a part is missing.
        Damaged,
    }

    /// The most parts a secret is laid over; every one of them is looked for when a set is
    /// removed, so that none is left behind.
    const MOST_PARTS: usize = 16;
    const WHOLE: u8 = b'0';
    const IN_PARTS: u8 = b'1';
    const SETS: [char; 2] = ['a', 'b'];

    fn part(name: &str, set: char, place: usize) -> String {
        format!("{name} (part {set}{place})")
    }

    /// The set and count a head in parts points at.
    fn pointer(head: &[u8]) -> Option<(char, usize)> {
        let rest = std::str::from_utf8(head.strip_prefix(&[IN_PARTS])?).ok()?;
        let set = rest.chars().next().filter(|set| SETS.contains(set))?;
        let count: usize = rest[1..].parse().ok()?;
        (1..=MOST_PARTS).contains(&count).then_some((set, count))
    }

    /// Removes a set's parts from `from` on, every place there could be one.
    fn clear(store: &dyn Entries, name: &str, set: char, from: usize) -> Result<(), Failure> {
        for place in from..MOST_PARTS {
            store.remove(&part(name, set, place)).map_err(Failure::Store)?;
        }
        Ok(())
    }

    pub fn get(store: &dyn Entries, name: &str) -> Result<Option<Vec<u8>>, Failure> {
        let Some(head) = store.read(name).map_err(Failure::Store)? else { return Ok(None) };
        if let Some((&WHOLE, secret)) = head.split_first() {
            return Ok(Some(secret.to_vec()));
        }
        let (set, count) = pointer(&head).ok_or(Failure::Damaged)?;
        let mut secret = Vec::new();
        for place in 0..count {
            let bytes = store.read(&part(name, set, place)).map_err(Failure::Store)?.ok_or(Failure::Damaged)?;
            secret.extend_from_slice(&bytes);
        }
        Ok(Some(secret))
    }

    pub fn set(store: &dyn Entries, name: &str, secret: &[u8]) -> Result<(), Failure> {
        let room = store.room();
        if secret.len() < room {
            let mut head = Vec::with_capacity(secret.len() + 1);
            head.push(WHOLE);
            head.extend_from_slice(secret);
            store.write(name, &head).map_err(Failure::Store)?;
            // The secret is kept. Parts of a longer one before it are of no more use; any that
            // can't be removed now are looked for again at the next change.
            for set in SETS {
                let _ = clear(store, name, set, 0);
            }
            return Ok(());
        }
        let count = secret.len().div_ceil(room);
        if count > MOST_PARTS {
            return Err(Failure::Store(format!("it is longer than {} bytes", MOST_PARTS * room)));
        }
        // The set the head doesn't point at now, so what is kept stays whole until the head
        // is changed.
        let pointed = store.read(name).map_err(Failure::Store)?.and_then(|head| pointer(&head)).map(|(set, _)| set);
        let (set, other) = if pointed == Some(SETS[0]) { (SETS[1], SETS[0]) } else { (SETS[0], SETS[1]) };
        for (place, bytes) in secret.chunks(room).enumerate() {
            store.write(&part(name, set, place), bytes).map_err(Failure::Store)?;
        }
        clear(store, name, set, count)?;
        store.write(name, format!("{}{set}{count}", IN_PARTS as char).as_bytes()).map_err(Failure::Store)?;
        let _ = clear(store, name, other, 0);
        Ok(())
    }

    pub fn delete(store: &dyn Entries, name: &str) -> Result<(), Failure> {
        store.remove(name).map_err(Failure::Store)?;
        for set in SETS {
            clear(store, name, set, 0)?;
        }
        Ok(())
    }
}

/// The Secret Service through libsecret's `secret-tool` command. Its arguments are only the
/// entry's label, service and account; the secret goes to it on its standard input and comes
/// back on its standard output, both pipes of this program's own.
#[cfg(all(unix, any(not(target_os = "macos"), test)))]
mod secret_tool {
    use std::ffi::OsStr;
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::sync::mpsc::{Receiver, channel};
    use std::time::{Duration, Instant};

    /// How long the command may take: it may be showing the operator a window that asks for
    /// the keyring's password.
    pub const PATIENCE: Duration = Duration::from_secs(120);

    #[derive(Debug, PartialEq, Eq)]
    pub enum Failure {
        /// There is no such command on this computer.
        NoTool,
        /// It did not finish in time.
        Slow,
        /// What it gave back isn't text.
        Damaged,
        /// It failed, with what it said (never the secret).
        Refused(String),
    }

    /// How a run of the command ended.
    struct Ran {
        ok: bool,
        out: Vec<u8>,
        said: String,
    }

    /// Reads a pipe to its end on a thread of its own.
    fn reader(mut pipe: impl Read + Send + 'static) -> Receiver<Vec<u8>> {
        let (tx, rx) = channel();
        let _ = std::thread::Builder::new().name("secret-tool pipe".to_owned()).spawn(move || {
            let mut all = Vec::new();
            let _ = pipe.read_to_end(&mut all);
            let _ = tx.send(all);
        });
        rx
    }

    /// The first line of what the command said on its standard error, shortened. If the secret
    /// just given is anywhere in it, nothing of it is repeated.
    fn said(error: &[u8], secret: Option<&str>) -> String {
        let text = String::from_utf8_lossy(error);
        if secret.is_some_and(|secret| text.contains(secret)) {
            return String::new();
        }
        text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("").chars().take(200).collect()
    }

    fn run(program: &OsStr, arguments: &[&str], secret: Option<&str>, patience: Duration) -> Result<Ran, Failure> {
        let mut child = Command::new(program)
            .args(arguments)
            .stdin(if secret.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Failure::NoTool,
                _ => Failure::Refused(e.to_string()),
            })?;
        let out = child.stdout.take().map(reader);
        let error = child.stderr.take().map(reader);
        // The secret is shorter than a pipe holds, so this doesn't wait for the command to
        // read; the pipe is closed straight after, which is how the command knows it has it all.
        let handed = match (secret, child.stdin.take()) {
            (Some(secret), Some(mut pipe)) => pipe.write_all(secret.as_bytes()).is_ok(),
            (Some(_), None) => false,
            (None, _) => true,
        };
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < patience => std::thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Failure::Slow);
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Failure::Refused(e.to_string()));
                }
            }
        };
        // The command has ended, so its pipes are at their end, unless something it started
        // still holds one open: that is not waited for.
        let all = |pipe: Option<Receiver<Vec<u8>>>| {
            pipe.and_then(|pipe| pipe.recv_timeout(Duration::from_secs(2)).ok()).unwrap_or_default()
        };
        let (out, error) = (all(out), all(error));
        Ok(Ran { ok: status.success() && handed, out, said: said(&error, secret) })
    }

    /// What a run that failed means. `secret-tool` ends with an error and says nothing when
    /// there is simply no such entry, and says why when the keyring could not be asked.
    fn nothing_there(ran: &Ran) -> Result<(), Failure> {
        if ran.said.is_empty() { Ok(()) } else { Err(Failure::Refused(ran.said.clone())) }
    }

    pub fn get(program: &OsStr, service: &str, account: &str, patience: Duration) -> Result<Option<String>, Failure> {
        let ran = run(program, &["lookup", "service", service, "account", account], None, patience)?;
        if !ran.ok {
            return nothing_there(&ran).map(|()| None);
        }
        String::from_utf8(ran.out).map(Some).map_err(|_| Failure::Damaged)
    }

    pub fn set(program: &OsStr, service: &str, account: &str, secret: &str, patience: Duration) -> Result<(), Failure> {
        let label = format!("--label={service}: {account}");
        let ran = run(program, &["store", &label, "service", service, "account", account], Some(secret), patience)?;
        if ran.ok {
            return Ok(());
        }
        Err(Failure::Refused(if ran.said.is_empty() { "secret-tool did not keep it".to_owned() } else { ran.said }))
    }

    pub fn delete(program: &OsStr, service: &str, account: &str, patience: Duration) -> Result<(), Failure> {
        let ran = run(program, &["clear", "service", service, "account", account], None, patience)?;
        if ran.ok { Ok(()) } else { nothing_there(&ran) }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use super::*;

    /// A folder of a test's own.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("softpresenter-secrets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_test_copy_keeps_each_secret_in_a_file_of_its_own_that_only_its_user_reads() {
        let dir = folder("test-copy");
        let store = Store::TestCopy(dir.clone());
        assert_eq!(store.get("SoftPresenter Test", "device token"), Ok(None));
        store.set("SoftPresenter Test", "device token", "SENTINEL-1").unwrap();
        store.set("SoftPresenter Test", "other", "SENTINEL-2").unwrap();
        assert_eq!(store.get("SoftPresenter Test", "device token"), Ok(Some("SENTINEL-1".to_owned())));
        // Kept again, the new one takes the old one's place, and nothing half-written is left.
        store.set("SoftPresenter Test", "device token", "SENTINEL-3").unwrap();
        assert_eq!(store.get("SoftPresenter Test", "device token"), Ok(Some("SENTINEL-3".to_owned())));
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|file| file.file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        assert_eq!(files, ["softpresenter-test-device-token.test-secret", "softpresenter-test-other.test-secret"]);
        #[cfg(unix)]
        for file in &files {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(file)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{file}");
        }
        // The file is made new, whatever was left at the name it is first written under: a link
        // there is never written through.
        #[cfg(unix)]
        {
            let elsewhere = dir.join("elsewhere");
            std::fs::write(&elsewhere, b"someone else's").unwrap();
            let part = dir.join("softpresenter-test-device-token.test-secret.part");
            std::os::unix::fs::symlink(&elsewhere, &part).unwrap();
            store.set("SoftPresenter Test", "device token", "SENTINEL-7").unwrap();
            assert_eq!(std::fs::read(&elsewhere).unwrap(), b"someone else's");
            assert!(std::fs::symlink_metadata(dir.join(&files[0])).unwrap().is_file());
            assert_eq!(store.get("SoftPresenter Test", "device token"), Ok(Some("SENTINEL-7".to_owned())));
            std::fs::remove_file(elsewhere).unwrap();
        }
        store.delete("SoftPresenter Test", "device token").unwrap();
        assert_eq!(store.get("SoftPresenter Test", "device token"), Ok(None));
        assert_eq!(store.get("SoftPresenter Test", "other"), Ok(Some("SENTINEL-2".to_owned())));
        // Forgetting what isn't there is no error.
        store.delete("SoftPresenter Test", "device token").unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_test_copys_voxlive_and_canva_files_keep_their_names() {
        assert_eq!(test_copy::file_name("SoftPresenter VoxLive", "church API key"), "voxlive-test-key");
        assert_eq!(test_copy::file_name("SoftPresenter Canva Sync", "church sign-in"), "canva-test-sign-in.json");
        assert_eq!(
            test_copy::file_name("SoftPresenter TimelyChurch", "device token"),
            "softpresenter-timelychurch-device-token.test-secret"
        );
        // Nothing in a name can leave the folder.
        assert_eq!(test_copy::file_name("../a/b", "..\\c"), "a-b-c.test-secret");
    }

    #[test]
    fn a_test_copy_that_cant_write_says_so_without_the_secret() {
        let dir = folder("unwritable");
        std::fs::write(dir.join("a-file"), b"not a folder").unwrap();
        let store = Store::TestCopy(dir.join("a-file"));
        let said = store.set("SoftPresenter Test", "device token", "SENTINEL-4").unwrap_err();
        assert!(said.contains("could not be kept"), "{said}");
        assert!(!said.contains("SENTINEL"), "{said}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn names_and_secrets_no_store_could_hold_are_refused_before_any_store_is_asked() {
        let store = Store::TestCopy(folder("refused"));
        for (service, account) in [("", "a"), ("a", ""), ("-a", "b"), ("a", "--b"), ("a\nb", "c"), ("a", "b\0")] {
            assert!(store.get(service, account).is_err(), "{service:?} {account:?}");
            assert!(store.set(service, account, "x").is_err(), "{service:?} {account:?}");
            assert!(store.delete(service, account).is_err(), "{service:?} {account:?}");
        }
        for secret in ["", "a\0b", &"x".repeat(LONGEST + 1)] {
            let said = store.set("SoftPresenter Test", "device token", secret).unwrap_err();
            assert!(said.len() < 200, "the refusal doesn't repeat it");
        }
        assert_eq!(store.get("SoftPresenter Test", "device token"), Ok(None));
    }

    /// Small named entries in memory, as Windows' Credential Manager keeps them, which can be
    /// told to fail from the nth write on.
    struct Small {
        room: usize,
        entries: RefCell<BTreeMap<String, Vec<u8>>>,
        writes_left: RefCell<Option<usize>>,
    }

    impl Small {
        fn new(room: usize) -> Small {
            Small { room, entries: RefCell::default(), writes_left: RefCell::new(None) }
        }
        fn names(&self) -> Vec<String> {
            self.entries.borrow().keys().cloned().collect()
        }
    }

    impl split::Entries for Small {
        fn room(&self) -> usize {
            self.room
        }
        fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.entries.borrow().get(name).cloned())
        }
        fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
            assert!(bytes.len() <= self.room, "an entry is never given more than it holds");
            if let Some(left) = self.writes_left.borrow_mut().as_mut() {
                if *left == 0 {
                    return Err("the store is full".to_owned());
                }
                *left -= 1;
            }
            self.entries.borrow_mut().insert(name.to_owned(), bytes.to_vec());
            Ok(())
        }
        fn remove(&self, name: &str) -> Result<bool, String> {
            Ok(self.entries.borrow_mut().remove(name).is_some())
        }
    }

    #[test]
    fn a_secret_longer_than_an_entry_is_laid_over_several_and_read_back_whole() {
        let store = Small::new(10);
        let name = entry_name("SoftPresenter Test", "sign-in");
        assert_eq!(name, "SoftPresenter Test: sign-in");
        assert_eq!(split::get(&store, &name), Ok(None));
        // One that fits is one entry.
        split::set(&store, &name, b"short").unwrap();
        assert_eq!(store.names(), [name.as_str()]);
        assert_eq!(split::get(&store, &name), Ok(Some(b"short".to_vec())));
        // A longer one is parts, with the entry of its own name pointing at them.
        let long = b"0123456789abcdefghijKLMNO";
        split::set(&store, &name, long).unwrap();
        assert_eq!(store.names().len(), 4, "{:?}", store.names());
        assert_eq!(split::get(&store, &name), Ok(Some(long.to_vec())));
        // Replaced by another long one, the old parts are gone.
        let longer = b"zyxwvutsrqponmlkjihgfedcba0123456789";
        split::set(&store, &name, longer).unwrap();
        assert_eq!(store.names().len(), 5, "{:?}", store.names());
        assert_eq!(split::get(&store, &name), Ok(Some(longer.to_vec())));
        // Exactly an entry's room does not fit beside the head's first byte.
        split::set(&store, &name, b"0123456789").unwrap();
        assert_eq!(store.names().len(), 2);
        assert_eq!(split::get(&store, &name), Ok(Some(b"0123456789".to_vec())));
        // A short one again leaves no parts, and forgetting leaves nothing.
        split::set(&store, &name, b"short").unwrap();
        assert_eq!(store.names(), [name.as_str()]);
        split::set(&store, &name, long).unwrap();
        split::delete(&store, &name).unwrap();
        assert!(store.names().is_empty(), "{:?}", store.names());
        split::delete(&store, &name).unwrap();
    }

    #[test]
    fn a_long_secret_whose_writing_stops_part_way_leaves_the_one_before_whole() {
        let name = "Test: sign-in";
        let first = b"first-first-first-first-first";
        let second = b"SECOND-SECOND-SECOND-SECOND-SECOND-SECOND";
        // Stopped after each number of writes a replacement makes, up to the head's.
        for writes in 0..6 {
            let store = Small::new(10);
            split::set(&store, name, first).unwrap();
            *store.writes_left.borrow_mut() = Some(writes);
            let failed = split::set(&store, name, second).unwrap_err();
            assert_eq!(failed, split::Failure::Store("the store is full".to_owned()));
            assert_eq!(split::get(&store, name), Ok(Some(first.to_vec())), "after {writes} writes");
            // Tried again with the store working, it is kept, and nothing of either is left over.
            *store.writes_left.borrow_mut() = None;
            split::set(&store, name, second).unwrap();
            assert_eq!(split::get(&store, name), Ok(Some(second.to_vec())));
            assert_eq!(store.names().len(), 1 + second.len().div_ceil(10), "{:?}", store.names());
        }
    }

    #[test]
    fn entries_not_written_here_or_missing_a_part_are_damaged_not_guessed_at() {
        use split::Entries;
        let store = Small::new(10);
        let name = "Test: sign-in";
        for head in [&b""[..], b"2abc", b"1c2", b"1a0", b"1a99", b"1a"] {
            store.write(name, head).unwrap();
            assert_eq!(split::get(&store, name), Err(split::Failure::Damaged), "{head:?}");
        }
        split::set(&store, name, b"0123456789abcdefghij").unwrap();
        let parts: Vec<String> = store.names().into_iter().filter(|entry| entry != name).collect();
        store.remove(&parts[1]).unwrap();
        assert_eq!(split::get(&store, name), Err(split::Failure::Damaged));
        // Far too long for the parts there may be is refused, and nothing is written.
        let store = Small::new(10);
        assert!(split::set(&store, name, &[b'x'; 161]).is_err());
        assert!(store.names().is_empty());
    }

    /// A stand-in for `secret-tool`, written for the test: it keeps what it is given on its
    /// standard input in a file named from its arguments, and writes down the arguments it was
    /// started with, as `ps` would show them.
    #[cfg(unix)]
    fn stand_in(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let program = dir.join("secret-tool");
        let script = r#"#!/bin/sh
here=$(dirname "$0")
printf '%s\n' "$*" >> "$here/arguments"
action=$1; shift
if [ "$action" = store ]; then shift; fi
file="$here/kept-$(printf '%s' "$*" | tr -c 'A-Za-z0-9' '_')"
case "$4" in
  *broken*) echo "secret-tool: The name org.freedesktop.secrets was not provided by any .service files" >&2; exit 1;;
  *slow*) sleep 30;;
esac
case "$action" in
  store) cat > "$file";;
  lookup) [ -f "$file" ] || exit 1; cat "$file";;
  clear) [ -f "$file" ] || exit 1; rm "$file";;
  *) exit 2;;
esac
"#;
        std::fs::write(&program, script).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        program
    }

    #[cfg(unix)]
    #[test]
    fn secret_tool_is_handed_the_secret_on_its_input_and_never_as_an_argument() {
        use secret_tool::PATIENCE;
        let dir = folder("secret-tool");
        let program = stand_in(&dir);
        let program = program.as_os_str();
        assert_eq!(secret_tool::get(program, "SoftPresenter Test", "token", PATIENCE), Ok(None));
        // Forgetting what isn't there is no error.
        assert_eq!(secret_tool::delete(program, "SoftPresenter Test", "token", PATIENCE), Ok(()));
        let secret = "SENTINEL-5 with spaces\nand a second line, and no line end";
        secret_tool::set(program, "SoftPresenter Test", "token", secret, PATIENCE).unwrap();
        assert_eq!(secret_tool::get(program, "SoftPresenter Test", "token", PATIENCE), Ok(Some(secret.to_owned())));
        assert_eq!(secret_tool::get(program, "SoftPresenter Test", "another", PATIENCE), Ok(None));
        secret_tool::delete(program, "SoftPresenter Test", "token", PATIENCE).unwrap();
        assert_eq!(secret_tool::get(program, "SoftPresenter Test", "token", PATIENCE), Ok(None));
        let arguments = std::fs::read_to_string(dir.join("arguments")).unwrap();
        assert!(!arguments.contains("SENTINEL"), "{arguments}");
        assert!(
            arguments.contains("store --label=SoftPresenter Test: token service SoftPresenter Test account token"),
            "{arguments}"
        );
        assert!(arguments.contains("lookup service SoftPresenter Test account token"), "{arguments}");
        assert!(arguments.contains("clear service SoftPresenter Test account token"), "{arguments}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn no_secret_tool_or_no_keyring_is_a_refusal_and_never_a_file() {
        use secret_tool::{Failure, PATIENCE};
        let dir = folder("no-keyring");
        // No such command on this computer.
        let missing = dir.join("no-such-secret-tool");
        let missing = missing.as_os_str();
        assert_eq!(
            secret_tool::set(missing, "SoftPresenter Test", "token", "SENTINEL-6", PATIENCE),
            Err(Failure::NoTool)
        );
        assert_eq!(secret_tool::get(missing, "SoftPresenter Test", "token", PATIENCE), Err(Failure::NoTool));
        assert_eq!(secret_tool::delete(missing, "SoftPresenter Test", "token", PATIENCE), Err(Failure::NoTool));
        // The command is there, but no keyring answers it: what it says is passed on.
        let program = stand_in(&dir);
        let program = program.as_os_str();
        for failed in [
            secret_tool::set(program, "SoftPresenter Test", "broken", "SENTINEL-6", PATIENCE).unwrap_err(),
            secret_tool::get(program, "SoftPresenter Test", "broken", PATIENCE).unwrap_err(),
            secret_tool::delete(program, "SoftPresenter Test", "broken", PATIENCE).unwrap_err(),
        ] {
            let Failure::Refused(said) = failed else { panic!("{failed:?}") };
            assert!(said.contains("org.freedesktop.secrets"), "{said}");
            assert!(!said.contains("SENTINEL"), "{said}");
        }
        // One that doesn't answer in time is given up on.
        let short = std::time::Duration::from_millis(300);
        assert_eq!(secret_tool::get(program, "SoftPresenter Test", "slow", short), Err(Failure::Slow));
        // Nothing of the secret was written anywhere in the folder.
        for file in std::fs::read_dir(&dir).unwrap().flatten() {
            let text = std::fs::read(file.path()).unwrap();
            assert!(!String::from_utf8_lossy(&text).contains("SENTINEL"), "{}", file.path().display());
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The round trip through this system's real store (the Keychain, Credential Manager, or
    /// the Secret Service), under a name of the test's own, which it removes again. It needs a
    /// signed-in desktop session, so it is run by hand, the same way on every system:
    /// `cargo test -p sp-app secrets::tests::the_systems_store_keeps_reads_and_forgets -- --ignored`.
    #[test]
    #[ignore = "needs the system's secret store, in a signed-in session"]
    fn the_systems_store_keeps_reads_and_forgets() {
        let store = Store::System;
        let (service, account) = ("SoftPresenter test (safe to delete)", "round trip");
        let other = "round trip, never kept";
        // Left from a run that stopped part way.
        store.delete(service, account).unwrap();
        assert_eq!(store.get(service, account), Ok(None), "nothing kept reads as nothing");
        assert_eq!(store.get(service, other), Ok(None));
        store.delete(service, other).unwrap();
        // A short one, as a key is; then a long one, as Canva's sign-in is, longer than one of
        // Windows' entries; then a short one again.
        let long: String = (0..7000).map(|n| char::from(b'a' + (n % 26) as u8)).collect();
        for secret in ["a made-up key: clé, ключ, 鍵", &long, "short again\nwith a second line"] {
            store.set(service, account, secret).unwrap();
            assert!(store.get(service, account).as_ref() == Ok(&Some(secret.to_owned())), "not read back as kept");
            assert_eq!(store.get(service, other), Ok(None));
        }
        store.delete(service, account).unwrap();
        assert_eq!(store.get(service, account), Ok(None), "forgotten");
    }
}
