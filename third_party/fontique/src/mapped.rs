// Copyright 2026 the SoftPresenter contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Font data mapped from a file that nothing else can change while it is mapped.
//!
//! SoftPresenter: added. `load_blob` reads font files whole, because a mapped file that another
//! program cuts short or writes over kills the process with a bus error. Reading costs the whole
//! file in memory, though: 188 MB for an emoji font of which a slide uses a few glyphs. A map
//! costs only the pages used, and the system can drop those again. So a font file is mapped
//! here, but only when nothing another program does can change what is mapped:
//!
//! - on macOS, a file on the system volume the computer started from, which is a read-only
//!   snapshot that can be neither written to nor unmounted while the system runs (the signed
//!   system volume of macOS 11 and later, where the large fallback fonts are); or
//! - on macOS and Linux, a private copy of the file, made in a folder the application keeps to
//!   its user, opened and then left with no name at all, so that no program can open it to
//!   change it; the copy's data stays the copy's whatever then happens to the original (where
//!   the disk can, the copy is a clone: an APFS clone, or a reflink on Btrfs or XFS, made at
//!   once and sharing the original's blocks until either changes).
//!
//! Anything else (Windows, a copy that can't be made, a disk elsewhere on a network) is left
//! to the caller, which reads the file whole as before.

use crate::Blob;
use std::fs::File;
use std::path::Path;
use std::string::String;
use std::sync::Arc;

/// Where the data [`map_unchanging`] hands out is mapped from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MappedFrom {
    /// The font file itself, on the read-only system snapshot macOS started from.
    SystemVolume,
    /// A private copy of the font file, with no name left.
    PrivateCopy,
}

/// The data of a font file, mapped where nothing another program does can change it, or `None`.
///
/// The file is mapped in place if it is on macOS's system volume. If not, and `copies` names a
/// folder, it is copied into that folder (cloned where the disk can), the copy is opened, its
/// name removed, and the copy mapped. `copies` must be a folder only this user can enter (the
/// copy has a name there for a moment) on a disk of this computer. `None` when neither can be
/// done; nothing is left behind in `copies` either way, unless the process ends in the moment
/// between the copy being made and its name removed (look for names ending in
/// [`COPY_SUFFIX`]).
///
/// The data is not checked to be a font.
///
/// SoftPresenter: added.
pub fn map_unchanging(path: &Path, copies: Option<&Path>) -> Option<(Blob<u8>, MappedFrom)> {
    let file = File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    if on_system_volume(&file) {
        return map(&file).map(|blob| (blob, MappedFrom::SystemVolume));
    }
    // Elsewhere a file can't be left without a name while it is open.
    if !cfg!(unix) {
        return None;
    }
    let copy = copies?.join(copy_name(path));
    // `std::fs::copy` writes over a file that is already there, cutting it short first, which
    // is what must never happen to a file mapped somewhere: only a new name is copied to.
    if std::fs::symlink_metadata(&copy).is_ok() {
        return None;
    }
    let opened = std::fs::copy(path, &copy).ok().and_then(|_| File::open(&copy).ok());
    // Whatever happened, the name goes.
    let _ = std::fs::remove_file(&copy);
    let opened = opened?;
    if !nameless_local_file(&opened) {
        return None;
    }
    map(&opened).map(|blob| (blob, MappedFrom::PrivateCopy))
}

/// How the names of the private copies end, so that one left behind can be told apart.
///
/// SoftPresenter: added.
pub const COPY_SUFFIX: &str = ".fontcopy";

/// A name for a private copy that no other copy has had: this process's, a count and a number
/// nobody can guess beforehand, then the file's own name for whoever looks.
fn copy_name(path: &Path) -> String {
    use core::hash::{BuildHasher, Hasher};
    use core::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    let unguessable = std::collections::hash_map::RandomState::new().build_hasher().finish();
    let name = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
    let name: String = name.chars().filter(|c| c.is_alphanumeric() || "-_.".contains(*c)).take(64).collect();
    std::format!("{}-{count}-{unguessable:016x}-{name}{COPY_SUFFIX}", std::process::id())
}

/// Maps the whole of an open file, read-only.
fn map(file: &File) -> Option<Blob<u8>> {
    if file.metadata().ok()?.len() == 0 {
        return None;
    }
    // SAFETY: a map stays sound only while no one changes or cuts short the file under it.
    // `map_unchanging` maps only a file that nothing can change while it runs: a file on the
    // read-only system snapshot macOS started from (`on_system_volume`, checked on this very
    // descriptor), or a copy made a moment ago in a private folder, which has no name left
    // (`nameless_local_file`, also checked on this descriptor), so no other program can open
    // it, and which lives on a disk of this computer. Only root, or a debugger attached to this
    // process, could still reach either. The map is read-only, and nothing here writes to it.
    let mapped = unsafe { memmap2::Mmap::map(file) }.ok()?;
    Some(Blob::new(Arc::new(mapped)))
}

/// Mount flags, from macOS's `<sys/mount.h>`.
#[cfg(target_vendor = "apple")]
mod mount {
    pub(super) const RDONLY: u32 = 0x0000_0001;
    pub(super) const LOCAL: u32 = 0x0000_1000;
    pub(super) const ROOTFS: u32 = 0x0000_4000;
    pub(super) const SNAPSHOT: u32 = 0x4000_0000;
}

/// Whether an open file is on the system volume macOS started from: a read-only snapshot on
/// this computer's own disk, which nothing can write to and which can't be unmounted while the
/// system runs. Every flag must be there; a read-only disk image or network share has some of
/// them but can be ejected, so it never passes.
#[cfg(target_vendor = "apple")]
fn on_system_volume(file: &File) -> bool {
    use mount::{LOCAL, RDONLY, ROOTFS, SNAPSHOT};
    let all = RDONLY | LOCAL | ROOTFS | SNAPSHOT;
    rustix::fs::fstatfs(file).is_ok_and(|fs| fs.f_flags & all == all)
}

#[cfg(not(target_vendor = "apple"))]
fn on_system_volume(_file: &File) -> bool {
    false
}

/// Whether an open file is a plain file that has no name left anywhere (so no program can open
/// it any more) and is kept on a disk of this computer (one on a network can change or go away
/// beneath a map).
#[cfg(unix)]
fn nameless_local_file(file: &File) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = file.metadata() else { return false };
    meta.is_file() && meta.nlink() == 0 && on_local_disk(file)
}

#[cfg(not(unix))]
fn nameless_local_file(_file: &File) -> bool {
    false
}

#[cfg(target_vendor = "apple")]
fn on_local_disk(file: &File) -> bool {
    rustix::fs::fstatfs(file).is_ok_and(|fs| fs.f_flags & mount::LOCAL != 0)
}

/// On Linux, only the usual disk file systems count: ext2 to ext4, Btrfs, XFS, F2FS, bcachefs
/// and ZFS (the numbers `statfs` gives for each). A copy kept in memory (tmpfs) would save
/// nothing, and one on a network file system could change beneath the map.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn on_local_disk(file: &File) -> bool {
    const LOCAL: [u32; 6] = [0xEF53, 0x9123_683E, 0x5846_5342, 0xF2F5_2010, 0xCA45_1A4E, 0x2FC1_2FC1];
    rustix::fs::fstatfs(file).is_ok_and(|fs| LOCAL.contains(&(fs.f_type as u32)))
}

#[cfg(all(unix, not(target_vendor = "apple"), not(any(target_os = "linux", target_os = "android"))))]
fn on_local_disk(_file: &File) -> bool {
    false
}
