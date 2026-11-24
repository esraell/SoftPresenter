//! Who owns a file or folder and who may open it, read and set through Windows' own security
//! calls on a handle to the thing itself (never what a link or a junction points to), and the
//! user this program runs as, from its own token. Access lists go in and out in the form
//! Windows writes them down (SDDL); deciding what they mean is left to the caller.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetSecurityDescriptorOwner, GetTokenInformation, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle, OPEN_EXISTING, READ_CONTROL,
    WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::core::{BOOL, PWSTR};

/// Who owns a file or folder and who may open it.
#[derive(Clone, Debug)]
pub struct Security {
    /// The owner, written out as a SID (`S-1-5-21-…`, never a short name); `None` if it has
    /// none.
    pub owner: Option<String>,
    /// The access list as Windows writes it down (`D:PAI(A;OICI;FA;;;S-1-5-21-…)`), short names
    /// and all. Empty if there is no list at all, and `D:NO_ACCESS_CONTROL` for an empty one:
    /// both let everybody in.
    pub list: String,
}

/// A handle this module opened, closed when dropped.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by this module, is closed only here and is not used
        // after.
        unsafe { CloseHandle(self.0) };
    }
}

/// Memory a Windows call allocated for its answer (a string, a security descriptor), freed when
/// dropped.
struct Local<T>(*mut T);

impl<T> Drop for Local<T> {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer came from a Windows call that allocates its answer with
            // LocalAlloc and leaves freeing it to the caller; it is freed only here and not used
            // after.
            unsafe { LocalFree(self.0.cast()) };
        }
    }
}

/// Text as Windows takes it: wide letters ending in a NUL. Text with a NUL inside is refused,
/// as Windows would read only the part before it.
fn wide(text: &std::ffi::OsStr) -> Result<Vec<u16>, String> {
    let wide: Vec<u16> = text.encode_wide().collect();
    if wide.contains(&0) {
        return Err(format!("{} has a NUL in it", text.display()));
    }
    Ok(wide.into_iter().chain(Some(0)).collect())
}

/// A wide, NUL-terminated string a Windows call wrote, as text. Anything that isn't proper
/// UTF-16 is an error rather than guessed at.
///
/// # Safety
///
/// `text` must be null or point to a NUL-terminated wide string that stays valid during the
/// call.
unsafe fn read_wide(text: *const u16) -> Result<String, String> {
    if text.is_null() {
        return Err("Windows gave no text".to_owned());
    }
    let mut len = 0;
    // SAFETY: the string ends in a NUL (the caller's word), so every place up to and including
    // the NUL may be read, and the loop stops there.
    while unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: the `len` places before the NUL were each just read.
    let units = unsafe { std::slice::from_raw_parts(text, len) };
    String::from_utf16(units).map_err(|_| "Windows gave text that is not UTF-16".to_owned())
}

/// A SID written out (`S-1-5-21-…`).
///
/// # Safety
///
/// `sid` must point to a valid SID that stays valid during the call.
unsafe fn sid_text(sid: PSID) -> Result<String, String> {
    let mut text: PWSTR = null_mut();
    // SAFETY: `sid` is a valid SID (the caller's word); the call writes into `text` a pointer
    // to a new string, which `Local` frees.
    let converted = unsafe { ConvertSidToStringSidW(sid, &mut text) } != 0;
    let text = Local(text);
    if !converted {
        return Err(format!("could not write out a SID: {}", std::io::Error::last_os_error()));
    }
    // SAFETY: on success `text` is a NUL-terminated wide string, held until the end of this
    // function.
    unsafe { read_wide(text.0) }
}

/// The user this program runs as, written out as a SID (`S-1-5-21-…`), from its own token.
pub fn current_user() -> Result<String, String> {
    let failed =
        |what: &str| format!("could not tell which user this is ({what}: {})", std::io::Error::last_os_error());
    let mut token: HANDLE = null_mut();
    // SAFETY: GetCurrentProcess gives a stand-in handle for this program that needs no closing;
    // OpenProcessToken writes the token's handle into `token`, a valid place for one.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(failed("its token can't be opened"));
    }
    let token = Handle(token);
    let mut needed = 0u32;
    // SAFETY: asked with no room for the answer, GetTokenInformation only writes the size it
    // needs into `needed` (and fails, as expected).
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed) };
    // Room for the answer, in pieces aligned for a TOKEN_USER (which starts with a pointer).
    const _: () = assert!(align_of::<TOKEN_USER>() <= align_of::<usize>());
    let bytes = (needed as usize).max(size_of::<TOKEN_USER>());
    let mut answer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    let room = u32::try_from(answer.len() * size_of::<usize>())
        .map_err(|_| "could not tell which user this is (its token's answer is too long)".to_owned())?;
    // SAFETY: `answer` may be written for `room` bytes and is aligned for a TOKEN_USER.
    if unsafe { GetTokenInformation(token.0, TokenUser, answer.as_mut_ptr().cast(), room, &mut needed) } == 0 {
        return Err(failed("its token can't be read"));
    }
    // SAFETY: Windows filled `answer` with a TOKEN_USER, which may be read from its start.
    let sid = unsafe { (*answer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    if sid.is_null() {
        return Err("could not tell which user this is (its token names nobody)".to_owned());
    }
    // SAFETY: the SID lies in `answer`, which Windows filled and which outlives this call.
    unsafe { sid_text(sid) }
}

/// Opens a file or folder itself: FILE_FLAG_OPEN_REPARSE_POINT opens a link, a junction or a
/// socket file and never what it points to, and FILE_FLAG_BACKUP_SEMANTICS lets a folder be
/// opened.
fn open(path: &Path, access: u32, share: u32) -> Result<Handle, String> {
    let name = wide(path.as_os_str())?;
    // SAFETY: `name` is a NUL-terminated wide string that outlives the call; no security
    // attributes and no template are passed (null), as CreateFileW allows.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            share,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err(format!("could not open {}: {}", path.display(), std::io::Error::last_os_error()));
    }
    Ok(Handle(handle))
}

/// The owner and access list of what a handle (opened with READ_CONTROL) is open on.
fn security(handle: &Handle) -> Result<Security, String> {
    let mut owner: PSID = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: the handle is open with READ_CONTROL. Windows writes into `descriptor` a pointer
    // to a descriptor it allocated, which `Local` frees, and into `owner` a pointer into that
    // descriptor; the group and the lists' own pointers are not asked for (null), which the
    // call allows when the descriptor is.
    let error = unsafe {
        GetSecurityInfo(
            handle.0,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    let descriptor = Local(descriptor);
    if error != 0 {
        return Err(format!("could not read who may open it: {}", std::io::Error::from_raw_os_error(error as i32)));
    }
    let owner = if owner.is_null() {
        None
    } else {
        // SAFETY: `owner` points to the owner's SID inside `descriptor`, which is still held.
        Some(unsafe { sid_text(owner) }?)
    };
    let mut text: PWSTR = null_mut();
    // SAFETY: `descriptor` holds the valid descriptor Windows just gave; the call writes into
    // `text` a pointer to a new string, which `Local` frees, and the string's length is not
    // asked for (null), which the call allows.
    let written = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor.0,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            null_mut(),
        )
    } != 0;
    let text = Local(text);
    if !written {
        return Err(format!("could not write out who may open it: {}", std::io::Error::last_os_error()));
    }
    // SAFETY: on success `text` is a NUL-terminated wide string, held until the end of this
    // function.
    let list = unsafe { read_wide(text.0) }?;
    Ok(Security { owner, list })
}

/// The owner and access list of a file or folder itself (a socket file too), never of what a
/// link or a junction points to. It is opened only to read them, and nothing is held against
/// anybody else.
pub fn security_of(path: &Path) -> Result<Security, String> {
    let handle = open(path, READ_CONTROL, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)?;
    security(&handle)
}

/// A security descriptor made from its written form (SDDL), with the owner and access list in
/// it, which point into its memory.
struct Descriptor {
    memory: Local<core::ffi::c_void>,
    owner: PSID,
    list: *mut ACL,
}

/// The descriptor a written form describes, which must name an owner and give an access list
/// that is protected (takes nothing on from the folder above). Never one without a list, which
/// would let everybody in.
fn descriptor(security: &str) -> Result<Descriptor, String> {
    let text = wide(std::ffi::OsStr::new(security))?;
    let mut made: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: `text` is a NUL-terminated wide string that outlives the call; the call writes
    // into `made` a pointer to a descriptor it allocated, which `Local` frees, and its size is
    // not asked for (null), which the call allows.
    let read = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(), SDDL_REVISION_1, &mut made, null_mut())
    } != 0;
    let memory = Local(made);
    if !read {
        return Err(format!("Windows could not read {security}: {}", std::io::Error::last_os_error()));
    }
    let (mut owner, mut list): (PSID, *mut ACL) = (null_mut(), null_mut());
    let (mut defaulted, mut present): (BOOL, BOOL) = (0, 0);
    let (mut control, mut revision) = (0u16, 0u32);
    // SAFETY: `memory` holds the valid descriptor just made; each call writes only into the
    // local variables it is given, and the pointers it gives point into `memory`.
    let looked = unsafe {
        GetSecurityDescriptorOwner(memory.0, &mut owner, &mut defaulted) != 0
            && GetSecurityDescriptorDacl(memory.0, &mut present, &mut list, &mut defaulted) != 0
            && GetSecurityDescriptorControl(memory.0, &mut control, &mut revision) != 0
    };
    if !looked {
        return Err(format!("could not look into {security}: {}", std::io::Error::last_os_error()));
    }
    if owner.is_null() {
        return Err(format!("{security} names no owner"));
    }
    if present == 0 || list.is_null() {
        return Err(format!("{security} gives no access list, which would let everybody in"));
    }
    if control & SE_DACL_PROTECTED == 0 {
        return Err(format!("{security} would take on the access list of the folder above"));
    }
    Ok(Descriptor { memory, owner, list })
}

/// Makes a folder that has this owner and access list (written as SDDL; the list must be
/// protected) from the moment it is made, so there is no moment when it has any other. `false`
/// if something of that name is already there, which is left as it is.
pub fn make_folder(path: &Path, security: &str) -> Result<bool, String> {
    let name = wide(path.as_os_str())?;
    let descriptor = descriptor(security)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.memory.0,
        bInheritHandle: 0,
    };
    // SAFETY: `name` is a NUL-terminated wide string, and `attributes` is a SECURITY_ATTRIBUTES
    // with its size set that points to a valid descriptor; all outlive the call.
    if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } != 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS as i32) {
        return Ok(false);
    }
    Err(format!("could not make {}: {error}", path.display()))
}

/// A folder held open to be kept private. While it is held nobody can delete or rename it
/// (it is open without FILE_SHARE_DELETE), so it stays the folder at its path, and what is read
/// and set is that folder itself, never what a link or a junction in its place points to.
pub struct HeldFolder(Handle);

impl HeldFolder {
    /// Opens what is at a path to read and set its owner and access list, and holds it: while
    /// it is held nobody can open it to delete or rename it, and one that has it open to do
    /// either stands in the way (opening fails). It asks to list the folder as well, because
    /// Windows leaves a handle that only reads attributes or sets security out of its sharing
    /// rules, so without that the hold would keep nobody out (a test on Windows showed it). A
    /// program that has the folder open to look inside, sharing it for reading, doesn't stand in
    /// the way.
    pub fn open(path: &Path) -> Result<HeldFolder, String> {
        let access = READ_CONTROL | WRITE_DAC | WRITE_OWNER | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY;
        open(path, access, FILE_SHARE_READ | FILE_SHARE_WRITE).map(HeldFolder)
    }

    /// Whether it is a real folder: not a file, a link or a junction.
    pub fn is_real_folder(&self) -> Result<bool, String> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the handle is open, and `info` is a valid place for the answer.
        if unsafe { GetFileInformationByHandle(self.0.0, &mut info) } == 0 {
            return Err(format!("could not look at it: {}", std::io::Error::last_os_error()));
        }
        let attributes = info.dwFileAttributes;
        Ok(attributes & FILE_ATTRIBUTE_DIRECTORY != 0 && attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0)
    }

    /// Its owner and access list.
    pub fn security(&self) -> Result<Security, String> {
        security(&self.0)
    }

    /// Gives it this owner and access list (written as SDDL), the list protected: nothing is
    /// taken on from the folder above. Windows passes the list's inheritable entries on to
    /// what is already inside.
    pub fn set_security(&self, security: &str) -> Result<(), String> {
        let descriptor = descriptor(security)?;
        // SAFETY: the handle is open with WRITE_DAC and WRITE_OWNER; `descriptor.owner` and
        // `descriptor.list` point into `descriptor`, held until the end of this function, and
        // neither is null (a null list would let everybody in). No group and no system list are
        // set (null), as the flags say.
        let error = unsafe {
            SetSecurityInfo(
                self.0.0,
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor.owner,
                null_mut(),
                descriptor.list,
                null(),
            )
        };
        if error != 0 {
            return Err(format!("could not set who may open it: {}", std::io::Error::from_raw_os_error(error as i32)));
        }
        Ok(())
    }
}
