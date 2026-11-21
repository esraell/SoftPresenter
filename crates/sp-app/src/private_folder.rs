//! Keeping the control socket's folder to the user the app runs as.
//!
//! The socket (`control`) can drive the whole app, so it sits in a folder nobody else may use.
//! On a Mac and on Linux that is a folder with mode 0700. On Windows it is a folder whose
//! access list names only this user (and, if they were already named there, the system and the
//! administrators, who can reach any file anyway): the list is set, then read back, and so is
//! the socket file's own list once it is made, because on Windows it is the file's list that
//! decides who may connect. If either can't be set or read, or names anybody else, the app does
//! not listen.
//!
//! Known limits on Windows, all resting on the folder above being the user's own (as it is in
//! the usual place, the user's profile, which this code does not check):
//! - The folder is made first and its list set a moment later, so somebody who could already
//!   write in the folder above might get in between, or put another folder in its place after
//!   it was looked at. The folder is not held open against that.
//! - The owner is set, and the tool's word is taken that it was; it is not read back.
//! - The command line does not check the folder before it sends: it trusts the path.
//! - A program run by the same user with fewer rights than usual (low integrity) is not kept
//!   out by anything set here. Windows keeps it out by itself, which a test in `control`
//!   shows on Windows (with a socket marked for such programs, which lets it in, beside).
//!
//! Windows offers no way to do this without `unsafe` code except through its own tools, so the
//! lists are set and read with `icacls.exe` and the user is found with `whoami.exe`, both taken
//! from the system's own folder.

use std::path::Path;

/// A folder that has been kept private, and what is needed to check the socket made in it.
pub struct Private {
    /// The user the app runs as, as Windows writes it (S-1-5-21-…).
    #[cfg(windows)]
    user: String,
    /// The folder itself: once private, the place for the notes access lists are read through.
    #[cfg(windows)]
    folder: std::path::PathBuf,
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
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)
            .map_err(|e| format!("Could not make {}: {e}", folder.display()))?;
    }
    // Only this user may enter the folder. It must be a real folder, not a link to somewhere
    // else; and setting its permissions fails unless this user owns it.
    let meta = std::fs::symlink_metadata(folder).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err(format!("{} is not a folder.", folder.display()));
    }
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Could not keep {} private: {e}", folder.display()))?;
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

/// A real folder: not a link or a junction to somewhere else.
#[cfg(windows)]
fn real_folder(folder: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(folder).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_attributes() & REPARSE_POINT != 0 {
        return Err(format!("{} is not a folder.", folder.display()));
    }
    Ok(())
}

/// One of Windows' own tools, from its own folder, run without a console window of its own.
#[cfg(windows)]
pub(crate) fn tool(name: &str) -> Result<std::process::Command, String> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let root = std::env::var_os("SystemRoot")
        .filter(|root| !root.is_empty())
        .ok_or("could not find Windows' own folder (SystemRoot is not set)")?;
    let mut tool = std::process::Command::new(Path::new(&root).join("System32").join(name));
    tool.creation_flags(NO_WINDOW).stdin(std::process::Stdio::null());
    Ok(tool)
}

/// Runs a tool and gives what it printed; an error says what it printed when it failed.
#[cfg(windows)]
pub(crate) fn run(tool: &mut std::process::Command) -> Result<String, String> {
    let name = tool.get_program().to_string_lossy().into_owned();
    let output = tool.output().map_err(|e| format!("could not run {name}: {e}"))?;
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let problem = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{name} failed: {} {}", printed.trim(), problem.trim()).trim().to_owned());
    }
    Ok(printed)
}

/// The user the app runs as, as Windows writes it.
#[cfg(windows)]
pub(crate) fn user() -> Result<String, String> {
    let printed = run(tool("whoami.exe")?.args(["/user", "/fo", "csv", "/nh"]))?;
    user_in(&printed).ok_or_else(|| format!("could not tell which user this is from: {}", printed.trim()))
}

/// How the notes access lists are read through are named: nothing like a socket's name, so one
/// left behind is known for what it is.
#[cfg(windows)]
const NOTE: &str = "softpresenter-access-note-";

/// The access list of a file or folder, read through a note `icacls` writes in `notes`, under
/// a name nobody can guess beforehand. The note is removed as soon as it is read, whatever
/// went wrong.
#[cfg(windows)]
pub(crate) fn access_list(of: &Path, notes: &Path) -> Result<String, String> {
    use std::hash::{BuildHasher, Hasher};
    let unguessable = std::collections::hash_map::RandomState::new().build_hasher().finish();
    let note = notes.join(format!("{NOTE}{}-{unguessable:016x}.txt", std::process::id()));
    // /L: of the thing itself, never of what a link points to.
    let saved = run(tool("icacls.exe")?.arg(of).arg("/save").arg(&note).arg("/L"));
    let read = std::fs::read(&note);
    let _ = std::fs::remove_file(&note);
    saved?;
    let bytes = read.map_err(|e| format!("could not read the access list of {}: {e}", of.display()))?;
    saved_list(&bytes).ok_or_else(|| format!("could not read the access list of {}", of.display()))
}

/// Makes the folder if it isn't there, and lets only this user in: this user owns it, its
/// access list comes from nowhere else and names this user, and everything made in it gets the
/// same list. The list is then read back, and anybody else on it is a reason not to listen.
#[cfg(windows)]
pub fn keep_private(folder: &Path) -> Result<Private, String> {
    if !folder.exists() {
        std::fs::create_dir_all(folder).map_err(|e| format!("Could not make {}: {e}", folder.display()))?;
    }
    real_folder(folder)?;
    let private = |problem: String| format!("Could not keep {} private: {problem}", folder.display());
    let user = user().map_err(private)?;
    // Owning it, as on Unix: an owner can always change the list, so nobody else may be it.
    run(tool("icacls.exe").map_err(private)?.arg(folder).arg("/setowner").arg(format!("*{user}")).arg("/L"))
        .map_err(private)?;
    // /inheritance:r: nothing comes from the folder above. (OI)(CI): files and folders made in
    // here get the same list. F: this user may do everything.
    run(tool("icacls.exe")
        .map_err(private)?
        .arg(folder)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("*{user}:(OI)(CI)F"))
        .arg("/L"))
    .map_err(private)?;
    // The list is read back twice, through notes in two places, and both must show a private
    // folder. A note is a file, and whoever can change it can make the list look private:
    // somebody still let into this folder could change a note kept here, and on a computer
    // whose temporary folder is shared somebody else might change one kept there. Nobody but
    // this user can do both unless both places are already theirs to write in. The temporary
    // folder comes first, so nothing is written here until one reading says it is private.
    for notes in [std::env::temp_dir().as_path(), folder] {
        let list = access_list(folder, notes).map_err(private)?;
        only_this_user(&list, &user, true).map_err(|who| others_in_folder(folder, &who))?;
    }
    // From here on the notes are kept in the folder, which is this user's alone. One left by a
    // copy that was stopped half-way is cleared: only plain files with a note's name, never a
    // socket.
    clear_notes(folder);
    // Still the folder that was looked at, not a link put in its place meanwhile.
    real_folder(folder)?;
    Ok(Private { user, folder: folder.to_owned() })
}

/// Removes notes left in the folder by a copy that was stopped before it could.
#[cfg(windows)]
fn clear_notes(folder: &Path) {
    use std::os::windows::fs::MetadataExt;
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let plain = std::fs::symlink_metadata(entry.path())
            .is_ok_and(|meta| meta.is_file() && meta.file_attributes() & REPARSE_POINT == 0);
        if plain && name.starts_with(NOTE) && name.ends_with(".txt") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(windows)]
impl Private {
    /// The socket just made is for this user only: its own access list decides who may
    /// connect, so it is read back too.
    pub fn check_socket(&self, socket: &Path) -> Result<(), String> {
        let list = access_list(socket, &self.folder)
            .map_err(|problem| format!("Could not keep {} private: {problem}", socket.display()))?;
        only_this_user(&list, &self.user, false).map_err(|who| others(socket, &who))
    }

    /// A socket that was already there keeps the access list it had, whatever the folder's is
    /// now. One that lets anybody else in is not asked whether an app answers on it, and is
    /// not replaced: it is in the way.
    pub fn check_existing(&self, socket: &Path) -> Result<(), String> {
        let in_the_way = |why: String| format!("{} is in the way of the control socket: {why}", socket.display());
        let list = access_list(socket, &self.folder)
            .map_err(|problem| in_the_way(format!("its access list can't be read ({problem}).")))?;
        only_this_user(&list, &self.user, false)
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

/// The user in what `whoami /user /fo csv /nh` prints: the last field of its line, which must
/// be written the way Windows writes a person's account (S-1-5-21-… with four numbers after
/// it, or S-1-12-1-… for an account kept by an organisation's directory in the cloud) or the
/// system's own (S-1-5-18). The field before it holds the computer's and the account's names,
/// which somebody else may have chosen, so nothing is looked for in there.
#[cfg(any(windows, test))]
fn user_in(printed: &str) -> Option<String> {
    let line = printed.lines().map(str::trim).rfind(|line| !line.is_empty())?;
    let (_names, field) = line.rsplit_once(',')?;
    let field = field.trim().strip_prefix('"')?.strip_suffix('"')?;
    let parts: Vec<&str> = field.split('-').collect();
    let numbers =
        parts.iter().skip(2).all(|part| (1..=10).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit()));
    let shape = match parts.as_slice() {
        ["S", "1", "5", "18"] => true,
        ["S", "1", "5", "21", rest @ ..] | ["S", "1", "12", "1", rest @ ..] => rest.len() == 4,
        _ => false,
    };
    (numbers && shape).then(|| field.to_owned())
}

/// The access list in a note `icacls /save` wrote: its last line, whether the note is in
/// Windows' two-byte letters or not.
#[cfg(any(windows, test))]
fn saved_list(note: &[u8]) -> Option<String> {
    let two_byte = note.starts_with(&[0xff, 0xfe]) || note.iter().skip(1).step_by(2).take(8).any(|byte| *byte == 0);
    let text = if two_byte {
        let units: Vec<u16> = note.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(note).into_owned()
    };
    let line = text.lines().map(|line| line.trim().trim_start_matches('\u{feff}')).rfind(|line| !line.is_empty())?;
    line.contains("D:").then(|| line.to_owned())
}

/// One line of an access list: who, and what it lets them do.
#[cfg(any(windows, test))]
struct Entry<'a> {
    /// `A` lets in; anything else (refusing, auditing, conditions) is not expected here.
    kind: &'a str,
    /// Two letters each: `OI` and `CI` pass the entry on to files and folders made inside.
    passed_on: &'a str,
    rights: &'a str,
    who: &'a str,
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
        let parts: Vec<&str> = inside[..end].split(';').collect();
        if parts.len() < 6 || inside[..end].contains('(') {
            return None;
        }
        found.push(Entry { kind: parts[0], passed_on: parts[1], rights: parts[2], who: parts[5] });
        rest = &inside[end + 1..];
    }
    // Nothing may follow but the system's own labels (S:…).
    (rest.is_empty() || rest.starts_with("S:")).then_some((marks.contains('P'), found))
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
    // Windows writes the first administrator account of a computer as LA, and the system's own
    // account (which a service may run as) as SY.
    let this_user = |who: &str| {
        who == user
            || (who == "LA" && user.starts_with("S-1-5-21-") && user.ends_with("-500"))
            || (who == "SY" && user == "S-1-5-18")
    };
    let mut lets_user_in = false;
    for entry in &entries {
        if entry.kind != "A" {
            return Err(format!("its access list has an entry of a kind not expected, {}", entry.kind));
        }
        if this_user(entry.who) {
            let passed_on: Vec<&[u8]> = entry.passed_on.as_bytes().chunks(2).collect();
            let to_files = passed_on.contains(&b"OI".as_slice());
            let all = entry.rights == "FA" || entry.rights.eq_ignore_ascii_case("0x1f01ff");
            lets_user_in |= all && (to_files || !folder);
        } else if !matches!(entry.who, "SY" | "BA" | "S-1-5-18" | "S-1-5-32-544") {
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
    fn the_user_is_found_in_what_whoami_prints() {
        assert_eq!(user_in("\"desk\\operator\",\"S-1-5-21-1111-2222-3333-1001\"\r\n").as_deref(), Some(USER));
        assert_eq!(user_in("\"nt authority\\system\",\"S-1-5-18\"\r\n").as_deref(), Some("S-1-5-18"));
        assert_eq!(user_in("\"cloud\\operator\",\"S-1-12-1-11-22-33-44\"").as_deref(), Some("S-1-12-1-11-22-33-44"));
        for not_a_user in [
            "nothing of the kind",
            "\"x\",\"S-1-\"",
            "\"x\",\"\"",
            // Not in the last field, or not a field at all.
            "desk\\operator S-1-5-21-1111-2222-3333-1001",
            "\"S-1-5-21-1111-2222-3333-1001\"",
            "\"S-1-5-21-1111-2222-3333-1001\",\"operator\"",
            // A group everybody is in, a list's short names, too few or too many numbers, rubbish.
            "\"x\",\"S-1-1-0\"",
            "\"x\",\"S-1-5-32-544\"",
            "\"x\",\"WD\"",
            "\"x\",\"S-1-5-21-1111-2222-3333\"",
            "\"x\",\"S-1-5-21-1111-2222-3333-1001-5\"",
            "\"x\",\"S-1-5-21-1111-2222-3333-1001 \"",
            "\"x\",\"S-1-5-21-1111-2222-3333-1001:(OI)(CI)F\"",
            "\"x\",\"S-1-5-21-1111--3333-1001\"",
            "\"x\",S-1-5-21-1111-2222-3333-1001",
        ] {
            assert_eq!(user_in(not_a_user), None, "{not_a_user}");
        }
    }

    /// The names in front are somebody else's to choose; an account written into them is not
    /// taken for the user.
    #[test]
    fn a_computer_or_account_named_like_a_user_is_not_taken_for_one() {
        for names in ["S-1-5-21-9-9-9-500\\operator", "desk\\S-1-5-18", "S-1-1-0\\S-1-5-21-9-9-9-500"] {
            let printed = format!("\"{names}\",\"{USER}\"\r\n");
            assert_eq!(user_in(&printed).as_deref(), Some(USER), "{printed}");
            // And with no proper last field there is no user at all, rather than the name's.
            assert_eq!(user_in(&format!("\"{names}\",\"\"")), None);
            assert_eq!(user_in(&format!("\"{names}\"")), None);
        }
    }

    #[test]
    fn a_saved_access_list_is_read_in_either_kind_of_letters() {
        let text = "control\r\nD:PAI(A;OICI;FA;;;S-1-5-21-1111-2222-3333-1001)\r\n";
        let wanted = "D:PAI(A;OICI;FA;;;S-1-5-21-1111-2222-3333-1001)";
        assert_eq!(saved_list(text.as_bytes()).as_deref(), Some(wanted));
        let two_byte: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(saved_list(&two_byte).as_deref(), Some(wanted));
        let mut marked = vec![0xff, 0xfe];
        marked.extend(&two_byte);
        assert_eq!(saved_list(&marked).as_deref(), Some(wanted));
        assert_eq!(saved_list(b"control\r\n"), None);
        assert_eq!(saved_list(b""), None);
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
}
