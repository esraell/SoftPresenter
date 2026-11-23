//! Reading a zip that somebody else made without trusting it: the entries are counted, the size
//! of each and of all together is bounded by what is actually read (never by what an entry says
//! about itself), a zip whose entries overlap is refused, an entry's name is only looked at and
//! never used as a place on disk, and a zip inside a zip is not opened. Bibles and presentation
//! files are read through it.

use std::io::Read;

/// How much may be read from one zip.
#[derive(Clone, Copy, Debug)]
pub struct ZipLimits {
    /// The largest zip.
    pub file: u64,
    /// The largest one entry, unpacked.
    pub entry: u64,
    /// Every entry read, together, unpacked.
    pub unpacked: u64,
    /// The most entries the zip may list.
    pub entries: usize,
}

/// Why a zip was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZipRefusal {
    /// Not a zip, or damaged.
    Damaged,
    /// Entries share their packed bytes, which would make the same unpacking be done over and
    /// over: none is ever made that way honestly.
    Overlap,
    /// More entries than the limit.
    TooMany,
    /// More unpacked than the limits allow.
    UnpacksTooMuch,
}

/// Whether the bytes start as a zip does.
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

/// Goes through the entries of a zip for which `wanted(name)` is true, handing each one's name
/// and bytes to `visit`, with the count of bytes unpacked so far, which `visit` adds to by what
/// it makes of the bytes if that is more than their length (decoded text can be longer). An
/// entry that is itself a zip counts but is not passed on. A refusal from `visit` stops it.
pub fn for_each_entry(
    bytes: &[u8],
    limits: &ZipLimits,
    wanted: impl Fn(&str) -> bool,
    mut visit: impl FnMut(String, Vec<u8>, &mut u64) -> Result<(), ZipRefusal>,
) -> Result<(), ZipRefusal> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| ZipRefusal::Damaged)?;
    if zip.len() > limits.entries {
        return Err(ZipRefusal::TooMany);
    }
    if zip.has_overlapping_files().unwrap_or(true) {
        return Err(ZipRefusal::Overlap);
    }
    let mut unpacked = 0_u64;
    let mut packed = 0_u64;
    for at in 0..zip.len() {
        let (entry_name, is_wanted, size) = {
            let entry = zip.by_index_raw(at).map_err(|_| ZipRefusal::Damaged)?;
            let entry_name = String::from_utf8_lossy(entry.name_raw()).into_owned();
            let plain = !entry.is_dir() && !entry.is_symlink() && !entry.encrypted();
            (entry_name.clone(), plain && wanted(&entry_name), entry.compressed_size())
        };
        if !is_wanted {
            continue;
        }
        // The entries read can't be packed into more bytes than the zip has.
        packed = packed.saturating_add(size);
        if packed > bytes.len() as u64 {
            return Err(ZipRefusal::Damaged);
        }
        let mut entry = zip.by_index(at).map_err(|_| ZipRefusal::Damaged)?;
        // What the entry says about its size is not trusted: only what is read counts.
        let room = limits.entry.min(limits.unpacked.saturating_sub(unpacked));
        let mut read = Vec::new();
        entry.by_ref().take(room + 1).read_to_end(&mut read).map_err(|_| ZipRefusal::Damaged)?;
        if read.len() as u64 > room {
            return Err(ZipRefusal::UnpacksTooMuch);
        }
        if is_zip(&read) {
            unpacked += read.len() as u64;
            continue;
        }
        visit(entry_name, read, &mut unpacked)?;
    }
    Ok(())
}
