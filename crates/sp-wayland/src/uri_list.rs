//! The list of files another program hands over when they are dragged from it: `text/uri-list`
//! (RFC 2483), one address a line.

use std::path::PathBuf;

/// The files of a list, and how many of its lines were something else.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Files {
    pub paths: Vec<PathBuf>,
    /// Lines that are no file on this computer: a web address, a file on another computer, a
    /// line that can't be read.
    pub skipped: usize,
}

/// The files on this computer that `list` names. Lines end in CR LF (a bare LF is taken too),
/// a line starting with `#` is a comment, and a file is a `file:` address with no computer
/// named, or `localhost`, or `host` (this computer's own name); what `%` hides in it is put
/// back. Anything else is left out and counted.
pub fn local_files(list: &[u8], host: Option<&str>) -> Files {
    let mut files = Files::default();
    for line in list.split(|byte| *byte == b'\n').map(<[u8]>::trim_ascii) {
        if line.is_empty() || line.starts_with(b"#") {
            continue;
        }
        match local_file(line, host) {
            Some(path) => files.paths.push(path),
            None => files.skipped += 1,
        }
    }
    files
}

/// The file on this computer one line names, if it names one.
fn local_file(address: &[u8], host: Option<&str>) -> Option<PathBuf> {
    let rest = address.get(..5).filter(|scheme| scheme.eq_ignore_ascii_case(b"file:")).map(|_| &address[5..])?;
    // "file://computer/path", or "file:/path" with no computer named.
    let path = match rest.strip_prefix(b"//") {
        Some(named) => {
            let (computer, path) = named.split_at(named.iter().position(|byte| *byte == b'/')?);
            let here = computer.is_empty()
                || computer.eq_ignore_ascii_case(b"localhost")
                || host.is_some_and(|host| computer.eq_ignore_ascii_case(host.as_bytes()));
            here.then_some(path)?
        }
        None => rest,
    };
    let path = percent_decoded(path)?;
    (path.starts_with(b"/") && !path.contains(&0)).then(|| path_of(path))
}

/// `text` with each `%41` turned back into the byte it stands for; `None` when a `%` isn't
/// followed by two hexadecimal digits.
fn percent_decoded(text: &[u8]) -> Option<Vec<u8>> {
    let hex = |digit: u8| char::from(digit).to_digit(16).and_then(|value| u8::try_from(value).ok());
    let mut decoded = Vec::with_capacity(text.len());
    let mut bytes = text.iter().copied();
    while let Some(byte) = bytes.next() {
        decoded.push(if byte == b'%' { hex(bytes.next()?)? * 16 + hex(bytes.next()?)? } else { byte });
    }
    Some(decoded)
}

/// A path from its bytes: as they are where a file's name is bytes, as UTF-8 elsewhere.
fn path_of(bytes: Vec<u8>) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &str) -> Vec<PathBuf> {
        local_files(list.as_bytes(), None).paths
    }

    #[test]
    fn each_line_is_a_file_and_the_last_may_end_the_list() {
        let list = "file:///home/anna/Pictures/sky.png\r\nfile:///home/anna/Pictures/sea.jpg\r\n";
        let files = local_files(list.as_bytes(), None);
        assert_eq!(
            files.paths,
            [PathBuf::from("/home/anna/Pictures/sky.png"), PathBuf::from("/home/anna/Pictures/sea.jpg")]
        );
        assert_eq!(files.skipped, 0, "the list's last line end is not a line of its own");
        // Without the last line end, with bare line feeds, and with blank lines between.
        assert_eq!(paths("file:///a.png\r\nfile:///b.png"), [PathBuf::from("/a.png"), PathBuf::from("/b.png")]);
        assert_eq!(paths("file:///a.png\n\n  file:///b.png  \n"), [PathBuf::from("/a.png"), PathBuf::from("/b.png")]);
        assert_eq!(local_files(b"", None), Files::default());
    }

    #[test]
    fn what_a_percent_sign_hides_is_put_back() {
        assert_eq!(
            paths("file:///home/anna/Easter%20Sunday/the%20cross.png\r\n"),
            [PathBuf::from("/home/anna/Easter Sunday/the cross.png")]
        );
        // Letters outside ASCII come as UTF-8 behind percent signs; some programs send them bare.
        assert_eq!(
            paths("file:///home/anna/%D0%9F%D0%B0%D1%81%D1%85%D0%B0/%C3%A9t%C3%A9.jpg\r\n"),
            [PathBuf::from("/home/anna/Пасха/été.jpg")]
        );
        assert_eq!(paths("file:///home/anna/Пасха/été.jpg\r\n"), [PathBuf::from("/home/anna/Пасха/été.jpg")]);
        // A space left bare, a percent sign of the name's own, and marks an address would read
        // as something else.
        assert_eq!(paths("file:///home/anna/two words.png\r\n"), [PathBuf::from("/home/anna/two words.png")]);
        assert_eq!(paths("file:///100%25%20wool%23%3F.png\r\n"), [PathBuf::from("/100% wool#?.png")]);
        // A plus sign is a plus sign, not a space.
        assert_eq!(paths("file:///a+b.png\r\n"), [PathBuf::from("/a+b.png")]);
    }

    #[test]
    fn comments_are_not_files_and_are_not_counted() {
        let list = "# from the file manager\r\nfile:///a.png\r\n#file:///hidden.png\r\n";
        assert_eq!(local_files(list.as_bytes(), None), Files { paths: vec![PathBuf::from("/a.png")], skipped: 0 });
    }

    #[test]
    fn only_files_on_this_computer_are_taken_and_the_rest_counted() {
        let list = "https://example.org/sky.png\r\n\
                    file:///a.png\r\n\
                    sftp://elsewhere/b.png\r\n\
                    file://elsewhere/c.png\r\n\
                    file://localhost/d.png\r\n\
                    FILE:///e.png\r\n\
                    file:/f.png\r\n\
                    /home/anna/g.png\r\n\
                    file:relative.png\r\n\
                    file:///bad%zz.png\r\n\
                    file:///cut%2\r\n\
                    file:///nul%00.png\r\n\
                    file://\r\n";
        let files = local_files(list.as_bytes(), None);
        assert_eq!(files.paths, ["/a.png", "/d.png", "/e.png", "/f.png"].map(PathBuf::from));
        assert_eq!(files.skipped, 9);
    }

    #[test]
    fn this_computers_own_name_is_this_computer() {
        let list = "file://chapel/a.png\r\nfile://CHAPEL/b.png\r\nfile://vestry/c.png\r\n";
        let files = local_files(list.as_bytes(), Some("chapel"));
        assert_eq!(files, Files { paths: vec![PathBuf::from("/a.png"), PathBuf::from("/b.png")], skipped: 1 });
        assert_eq!(local_files(list.as_bytes(), None).skipped, 3);
    }

    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_utf8_is_kept_as_it_is() {
        use std::os::unix::ffi::OsStrExt;
        let files = local_files(b"file:///old/caf%E9.png\r\n", None);
        assert_eq!(files.paths[0].as_os_str().as_bytes(), b"/old/caf\xE9.png");
    }
}
