//! Takes what identifies a person or a computer out of text that is about to be shared: a log
//! line, a crash message, a path. The home folder becomes `~` and the account's name goes from
//! every path (`C:\Users\Jane Smith\Music` and `/Users/jane/Music` both become `~\Music` and
//! `~/Music`), addresses on the network, e-mail addresses, the account and the query of a web
//! address, the value after a word such as `password` or `token`, and long opaque strings that
//! look like keys are replaced by a short word in angle brackets.
//!
//! It works on one piece of text at a time with the standard library only, and never fails: text
//! it does not understand is passed through (the other passes still run).

/// Who is asking: the home folder and the account name to take out of everything.
#[derive(Clone, Debug, Default)]
pub struct Scrubber {
    home: Option<String>,
    user: Option<String>,
}

impl Scrubber {
    /// For this home folder and account name (either may be unknown).
    pub fn new(home: Option<&str>, user: Option<&str>) -> Scrubber {
        let clean = |text: &str| Some(text.trim().to_owned()).filter(|text| !text.is_empty());
        Scrubber {
            home: home.and_then(clean).map(|home| home.trim_end_matches(['/', '\\']).to_owned()),
            user: user.and_then(clean),
        }
    }

    /// For the person running this program, as the system names them.
    pub fn from_env() -> Scrubber {
        let var = |name: &str| std::env::var(name).ok();
        let home = var("HOME").or_else(|| var("USERPROFILE"));
        let user = var("USER").or_else(|| var("USERNAME")).or_else(|| var("LOGNAME"));
        Scrubber::new(home.as_deref(), user.as_deref())
    }

    /// The text with everything that names the person or the computer taken out.
    pub fn scrub(&self, text: &str) -> String {
        let mut text = self.home_folder(text);
        text = user_folders(&text);
        text = self.account_name(&text);
        text = web_addresses(&text);
        text = email_addresses(&text);
        text = network_addresses(&text);
        text = secret_values(&text);
        long_tokens(&text)
    }

    /// The home folder, as written either way round and in any case, becomes `~`.
    fn home_folder(&self, text: &str) -> String {
        let Some(home) = self.home.as_deref().filter(|home| home.len() >= 3) else { return text.to_owned() };
        let forward = home.replace('\\', "/");
        let backward = home.replace('/', "\\");
        let mut text = text.to_owned();
        for form in [home.to_owned(), forward, backward] {
            text = replace_ignoring_case(&text, &form, "~", |_| true);
        }
        text
    }

    /// The account's name, wherever it stands as a word of its own ("Jane's MacBook").
    fn account_name(&self, text: &str) -> String {
        match self.user.as_deref().filter(|user| user.len() >= 3) {
            Some(user) => replace_ignoring_case(text, user, "<user>", |_| true),
            None => text.to_owned(),
        }
    }
}

/// A byte that is part of a word: a letter or digit, or part of a letter of another script.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte >= 0x80
}

/// Replaces every whole-word, case-insensitive (ASCII) occurrence of `needle`. A match on both
/// sides of which stands no word byte is replaced; `accept` may still refuse it.
fn replace_ignoring_case(text: &str, needle: &str, with: &str, accept: impl Fn(&str) -> bool) -> String {
    let (lower, find) = (text.to_ascii_lowercase(), needle.to_ascii_lowercase());
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&find) {
        let start = at + found;
        let end = start + find.len();
        // A needle that itself begins or ends with a separator (a folder) needs no word edge.
        let first_word = is_word(find.as_bytes()[0]);
        let last_word = is_word(find.as_bytes()[find.len() - 1]);
        let before_ok = !first_word || start == 0 || !is_word(bytes[start - 1]);
        let after_ok = !last_word || end == bytes.len() || !is_word(bytes[end]);
        out.push_str(&text[at..start]);
        if before_ok && after_ok && accept(&text[start..end]) {
            out.push_str(with);
        } else {
            out.push_str(&text[start..end]);
        }
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// `C:\Users\Jane Smith\Music` and `/Users/jane/Music` (and `/home/jane/…`) become `~\Music` and
/// `~/Music`, whoever's they are.
fn user_folders(text: &str) -> String {
    const MARKS: [&str; 3] = ["/users/", "\\users\\", "/home/"];
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        // The first mark from here.
        let next = MARKS.iter().filter_map(|mark| lower[at..].find(mark).map(|i| (at + i, mark.len()))).min();
        let Some((mark_at, len)) = next else { break };
        // Where the path begins: a drive letter, and a `\\?\` before it, belong to it.
        let mut start = mark_at;
        if start >= 2 && bytes[start - 2].is_ascii_alphabetic() && bytes[start - 1] == b':' {
            start -= 2;
            if start >= 4 && &text[start - 4..start] == "\\\\?\\" {
                start -= 4;
            }
        } else if start > 0 && is_word(bytes[start - 1]) {
            // `/srv/home/…` is not a home folder.
            out.push_str(&text[at..mark_at + 1]);
            at = mark_at + 1;
            continue;
        }
        if start < at {
            start = at;
        }
        // The account's folder: to the next separator, quote or bracket; a name running to the
        // end of the words ends at a space.
        let name_start = mark_at + len;
        let stops =
            |byte: u8| matches!(byte, b'/' | b'\\' | b'"' | b'\'' | b'`' | b'\n' | b'\r' | b')' | b']' | b'>' | b',');
        let mut end = name_start;
        while end < bytes.len() && !stops(bytes[end]) {
            end += 1;
        }
        if end == bytes.len() || !matches!(bytes[end], b'/' | b'\\') {
            let words = &text[name_start..end];
            end = name_start + words.find(char::is_whitespace).unwrap_or(words.len());
        }
        out.push_str(&text[at..start]);
        out.push('~');
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// A web address keeps its scheme and its host only: the account in it, its path (which may be
/// a private link's secret) and its query go. `https://user:pw@host.example/a/b?x=1` becomes
/// `https://host.example/…`.
fn web_addresses(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = text[at..].find("://") {
        let colon = at + found;
        // The scheme: letters before the colon.
        let mut scheme = colon;
        while scheme > at
            && (bytes[scheme - 1].is_ascii_alphabetic() || matches!(bytes[scheme - 1], b'+' | b'-' | b'.'))
        {
            scheme -= 1;
        }
        let mut end = colon + 3;
        while end < bytes.len()
            && !bytes[end].is_ascii_whitespace()
            && !matches!(bytes[end], b'"' | b'\'' | b'<' | b'>' | b')' | b'`')
        {
            end += 1;
        }
        if scheme == colon {
            out.push_str(&text[at..colon + 3]);
            at = colon + 3;
            continue;
        }
        let rest = &text[colon + 3..end];
        let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
        out.push_str(&text[at..colon + 3]);
        out.push_str(host);
        if authority_end < rest.len() {
            out.push_str("/…");
        }
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// `name@host.example` becomes `<email>`.
fn email_addresses(text: &str) -> String {
    let bytes = text.as_bytes();
    let local = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'%' | b'+' | b'-');
    let domain = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-');
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'@' || index < at {
            continue;
        }
        let mut start = index;
        while start > at && local(bytes[start - 1]) {
            start -= 1;
        }
        let mut end = index + 1;
        while end < bytes.len() && domain(bytes[end]) {
            end += 1;
        }
        while end > index + 1 && bytes[end - 1] == b'.' {
            end -= 1;
        }
        if start < index && text[index + 1..end].contains('.') {
            out.push_str(&text[at..start]);
            out.push_str("<email>");
            at = end;
        }
    }
    out.push_str(&text[at..]);
    out
}

/// Whether this is four numbers up to 255 with dots between.
fn is_ipv4(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| !part.is_empty() && part.len() <= 3 && part.bytes().all(|b| b.is_ascii_digit()))
        && parts.iter().all(|part| part.parse::<u16>().is_ok_and(|n| n <= 255))
}

/// Addresses on the network (IPv4 with or without a port, IPv6, and the hardware addresses that
/// look like it) become `<ip>`.
fn network_addresses(text: &str) -> String {
    let bytes = text.as_bytes();
    let part = |byte: u8| byte.is_ascii_hexdigit() || matches!(byte, b':' | b'.');
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    let mut index = 0;
    while index < bytes.len() {
        if !part(bytes[index]) || (index > 0 && is_word(bytes[index - 1]) && !part(bytes[index - 1])) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && part(bytes[index]) {
            index += 1;
        }
        let mut end = index;
        while end > start && matches!(bytes[end - 1], b'.' | b':') {
            end -= 1;
        }
        let run = &text[start..end];
        let colons = run.matches(':').count();
        let ipv6 = (run.contains("::") || colons >= 5) && run.bytes().any(|b| b.is_ascii_hexdigit());
        let ipv4 = match colons {
            0 => is_ipv4(run),
            1 => run.split_once(':').is_some_and(|(ip, port)| {
                is_ipv4(ip) && !port.is_empty() && port.len() <= 5 && port.bytes().all(|b| b.is_ascii_digit())
            }),
            _ => false,
        };
        let edge_before = start == 0 || !is_word(bytes[start - 1]);
        let edge_after = end == bytes.len() || !is_word(bytes[end]);
        if (ipv4 || ipv6) && edge_before && edge_after {
            out.push_str(&text[at..start]);
            out.push_str("<ip>");
            at = end;
        }
    }
    out.push_str(&text[at..]);
    out
}

/// The words after which a value is a secret.
const SECRET_WORDS: [&str; 11] = [
    "password",
    "passphrase",
    "passwd",
    "secret",
    "token",
    "apikey",
    "api_key",
    "api-key",
    "authorization",
    "cookie",
    "key",
];

/// The value after `password:`, `token=`, `"secret": "…"` and the like, and the credential after
/// `Bearer` or `Basic`, becomes `<hidden>`.
fn secret_values(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    let mut index = 0;
    while index < bytes.len() {
        // A word starts here (not in the middle of one).
        if !bytes[index].is_ascii_alphabetic() || (index > 0 && bytes[index - 1].is_ascii_alphabetic()) {
            index += 1;
            continue;
        }
        let word =
            SECRET_WORDS.iter().chain(["bearer", "basic"].iter()).find(|word| lower[index..].starts_with(**word));
        let Some(word) = word else {
            index += 1;
            continue;
        };
        let after_word = index + word.len();
        let mut at_value = after_word;
        let credential = matches!(*word, "bearer" | "basic");
        if credential {
            // The credential follows after a space.
            if bytes.get(at_value) != Some(&b' ') {
                index = after_word;
                continue;
            }
        } else {
            // An optional closing quote, spaces, then : or =.
            if matches!(bytes.get(at_value), Some(b'"' | b'\'')) {
                at_value += 1;
            }
            while bytes.get(at_value) == Some(&b' ') {
                at_value += 1;
            }
            if !matches!(bytes.get(at_value), Some(b':' | b'=')) {
                index = after_word;
                continue;
            }
            at_value += 1;
        }
        while matches!(bytes.get(at_value), Some(b' ')) {
            at_value += 1;
        }
        // The value: quoted, or up to the next break.
        let (value_start, value_end) = match bytes.get(at_value) {
            Some(&quote @ (b'"' | b'\'')) => {
                let from = at_value + 1;
                (from, text[from..].find(quote as char).map_or(bytes.len(), |i| from + i))
            }
            _ => {
                let mut end = at_value;
                while end < bytes.len()
                    && !bytes[end].is_ascii_whitespace()
                    && !matches!(bytes[end], b',' | b';' | b'&' | b')' | b'}' | b']' | b'"' | b'\'')
                {
                    end += 1;
                }
                (at_value, end)
            }
        };
        // `Authorization: Bearer …`: the credential is the word after.
        if !credential && matches!(&lower[value_start..value_end], "bearer" | "basic") {
            index = value_start;
            continue;
        }
        let long_enough = !credential || value_end - value_start >= 12;
        if long_enough && value_end > value_start && &text[value_start..value_end] != "<hidden>" {
            out.push_str(&text[at..value_start]);
            out.push_str("<hidden>");
            at = value_end;
            index = value_end;
        } else {
            index = after_word;
        }
    }
    out.push_str(&text[at..]);
    out
}

/// Whether this is the shape of a UUID (an id, not a key).
fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text
            .bytes()
            .enumerate()
            .all(|(at, byte)| if matches!(at, 8 | 13 | 18 | 23) { byte == b'-' } else { byte.is_ascii_hexdigit() })
}

/// A run of 32 or more letters, digits and `_-+=` with both letters and digits in it looks like
/// a key or a token, and becomes `<hidden>`. A UUID, a name in a path and a file name are left.
fn long_tokens(text: &str) -> String {
    let bytes = text.as_bytes();
    let token = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+' | b'=');
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    let mut index = 0;
    while index < bytes.len() {
        if !token(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && token(bytes[index]) {
            index += 1;
        }
        let run = &text[start..index];
        let in_path = start > 0 && matches!(bytes[start - 1], b'/' | b'\\');
        let file_name = bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_alphanumeric);
        if run.len() >= 32
            && run.bytes().any(|b| b.is_ascii_digit())
            && run.bytes().any(|b| b.is_ascii_alphabetic())
            && !is_uuid(run)
            && !in_path
            && !file_name
        {
            out.push_str(&text[at..start]);
            out.push_str("<hidden>");
            at = index;
        }
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jane_on_mac() -> Scrubber {
        Scrubber::new(Some("/Users/janesmith"), Some("janesmith"))
    }

    fn jane_on_windows() -> Scrubber {
        Scrubber::new(Some("C:\\Users\\Jane Smith"), Some("Jane Smith"))
    }

    #[test]
    fn a_mac_home_folder_becomes_a_tilde() {
        let said = jane_on_mac().scrub("read /Users/janesmith/Documents/SoftPresenter/Songs/Grace.song in 3 ms");
        assert_eq!(said, "read ~/Documents/SoftPresenter/Songs/Grace.song in 3 ms");
        // The Library log folder and a program's own source path.
        let said =
            jane_on_mac().scrub("/Users/janesmith/.cargo/registry/src/index.crates.io-1949/wgpu-29/src/lib.rs:41");
        assert_eq!(said, "~/.cargo/registry/src/index.crates.io-1949/wgpu-29/src/lib.rs:41");
    }

    #[test]
    fn a_windows_home_folder_with_a_space_in_the_name_becomes_a_tilde() {
        let said = jane_on_windows().scrub("added C:\\Users\\Jane Smith\\Music\\Hymns\\a.mp3 in 3ms");
        assert_eq!(said, "added ~\\Music\\Hymns\\a.mp3 in 3ms");
        // Written the other way round, or in other letters.
        let said = jane_on_windows().scrub("c:/users/jane smith/AppData/Roaming/SoftPresenter/SoftPresenter.log");
        assert_eq!(said, "~/AppData/Roaming/SoftPresenter/SoftPresenter.log");
        let said = jane_on_windows().scrub("C:\\USERS\\JANE SMITH\\Desktop");
        assert_eq!(said, "~\\Desktop");
    }

    #[test]
    fn the_account_is_taken_out_of_paths_whoever_it_is_and_out_of_other_words() {
        // Another account's folder, or one the program was not told of (a crash on a friend's
        // computer names the friend's account in the registry paths of its libraries).
        let nobody = Scrubber::default();
        assert_eq!(nobody.scrub("C:\\Users\\pastor\\Pictures\\a.png"), "~\\Pictures\\a.png");
        assert_eq!(nobody.scrub("\\\\?\\C:\\Users\\pastor\\Pictures\\a.png"), "~\\Pictures\\a.png");
        assert_eq!(nobody.scrub("/home/pastor/.config/x"), "~/.config/x");
        assert_eq!(nobody.scrub("in /Users/pastor"), "in ~");
        assert_eq!(nobody.scrub("opened \"/Users/pastor/a b.pdf\" ok"), "opened \"~/a b.pdf\" ok");
        // Not a home folder.
        assert_eq!(nobody.scrub("/srv/home/data/x"), "/srv/home/data/x");
        // The name on its own, as a word.
        assert_eq!(jane_on_mac().scrub("Janesmith's MacBook Pro"), "<user>'s MacBook Pro");
        assert_eq!(jane_on_mac().scrub("janesmithing"), "janesmithing");
    }

    #[test]
    fn addresses_on_the_network_are_taken_out() {
        let none = Scrubber::default();
        assert_eq!(none.scrub("A wrong password from 192.168.1.23."), "A wrong password from <ip>.");
        assert_eq!(none.scrub("listening on 10.0.0.5:8443"), "listening on <ip>");
        assert_eq!(none.scrub("from fe80::1c2d:3e4f:5a6b:7c8d%en0 and ::1"), "from <ip>%en0 and <ip>");
        assert_eq!(none.scrub("mac aa:bb:cc:dd:ee:ff"), "mac <ip>");
        // Versions and times are not addresses.
        assert_eq!(none.scrub("driver 31.0.15.4601 at 10:30:00"), "driver 31.0.15.4601 at 10:30:00");
        assert_eq!(none.scrub("v1.2.3.4"), "v1.2.3.4");
    }

    #[test]
    fn web_addresses_keep_only_the_host() {
        let none = Scrubber::default();
        assert_eq!(
            none.scrub("opened https://user:pw@voxlive.example/listen/grace/screen/abc2345?x=1#top now"),
            "opened https://voxlive.example/… now"
        );
        assert_eq!(none.scrub("(https://example.com)"), "(https://example.com)");
        assert_eq!(none.scrub("file:///Users/pastor/a.pdf"), "file://~/…");
    }

    #[test]
    fn email_addresses_are_taken_out() {
        let none = Scrubber::default();
        assert_eq!(none.scrub("sign-in for pastor.jim+church@example.org failed."), "sign-in for <email> failed.");
        assert_eq!(none.scrub("@home and a@b"), "@home and a@b");
    }

    #[test]
    fn what_follows_a_secret_word_is_hidden() {
        let none = Scrubber::default();
        assert_eq!(none.scrub("password: \"hunter2 two\" ok"), "password: \"<hidden>\" ok");
        assert_eq!(none.scrub("{\"client_secret\":\"abcDEF\",\"id\":3}"), "{\"client_secret\":\"<hidden>\",\"id\":3}");
        assert_eq!(none.scrub("Authorization: Bearer abc.def.ghi.jkl.mno"), "Authorization: Bearer <hidden>");
        assert_eq!(none.scrub("sent Bearer abc.def.ghi.jkl.mno to the server"), "sent Bearer <hidden> to the server");
        assert_eq!(none.scrub("?token=abc123&x=1"), "?token=<hidden>&x=1");
        assert_eq!(none.scrub("api_key = sk-123"), "api_key = <hidden>");
        // Words that only mention one.
        assert_eq!(none.scrub("A wrong password from here."), "A wrong password from here.");
        assert_eq!(none.scrub("a new password was set"), "a new password was set");
    }

    #[test]
    fn long_opaque_strings_are_hidden_but_ids_and_file_names_are_not() {
        let none = Scrubber::default();
        let key = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0";
        assert_eq!(none.scrub(&format!("saved {key} to the keychain")), "saved <hidden> to the keychain");
        let id = "6f1c2d3e-4a5b-4c6d-8e7f-0a1b2c3d4e5f";
        assert_eq!(none.scrub(&format!("song {id}")), format!("song {id}"));
        assert_eq!(
            none.scrub("Sunday_Morning_Service_Opening_Video_Final_2026.mp4"),
            "Sunday_Morning_Service_Opening_Video_Final_2026.mp4"
        );
        assert_eq!(
            none.scrub("~/x/Sunday_Morning_Service_Opening_Video_Final_2026"),
            "~/x/Sunday_Morning_Service_Opening_Video_Final_2026"
        );
    }

    #[test]
    fn ordinary_lines_and_other_scripts_pass_through_untouched() {
        let none = Scrubber::default();
        for line in [
            "NVIDIA GeForce RTX 3060 (Vulkan, DiscreteGpu, driver 551.23)",
            "Switched on, on port 8443.",
            "Песня «Тихая ночь» 3 слайда",
            "live: Amazing Grace slide 2 (Verse 1)",
        ] {
            assert_eq!(none.scrub(line), line);
        }
    }
}
