//! The signed-in user's entries in Windows' Credential Manager: read, written and removed with
//! Windows' own calls. Each is a "generic" credential, kept for this user on this computer (it
//! does not travel with a roaming profile), which Windows encrypts with the user's sign-in.
//! What is kept is bytes; nothing here prints them or puts them in an error.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, FILETIME};
use windows_sys::Win32::Security::Credentials::{
    CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
    CredReadW, CredWriteW,
};

/// The most bytes one entry holds.
pub const ROOM: usize = CRED_MAX_CREDENTIAL_BLOB_SIZE as usize;

/// Text as Windows takes it: wide letters ending in a NUL. Text with a NUL inside is refused,
/// as Windows would read only the part before it. `what` names the text in the error; the text
/// itself is not repeated.
fn wide(text: &str, what: &str) -> Result<Vec<u16>, String> {
    if text.contains('\0') {
        return Err(format!("the {what} has a NUL in it"));
    }
    Ok(text.encode_utf16().chain(Some(0)).collect())
}

/// True when the Windows call that just failed said there is no such entry.
fn not_found(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(ERROR_NOT_FOUND as i32)
}

/// What the entry named `target` holds, or `None` when there is no such entry.
pub fn read(target: &str) -> Result<Option<Vec<u8>>, String> {
    let name = wide(target, "entry's name")?;
    let mut found: *mut CREDENTIALW = null_mut();
    // SAFETY: `name` is a NUL-terminated wide string that outlives the call, and `found` is a
    // place for the one pointer CredReadW writes when it succeeds.
    if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut found) } == 0 {
        let error = std::io::Error::last_os_error();
        return if not_found(&error) { Ok(None) } else { Err(error.to_string()) };
    }
    if found.is_null() {
        return Err("Windows gave no entry".to_owned());
    }
    // SAFETY: CredReadW succeeded, so `found` points to a CREDENTIALW that Windows allocated
    // and that stays valid until CredFree below; only two plain fields are copied out of it.
    let (blob, size) = unsafe { ((*found).CredentialBlob, (*found).CredentialBlobSize as usize) };
    let kept = if blob.is_null() || size == 0 {
        Vec::new()
    } else {
        // SAFETY: Windows says the entry's blob is `size` bytes at `blob`, inside the buffer
        // that is still allocated; the bytes are copied before it is freed.
        unsafe { std::slice::from_raw_parts(blob, size) }.to_vec()
    };
    // SAFETY: `found` came from CredReadW, is freed only here, and nothing points into it any
    // more (`kept` is a copy).
    unsafe { CredFree(found.cast()) };
    Ok(Some(kept))
}

/// Writes the entry named `target` (replacing one of that name), holding `secret`. `user` and
/// `comment` are what Credential Manager shows beside it; neither is secret.
pub fn write(target: &str, user: &str, comment: &str, secret: &[u8]) -> Result<(), String> {
    if secret.len() > ROOM {
        return Err(format!("an entry holds at most {ROOM} bytes"));
    }
    let mut name = wide(target, "entry's name")?;
    let mut user = wide(user, "entry's user name")?;
    let mut comment = wide(comment, "entry's comment")?;
    let entry = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_mut_ptr(),
        Comment: comment.as_mut_ptr(),
        LastWritten: FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 },
        CredentialBlobSize: secret.len() as u32,
        // CredWriteW only reads the blob; the field is `*mut` because the same structure is
        // what CredReadW gives back.
        CredentialBlob: secret.as_ptr().cast_mut(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: null_mut(),
        TargetAlias: null_mut(),
        UserName: user.as_mut_ptr(),
    };
    // SAFETY: `entry` is a whole CREDENTIALW whose three strings are NUL-terminated wide
    // strings and whose blob is `secret.len()` readable bytes (at most ROOM, checked above);
    // all of them outlive the call, which only reads them, and Windows copies what it keeps.
    if unsafe { CredWriteW(&entry, 0) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

/// Removes the entry named `target`. `Ok(false)` when there was no such entry.
pub fn remove(target: &str) -> Result<bool, String> {
    let name = wide(target, "entry's name")?;
    // SAFETY: `name` is a NUL-terminated wide string that outlives the call.
    if unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        let error = std::io::Error::last_os_error();
        return if not_found(&error) { Ok(false) } else { Err(error.to_string()) };
    }
    Ok(true)
}
