//! Keeping the control socket's folder to the user the app runs as.
//!
//! The socket (`control`) can drive the whole app, so it sits in a folder nobody else may use.
//! On a Mac and on Linux that is a folder with mode 0700. On Windows it is a folder this user
//! owns, whose access list takes nothing from the folder above and names only this user (and,
//! if they were already named there, the system and the administrators, who can reach any file
//! anyway). A folder made here has that owner and list from the moment it is made; one that
//! was already there is given them, unless its list names somebody else or refuses somebody,
//! which is left as it is. The owner and the list are then read back, and so is the socket
//! file's own list once it is made, because on Windows it is the file's list that decides who
//! may connect. If any of them can't be set or read, or names anybody else, the app does not
//! listen. It is all done with Windows' own security calls (in `sp_winapi`), on the folder
//! held open: what is looked at and changed is the folder itself, never what a link or a
//! junction put in its place points to, and nobody can rename or remove it until the socket
//! has been made in it and checked.
//!
//! Known limits on Windows, all resting on the folder above being the user's own (as it is in
//! the usual place, the user's profile, which this code does not check):
//! - A folder that was already there and not yet private is made private when it is found.
//!   Somebody who opened it before that keeps what they opened it for, as Windows checks a list
//!   when a file is opened, not afterwards. A folder made here has no such moment.
//! - The folder is held only while the socket is set up. Once the app listens, somebody who can
//!   write in the folder above could rename the folder and put another in its place; and the
//!   command line does not check the folder before it sends: it trusts the path.
//! - The socket file's owner is not checked, only its list: the owner is whoever's token made
//!   it (this user, or the administrators for a program with an administrator's full token).
//! - A program run by the same user with fewer rights than usual (low integrity) is not kept
//!   out by anything set here. Windows keeps it out by itself, which a test in `control`
//!   shows on Windows (with a socket marked for such programs, which lets it in, beside).
//!   Windows reads the socket file's access list and label when a program connects, as when
//!   one opens a file, and refuses a program they do not let in; the one exception found is
//!   a full administrator's token (what an ssh login to an administrator's account has),
//!   which is let in whatever they say, as it could read the file anyway. The tests in
//!   `control` therefore ask through `runas` without those rights when they are run from one.

use std::path::Path;

/// A folder that has been kept private, and what is needed to check the socket made in it.
pub struct Private {
    /// The user the app runs as, as Windows writes it (S-1-5-21-…).
    #[cfg(windows)]
    user: String,
    /// The folder, held open so that nobody can rename or remove it while this lasts.
    #[cfg(windows)]
    _held: sp_winapi::HeldFolder,
}

/// Whether a file is a socket: one left by an app that stopped may be removed, anything else
/// is in the way.
#[cfg(unix)]
pub fn is_socket(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::FileTypeExt;
    meta.file_type().is_socket()
}

/// Makes the folder if it isn't there, and lets only this user in.
#[cfg(unix)]
pub fn keep_private(folder: &Path) -> Result<Private, String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    if !folder.exists() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(folder).map_err(|e| {
            crate::i18n::tf(
                "notices.private.not_made",
                "Could not make {folder}: {error}",
                &[("folder", &folder.display().to_string()), ("error", &e.to_string())],
            )
        })?;
    }
    // Only this user may enter the folder. It must be a real folder, not a link to somewhere
    // else; and setting its permissions fails unless this user owns it.
    let meta = std::fs::symlink_metadata(folder).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err(crate::i18n::tf(
            "notices.private.not_folder",
            "{folder} is not a folder.",
            &[("folder", &folder.display().to_string())],
        ));
    }
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700)).map_err(|e| {
        crate::i18n::tf(
            "notices.private.not_private",
            "Could not keep {folder} private: {error}",
            &[("folder", &folder.display().to_string()), ("error", &e.to_string())],
        )
    })?;
    Ok(Private {})
}

#[cfg(unix)]
impl Private {
    /// The socket just made is for this user only, like its folder.
    pub fn check_socket(&self, socket: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600));
        Ok(())
    }

    /// A socket that was already there. On Unix nobody else can have reached it, the folder
    /// being closed to them, so there is nothing to look at and it is not touched.
    pub fn check_existing(&self, _socket: &Path) -> Result<(), String> {
        Ok(())
    }
}

/// Set on a file or folder that is a link, a junction or another stand-in for something else.
#[cfg(windows)]
const REPARSE_POINT: u32 = 0x400;

/// Whether a file is a socket. Windows keeps a socket as a stand-in file that is neither a
/// link nor a folder.
#[cfg(windows)]
pub fn is_socket(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & REPARSE_POINT != 0 && !meta.is_dir() && !meta.file_type().is_symlink()
}

/// Makes the folder if it isn't there, and lets only this user in: this user owns it, its
/// access list takes nothing from the folder above and names this user, and everything made in
/// it gets the same list. The owner and list are read back, and anybody else on the list is a
/// reason not to listen.
#[cfg(windows)]
pub fn keep_private(folder: &Path) -> Result<Private, String> {
    let private = |problem: String| format!("Could not keep {} private: {problem}", folder.display());
    let user = sp_winapi::current_user().map_err(private)?;
    if !a_person(&user) {
        return Err(private(format!("it would be kept to {user}, which is not one person's account")));
    }
    // Made with its owner and list, so there is no moment when it has any other. Something
    // already there by that name (made a moment ago by somebody else, say) is looked at below,
    // as any folder that was already there is.
    if std::fs::symlink_metadata(folder).is_err() {
        let could_not = |problem: String| format!("Could not make {}: {problem}", folder.display());
        if let Some(above) = folder.parent() {
            std::fs::create_dir_all(above).map_err(|e| could_not(e.to_string()))?;
        }
        sp_winapi::make_folder(folder, &private_security(&user, "")).map_err(could_not)?;
    }
    // Held from here on: a real folder, not a link or a junction to somewhere else, and still
    // the same folder when the socket is made in it.
    let held = sp_winapi::HeldFolder::open(folder).map_err(private)?;
    if !held.is_real_folder().map_err(private)? {
        return Err(format!("{} is not a folder.", folder.display()));
    }
    let found = held.security().map_err(private)?;
    if folder_is_private(found.owner.as_deref(), &found.list, &user).is_err() {
        let wanted = private_from(&found.list, &user).map_err(|who| others_in_folder(folder, &who))?;
        held.set_security(&wanted).map_err(private)?;
        let now = held.security().map_err(private)?;
        folder_is_private(now.owner.as_deref(), &now.list, &user).map_err(|who| others_in_folder(folder, &who))?;
    }
    Ok(Private { user, _held: held })
}

#[cfg(windows)]
impl Private {
    /// The socket just made is for this user only: its own access list decides who may
    /// connect, so it is read back too.
    pub fn check_socket(&self, socket: &Path) -> Result<(), String> {
        let found = sp_winapi::security_of(socket)
            .map_err(|problem| format!("Could not keep {} private: {problem}", socket.display()))?;
        only_this_user(&found.list, &self.user, false).map_err(|who| others(socket, &who))
    }

    /// A socket that was already there keeps the access list it had, whatever the folder's is
    /// now. One that lets anybody else in is not asked whether an app answers on it, and is
    /// not replaced: it is in the way.
    pub fn check_existing(&self, socket: &Path) -> Result<(), String> {
        let in_the_way = |why: String| format!("{} is in the way of the control socket: {why}", socket.display());
        let found = sp_winapi::security_of(socket)
            .map_err(|problem| in_the_way(format!("its access list can't be read ({problem}).")))?;
        only_this_user(&found.list, &self.user, false)
            .map_err(|who| in_the_way(format!("it is not private ({who}). Delete it, then start SoftPresenter again.")))
    }
}

/// What is said when somebody else is on an access list.
#[cfg(windows)]
fn others(path: &Path, who: &str) -> String {
    format!("{} is not private ({who}), so SoftPresenter is not listening for commands there.", path.display())
}

/// The same for the folder, with what to do about it: entries a folder already had for other
/// people are not taken off here (they may have been put there on purpose), so the folder has
/// to go.
#[cfg(windows)]
fn others_in_folder(folder: &Path, who: &str) -> String {
    format!(
        "{} Delete that folder and start SoftPresenter again: it makes the folder afresh, for you alone.",
        others(folder, who)
    )
}

/// Whether a SID is one person's account, the only kind of user a folder is kept to: written
/// the way Windows writes a person's account (S-1-5-21-… with four numbers after it, or
/// S-1-12-1-… for an account kept by an organisation's directory in the cloud) or the system's
/// own (S-1-5-18). An account that many programs share (S-1-5-19, a local service) is not one.
#[cfg(any(windows, test))]
fn a_person(sid: &str) -> bool {
    let parts: Vec<&str> = sid.split('-').collect();
    let numbers =
        parts.iter().skip(2).all(|part| (1..=10).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit()));
    let shape = match parts.as_slice() {
        ["S", "1", "5", "18"] => true,
        ["S", "1", "5", "21", rest @ ..] | ["S", "1", "12", "1", rest @ ..] => rest.len() == 4,
        _ => false,
    };
    numbers && shape
}

/// The owner and access list to give a folder, written as Windows writes them down: this user
/// owns it, nothing is taken from the folder above, the entries in `kept` stay, and this user
/// may do everything in it, which passes on to the files and folders made inside.
#[cfg(any(windows, test))]
fn private_security(user: &str, kept: &str) -> String {
    format!("O:{user}D:P{kept}(A;OICI;FA;;;{user})")
}

/// What to give a folder that was already there and isn't private, from the access list it
/// has: what it takes from the folder above goes, its own entries for the system and the
/// administrators stay, and its own entries for this user are replaced by one that gives them
/// the whole of it. An entry of its own for anybody else, or one that refuses somebody, is not
/// taken off (it may have been put there on purpose): the error says who else is let in, or
/// what is wrong, and nothing is changed.
#[cfg(any(windows, test))]
fn private_from(list: &str, user: &str) -> Result<String, String> {
    let (_closed, entries) = entries(list).ok_or_else(|| format!("its access list can't be read: {list}"))?;
    let mut kept = String::new();
    for entry in &entries {
        if entry.passed_on.as_bytes().chunks(2).any(|flag| flag == b"ID") {
            continue;
        }
        if entry.kind != "A" {
            return Err(format!("its access list has an entry of a kind not expected, {}", entry.kind));
        }
        if is_this_user(entry.who, user) {
            continue;
        }
        if !may_also_open(entry.who) {
            return Err(format!("{} may open it too", entry.who));
        }
        kept.push('(');
        kept.push_str(entry.text);
        kept.push(')');
    }
    Ok(private_security(user, &kept))
}

/// Checks that a folder is private: this user owns it (an owner can always change the list, so
/// nobody else may be it), and its access list is what [`only_this_user`] wants of a folder's.
#[cfg(any(windows, test))]
fn folder_is_private(owner: Option<&str>, list: &str, user: &str) -> Result<(), String> {
    match owner {
        None => Err("it has no owner".to_owned()),
        Some(owner) if owner != user => Err(format!("it belongs to {owner}")),
        Some(_) => only_this_user(list, user, true),
    }
}

/// One line of an access list: who, and what it lets them do.
#[cfg(any(windows, test))]
struct Entry<'a> {
    /// `A` lets in; anything else (refusing, auditing, conditions) is not expected here.
    kind: &'a str,
    /// Two letters each: `OI` and `CI` pass the entry on to files and folders made inside, and
    /// `ID` marks one taken on from the folder above.
    passed_on: &'a str,
    rights: &'a str,
    who: &'a str,
    /// The whole line as written, without its brackets.
    text: &'a str,
}

/// The entries of an access list written the way Windows writes one down
/// (`D:PAI(A;OICI;FA;;;S-1-…)`), and whether the list is closed to what the folder above
/// would add (`P`). `None` if it can't be read with certainty.
#[cfg(any(windows, test))]
fn entries(list: &str) -> Option<(bool, Vec<Entry<'_>>)> {
    let list = &list[list.find("D:")? + 2..];
    let first = list.find('(').unwrap_or(list.len());
    let (marks, mut rest) = list.split_at(first);
    if !marks.chars().all(|c| c.is_ascii_uppercase()) || marks.contains("NO") {
        // NO_ACCESS_CONTROL: a list that lets everybody in.
        return None;
    }
    let mut found = Vec::new();
    while let Some(inside) = rest.strip_prefix('(') {
        let end = inside.find(')')?;
        let text = &inside[..end];
        let parts: Vec<&str> = text.split(';').collect();
        if parts.len() < 6 || text.contains('(') {
            return None;
        }
        found.push(Entry { kind: parts[0], passed_on: parts[1], rights: parts[2], who: parts[5], text });
        rest = &inside[end + 1..];
    }
    // Nothing may follow but the system's own labels (S:…).
    (rest.is_empty() || rest.starts_with("S:")).then_some((marks.contains('P'), found))
}

/// Whether the one an entry is for is this user. Windows writes the first administrator
/// account of a computer as LA, and the system's own account (which a service may run as) as
/// SY.
#[cfg(any(windows, test))]
fn is_this_user(who: &str, user: &str) -> bool {
    who == user
        || (who == "LA" && user.starts_with("S-1-5-21-") && user.ends_with("-500"))
        || (who == "SY" && user == "S-1-5-18")
}

/// The system and the administrators, who may be on a list beside this user: they can reach
/// any file anyway.
#[cfg(any(windows, test))]
fn may_also_open(who: &str) -> bool {
    matches!(who, "SY" | "BA" | "S-1-5-18" | "S-1-5-32-544")
}

/// Checks that an access list lets in this user and nobody but this user, the system and the
/// administrators. A folder's list must also be closed to what the folder above would add, and
/// must pass this user's entry on to the files made in it. The error names who else is let in,
/// or what is wrong.
#[cfg(any(windows, test))]
fn only_this_user(list: &str, user: &str, folder: bool) -> Result<(), String> {
    let (closed, entries) = entries(list).ok_or_else(|| format!("its access list can't be read: {list}"))?;
    if folder && !closed {
        return Err("it takes on the access list of the folder above".to_owned());
    }
    let mut lets_user_in = false;
    for entry in &entries {
        if entry.kind != "A" {
            return Err(format!("its access list has an entry of a kind not expected, {}", entry.kind));
        }
        if is_this_user(entry.who, user) {
            let passed_on: Vec<&[u8]> = entry.passed_on.as_bytes().chunks(2).collect();
            let to_files = passed_on.contains(&b"OI".as_slice());
            let all = entry.rights == "FA" || entry.rights.eq_ignore_ascii_case("0x1f01ff");
            lets_user_in |= all && (to_files || !folder);
        } else if !may_also_open(entry.who) {
            return Err(format!("{} may open it too", entry.who));
        }
    }
    if !lets_user_in {
        return Err("its access list doesn't give this user the whole of it".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "S-1-5-21-1111-2222-3333-1001";

    #[test]
    fn only_a_persons_account_is_a_user_to_keep_a_folder_to() {
        for person in [USER, "S-1-5-18", "S-1-12-1-11-22-33-44", "S-1-5-21-1-2-3-500"] {
            assert!(a_person(person), "{person}");
        }
        for not_a_person in [
            "",
            "nothing of the kind",
            "S-1-",
            // A group everybody is in, the administrators, an account services share, a list's
            // short names.
            "S-1-1-0",
            "S-1-5-32-544",
            "S-1-5-19",
            "WD",
            "LA",
            // Too few or too many numbers, or not numbers.
            "S-1-5-21-1111-2222-3333",
            "S-1-5-21-1111-2222-3333-1001-5",
            "S-1-5-21-1111--3333-1001",
            "S-1-5-21-1111-2222-3333-12345678901",
            "S-1-5-21-1111-2222-3333-1001 ",
            // Anything that would add to a written access list.
            "S-1-5-21-1111-2222-3333-1001)(A;;FA;;;WD",
            "S-1-5-21-1111-2222-3333-1001:(OI)(CI)F",
        ] {
            assert!(!a_person(not_a_person), "{not_a_person}");
        }
    }

    #[test]
    fn a_folder_only_this_user_may_open_is_private() {
        let list = format!("D:PAI(A;OICI;FA;;;{USER})");
        assert_eq!(only_this_user(&list, USER, true), Ok(()));
        // The system and the administrators can reach any file anyway.
        let list = format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{USER})S:(ML;;NW;;;ME)");
        assert_eq!(only_this_user(&list, USER, true), Ok(()));
        // A computer's first administrator account is written LA.
        assert_eq!(only_this_user("D:P(A;OICI;FA;;;LA)", "S-1-5-21-1-2-3-500", true), Ok(()));
        assert!(only_this_user("D:P(A;OICI;FA;;;LA)", USER, true).is_err());
    }

    #[test]
    fn a_folder_anybody_else_may_open_is_not_private() {
        for (list, why) in [
            (format!("D:PAI(A;OICI;FA;;;{USER})(A;OICI;FA;;;WD)"), "WD may open it too"),
            (format!("D:P(A;OICI;FA;;;{USER})(A;;0x1200a9;;;BU)"), "BU may open it too"),
            (format!("D:P(A;OICI;FA;;;{USER})(A;OICI;FA;;;S-1-5-21-1111-2222-3333-1002)"), "1002 may open it too"),
            (format!("D:AI(A;OICIID;FA;;;{USER})"), "the folder above"),
            (format!("D:P(A;;FA;;;{USER})"), "doesn't give this user"),
            (format!("D:P(A;OICI;0x1200a9;;;{USER})"), "doesn't give this user"),
            (format!("D:P(D;OICI;FA;;;WD)(A;OICI;FA;;;{USER})"), "not expected, D"),
            (format!("D:P(XA;OICI;FA;;;{USER};(@User.x == 1))(A;OICI;FA;;;WD)"), "can't be read"),
            ("D:NO_ACCESS_CONTROL".to_owned(), "can't be read"),
            ("D:P".to_owned(), "doesn't give this user"),
            ("".to_owned(), "can't be read"),
            (format!("D:P(A;OICI;FA;;;{USER})rubbish"), "can't be read"),
            ("nothing".to_owned(), "can't be read"),
        ] {
            let said = only_this_user(&list, USER, true).expect_err(&list);
            assert!(said.contains(why), "{list}: {said}");
        }
    }

    #[test]
    fn a_socket_takes_its_access_list_from_its_folder() {
        // Not closed (it comes from the folder) and not passed on: right for a file.
        let list = format!("D:AI(A;ID;FA;;;{USER})");
        assert_eq!(only_this_user(&list, USER, false), Ok(()));
        let list = format!("D:AI(A;ID;FA;;;{USER})(A;ID;0x1200a9;;;WD)");
        assert!(only_this_user(&list, USER, false).is_err());
    }

    /// The owner can always change the list, so a folder must belong to this user too.
    #[test]
    fn a_private_folder_belongs_to_this_user() {
        let list = format!("D:PAI(A;OICI;FA;;;{USER})");
        assert_eq!(folder_is_private(Some(USER), &list, USER), Ok(()));
        for (owner, why) in [
            (None, "it has no owner"),
            (Some("S-1-5-21-1111-2222-3333-1002"), "it belongs to S-1-5-21-1111-2222-3333-1002"),
            (Some("S-1-5-32-544"), "it belongs to S-1-5-32-544"),
            (Some("S-1-1-0"), "it belongs to S-1-1-0"),
        ] {
            assert_eq!(folder_is_private(owner, &list, USER), Err(why.to_owned()));
        }
        // And belonging to this user does not make a list that lets others in private.
        let open = format!("D:P(A;OICI;FA;;;{USER})(A;OICI;FA;;;WD)");
        assert!(folder_is_private(Some(USER), &open, USER).is_err());
    }

    /// What a folder is made with is private as it stands.
    #[test]
    fn a_folder_is_made_private() {
        let made = private_security(USER, "");
        assert_eq!(made, format!("O:{USER}D:P(A;OICI;FA;;;{USER})"));
        assert_eq!(folder_is_private(Some(USER), &made, USER), Ok(()));
    }

    /// Windows makes the folder with its owner and list, rather than with what the folder above
    /// would give it (which `keep_private` would then set right, so a test of that alone could
    /// not tell).
    #[cfg(windows)]
    #[test]
    fn windows_makes_a_folder_private_from_the_start() {
        let user = sp_winapi::current_user().expect("which user this is");
        let folder = std::env::temp_dir().join(format!("sp-made-private-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(sp_winapi::make_folder(&folder, &private_security(&user, "")), Ok(true), "made");
        let made = sp_winapi::security_of(&folder).expect("its owner and access list");
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(made.owner.as_deref(), Some(user.as_str()), "it belongs to this user");
        assert!(made.list.starts_with("D:P"), "closed to the folder above: {}", made.list);
        let (_, entries) = entries(&made.list).expect("a list that can be read");
        assert!(
            entries.iter().all(|entry| !entry.passed_on.as_bytes().chunks(2).any(|flag| flag == b"ID")),
            "nothing taken from the folder above: {}",
            made.list
        );
        assert_eq!(folder_is_private(made.owner.as_deref(), &made.list, &user), Ok(()), "{}", made.list);
        // Asked to make it again, it says it is there already and leaves it.
        std::fs::create_dir_all(&folder).expect("a folder in the way");
        assert_eq!(sp_winapi::make_folder(&folder, &private_security(&user, "")), Ok(false));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_folder_already_there_is_given_only_this_user() {
        for (list, wanted) in [
            // What a folder made the ordinary way takes from the folder above goes.
            (
                format!("D:AI(A;OICIID;FA;;;SY)(A;OICIID;FA;;;BA)(A;OICIID;FA;;;{USER})(A;OICIIOID;GA;;;CO)"),
                format!("O:{USER}D:P(A;OICI;FA;;;{USER})"),
            ),
            // Even when what it takes lets others in: it comes from the folder above, which is
            // this user's (see the module's limits).
            (format!("D:AI(A;OICIID;FA;;;WD)(A;OICIID;FA;;;{USER})"), format!("O:{USER}D:P(A;OICI;FA;;;{USER})")),
            // The system's and the administrators' own entries stay; this user's are replaced.
            (
                format!("D:(A;OICI;FA;;;SY)(A;;0x1200a9;;;{USER})(A;OICI;FA;;;S-1-5-32-544)(A;OICIID;FA;;;BA)"),
                format!("O:{USER}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;S-1-5-32-544)(A;OICI;FA;;;{USER})"),
            ),
            // A list with nothing in it, or already closed.
            ("D:".to_owned(), format!("O:{USER}D:P(A;OICI;FA;;;{USER})")),
            (format!("D:P(A;;FA;;;{USER})"), format!("O:{USER}D:P(A;OICI;FA;;;{USER})")),
        ] {
            let given = private_from(&list, USER).expect(&list);
            assert_eq!(given, wanted, "{list}");
            assert_eq!(folder_is_private(Some(USER), &given, USER), Ok(()), "{given}");
        }
        // A computer's first administrator account, written LA, is the user's own entry.
        let admin = "S-1-5-21-1-2-3-500";
        let given = private_from("D:AI(A;OICI;FA;;;LA)(A;OICIID;FA;;;SY)", admin).expect("given");
        assert_eq!(given, format!("O:{admin}D:P(A;OICI;FA;;;{admin})"));
    }

    /// Entries a folder has of its own for anybody else, or that refuse somebody, are not taken
    /// off: the folder is refused and nothing is set.
    #[test]
    fn a_folder_already_there_that_others_may_open_is_refused() {
        for (list, why) in [
            (format!("D:AI(A;OICI;FA;;;WD)(A;OICIID;FA;;;{USER})"), "WD may open it too"),
            (format!("D:(A;OICI;FA;;;{USER})(A;;0x1200a9;;;S-1-5-21-1111-2222-3333-1002)"), "1002 may open it too"),
            (format!("D:(A;OICIIO;GA;;;CO)(A;OICI;FA;;;{USER})"), "CO may open it too"),
            (format!("D:(D;OICI;FA;;;WD)(A;OICI;FA;;;{USER})"), "not expected, D"),
            (format!("D:(D;;FA;;;{USER})(A;OICI;FA;;;{USER})"), "not expected, D"),
            (format!("D:(OA;;FA;00000000-0000-0000-0000-000000000000;;{USER})"), "not expected, OA"),
            (format!("D:(XA;OICI;FA;;;{USER};(@User.x == 1))"), "can't be read"),
            // No list, or an empty one, lets everybody in: somebody may have done anything there.
            ("D:NO_ACCESS_CONTROL".to_owned(), "can't be read"),
            ("".to_owned(), "can't be read"),
            ("nothing".to_owned(), "can't be read"),
            (format!("D:(A;OICI;FA;;;{USER})rubbish"), "can't be read"),
        ] {
            let said = private_from(&list, USER).expect_err(&list);
            assert!(said.contains(why), "{list}: {said}");
        }
    }
}
