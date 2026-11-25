//! The control channel's requests: reading one off a connection, and writing its answer.
//!
//! A request looks like HTTP, but its first line may end `RTSP/1.0`, which an HTTP parser
//! refuses; so the first line is read here and the header lines are given to `httparse`.
//! Every length is checked against a limit before anything is kept or allocated.
//!
//! Follows UxPlay's `lib/raop.c`, `lib/http_request.c` and `lib/http_response.c` (copyright (C)
//! 2011-2012 Juho Vähä-Herttua, (C) 2021-2026 F. Duncanh; LGPL-2.1-or-later).

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

/// The most a request's first line and headers may take together.
pub const MAX_HEAD: usize = 8 * 1024;
/// The most header lines a request may have.
pub const MAX_HEADERS: usize = 32;
/// The most a request's body may take.
pub const MAX_BODY: usize = 64 * 1024;
/// How long a request may take to arrive once its first byte has.
pub const REQUEST_TIME: Duration = Duration::from_secs(10);

/// One request of the control channel.
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub url: String,
    /// `RTSP/1.0` or `HTTP/1.1`.
    pub protocol: String,
    /// The `CSeq` header, when it is a number: echoed in the answer.
    pub cseq: Option<u32>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Why a connection's next request could not be read.
#[derive(Debug)]
pub enum Bad {
    /// The connection closed between requests.
    Closed,
    /// Nothing came for longer than the connection is given.
    Quiet,
    /// The request took too long to arrive, was too big, or was not a request.
    Refused(&'static str),
    Io(io::ErrorKind),
}

fn find_head_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n").map(|at| at + 4)
}

fn token(text: &str, max: usize) -> bool {
    !text.is_empty() && text.len() <= max && text.bytes().all(|byte| (0x21..0x7f).contains(&byte))
}

/// What the head of a request (its first line and headers, with the empty line) says, and how
/// long the body after it is.
pub fn parse_head(head: &[u8]) -> Result<(Request, usize), &'static str> {
    let line_end = head.windows(2).position(|window| window == b"\r\n").ok_or("no request line")?;
    let line = std::str::from_utf8(&head[..line_end]).map_err(|_| "the request line is not text")?;
    let mut parts = line.split(' ');
    let (Some(method), Some(url), Some(protocol), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("the request line is not three words");
    };
    if !token(method, 32) || !method.bytes().all(|byte| byte.is_ascii_uppercase() || byte == b'_') {
        return Err("not a method");
    }
    if !token(url, 1024) {
        return Err("not an address");
    }
    if protocol != "RTSP/1.0" && protocol != "HTTP/1.1" {
        return Err("neither RTSP/1.0 nor HTTP/1.1");
    }
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let parsed = httparse::parse_headers(&head[line_end + 2..], &mut headers).map_err(|_| "bad header lines")?;
    let httparse::Status::Complete((_, headers)) = parsed else {
        return Err("the header lines do not end");
    };
    let mut request = Request {
        method: method.to_owned(),
        url: url.to_owned(),
        protocol: protocol.to_owned(),
        cseq: None,
        content_type: None,
        body: Vec::new(),
    };
    let mut length = 0_usize;
    for header in headers {
        let value = std::str::from_utf8(header.value).map_err(|_| "a header is not text")?.trim();
        if header.name.eq_ignore_ascii_case("Content-Length") {
            length = value.parse().map_err(|_| "the body's length is not a number")?;
            if length > MAX_BODY {
                return Err("the body is too long");
            }
        } else if header.name.eq_ignore_ascii_case("CSeq") {
            request.cseq = Some(value.parse().map_err(|_| "CSeq is not a number")?);
        } else if header.name.eq_ignore_ascii_case("Content-Type") {
            if value.len() > 128 {
                return Err("the content type is too long");
            }
            request.content_type = Some(value.to_owned());
        }
    }
    Ok((request, length))
}

/// Reads the connection's next request. `pending` holds what was read past the last one. The
/// reader's own time limit (a socket's read timeout) bounds each wait; `quiet` is how long the
/// connection may say nothing at all before a request starts.
pub fn read_request<R: Read>(reader: &mut R, pending: &mut Vec<u8>, quiet: Duration) -> Result<Request, Bad> {
    let waiting_since = Instant::now();
    let mut started = if pending.is_empty() { None } else { Some(Instant::now()) };
    let mut chunk = [0_u8; 4096];
    let mut fill = |pending: &mut Vec<u8>, started: &mut Option<Instant>, room: usize| -> Result<(), Bad> {
        loop {
            match started {
                Some(since) if since.elapsed() > REQUEST_TIME => return Err(Bad::Refused("the request took too long")),
                None if waiting_since.elapsed() > quiet => return Err(Bad::Quiet),
                _ => {}
            }
            let want = room.min(chunk.len());
            match reader.read(&mut chunk[..want]) {
                Ok(0) => {
                    return Err(if pending.is_empty() { Bad::Closed } else { Bad::Refused("it ended part way") });
                }
                Ok(n) => {
                    started.get_or_insert_with(Instant::now);
                    pending.extend_from_slice(&chunk[..n]);
                    return Ok(());
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(Bad::Io(error.kind())),
            }
        }
    };
    let head_end = loop {
        if let Some(end) = find_head_end(pending) {
            break end;
        }
        if pending.len() >= MAX_HEAD {
            return Err(Bad::Refused("the headers are too long"));
        }
        let room = MAX_HEAD - pending.len();
        fill(pending, &mut started, room)?;
    };
    if head_end > MAX_HEAD {
        return Err(Bad::Refused("the headers are too long"));
    }
    let (mut request, length) = parse_head(&pending[..head_end]).map_err(Bad::Refused)?;
    while pending.len() < head_end + length {
        let room = head_end + length - pending.len();
        fill(pending, &mut started, room)?;
    }
    request.body = pending[head_end..head_end + length].to_vec();
    pending.drain(..head_end + length);
    Ok(request)
}

/// An answer to a request.
pub struct Response {
    code: u16,
    words: &'static str,
    headers: Vec<(&'static str, String)>,
    pub content_type: Option<&'static str>,
    pub body: Vec<u8>,
    /// The connection is closed once this has been sent.
    pub close: bool,
}

impl Response {
    pub fn ok() -> Response {
        Response::status(200, "OK")
    }

    pub fn status(code: u16, words: &'static str) -> Response {
        Response { code, words, headers: Vec::new(), content_type: None, body: Vec::new(), close: false }
    }

    pub fn with_body(content_type: &'static str, body: Vec<u8>) -> Response {
        Response { content_type: Some(content_type), body, ..Response::ok() }
    }

    pub fn header(mut self, name: &'static str, value: impl Into<String>) -> Response {
        self.headers.push((name, value.into()));
        self
    }

    pub fn closing(mut self) -> Response {
        self.close = true;
        self
    }

    pub fn code(&self) -> u16 {
        self.code
    }

    /// The bytes to send: the status line in the request's own protocol, the headers, the
    /// server's name, the request's `CSeq`, and the body with its length.
    pub fn to_bytes(&self, protocol: &str, cseq: Option<u32>) -> Vec<u8> {
        let mut out = Vec::with_capacity(256 + self.body.len());
        let _ = write!(out, "{protocol} {} {}\r\n", self.code, self.words);
        for (name, value) in &self.headers {
            let _ = write!(out, "{name}: {value}\r\n");
        }
        if let Some(content_type) = self.content_type {
            let _ = write!(out, "Content-Type: {content_type}\r\n");
        }
        let _ = write!(out, "Server: AirTunes/{}\r\n", crate::discovery::SOURCE_VERSION);
        if let Some(cseq) = cseq {
            let _ = write!(out, "CSeq: {cseq}\r\n");
        }
        if !self.body.is_empty() || self.content_type.is_some() {
            let _ = write!(out, "Content-Length: {}\r\n", self.body.len());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUIET: Duration = Duration::from_secs(1);

    fn read(bytes: &[u8]) -> Result<Request, Bad> {
        read_request(&mut io::Cursor::new(bytes.to_vec()), &mut Vec::new(), QUIET)
    }

    #[test]
    fn a_request_with_an_rtsp_line_and_a_body_is_read() {
        let request = read(
            b"POST /fp-setup RTSP/1.0\r\nContent-Length: 4\r\nContent-Type: application/octet-stream\r\nCSeq: 7\r\n\r\nabcd",
        )
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "/fp-setup");
        assert_eq!(request.protocol, "RTSP/1.0");
        assert_eq!(request.cseq, Some(7));
        assert_eq!(request.content_type.as_deref(), Some("application/octet-stream"));
        assert_eq!(request.body, b"abcd");
    }

    #[test]
    fn two_requests_sent_together_are_read_one_after_the_other() {
        let bytes = b"GET /info RTSP/1.0\r\nCSeq: 1\r\n\r\nSETUP rtsp://1.2.3.4/5 RTSP/1.0\r\nCSeq: 2\r\nContent-Length: 2\r\n\r\nhiX";
        let mut reader = io::Cursor::new(bytes.to_vec());
        let mut pending = Vec::new();
        let first = read_request(&mut reader, &mut pending, QUIET).unwrap();
        assert_eq!((first.method.as_str(), first.cseq), ("GET", Some(1)));
        let second = read_request(&mut reader, &mut pending, QUIET).unwrap();
        assert_eq!((second.method.as_str(), second.body.as_slice()), ("SETUP", b"hi".as_slice()));
        assert_eq!(pending, b"X");
    }

    #[test]
    fn a_closed_connection_between_requests_is_not_an_error_in_a_request() {
        assert!(matches!(read(b""), Err(Bad::Closed)));
        assert!(matches!(read(b"GET /info RTSP/1.0\r\nCSeq"), Err(Bad::Refused("it ended part way"))));
        assert!(matches!(read(b"GET /info RTSP/1.0\r\nContent-Length: 9\r\n\r\nabc"), Err(Bad::Refused(_))));
    }

    #[test]
    fn what_is_too_long_or_not_a_request_is_refused_before_it_is_kept() {
        let mut long = b"GET /info RTSP/1.0\r\n".to_vec();
        long.extend(std::iter::repeat_n(b'a', MAX_HEAD + 100));
        assert!(matches!(read(&long), Err(Bad::Refused("the headers are too long"))));
        assert!(matches!(
            read(b"POST /x RTSP/1.0\r\nContent-Length: 65537\r\n\r\n"),
            Err(Bad::Refused("the body is too long"))
        ));
        assert!(matches!(
            read(b"POST /x RTSP/1.0\r\nContent-Length: 99999999999999999999\r\n\r\n"),
            Err(Bad::Refused(_))
        ));
        assert!(matches!(read(b"POST /x RTSP/1.0\r\nContent-Length: -1\r\n\r\n"), Err(Bad::Refused(_))));
        let mut many = b"GET /info RTSP/1.0\r\n".to_vec();
        for i in 0..=MAX_HEADERS {
            many.extend(format!("H{i}: x\r\n").bytes());
        }
        many.extend(b"\r\n");
        assert!(matches!(read(&many), Err(Bad::Refused(_))));
        for bad in [
            &b"GET /info SMTP/1.0\r\n\r\n"[..],
            b"GET  /info RTSP/1.0\r\n\r\n",
            b"GET /info RTSP/1.0 extra\r\n\r\n",
            b"get /info RTSP/1.0\r\n\r\n",
            b"\r\n\r\n",
            b"GET /in\x00fo RTSP/1.0\r\n\r\n",
            b"GET /info RTSP/1.0\r\nCSeq: -4\r\n\r\n",
            b"GET /info RTSP/1.0\r\nno colon here\r\n\r\n",
            b"\xff\xfe /info RTSP/1.0\r\n\r\n",
        ] {
            assert!(matches!(read(bad), Err(Bad::Refused(_))), "{bad:?}");
        }
    }

    /// Any bytes at all end in a request or a refusal, never a panic or an endless wait.
    #[test]
    fn random_bytes_never_stop_the_reader() {
        let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let good = b"POST /fp-setup RTSP/1.0\r\nContent-Length: 4\r\nCSeq: 7\r\n\r\nabcd";
        for round in 0..3000 {
            let mut bytes = if round % 2 == 0 {
                (0..(next() % 300) as usize).map(|_| next() as u8).collect::<Vec<u8>>()
            } else {
                // A good request with a few bytes changed, or cut short.
                let mut bytes = good.to_vec();
                for _ in 0..(next() % 4) {
                    let at = (next() % bytes.len() as u64) as usize;
                    bytes[at] = next() as u8;
                }
                bytes.truncate((next() % (bytes.len() as u64 + 1)) as usize);
                bytes
            };
            if round % 7 == 0 {
                bytes.extend(b"\r\n\r\n");
            }
            let _ = read(&bytes);
        }
    }

    /// A reader that gives a byte at a time and then says "not yet" for ever.
    struct Trickle(Vec<u8>, usize);
    impl Read for Trickle {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.1 < self.0.len() {
                buffer[0] = self.0[self.1];
                self.1 += 1;
                Ok(1)
            } else {
                std::thread::sleep(Duration::from_millis(5));
                Err(io::ErrorKind::WouldBlock.into())
            }
        }
    }

    #[test]
    fn a_connection_that_says_nothing_is_given_up_after_its_time() {
        let started = Instant::now();
        let result = read_request(&mut Trickle(Vec::new(), 0), &mut Vec::new(), Duration::from_millis(50));
        assert!(matches!(result, Err(Bad::Quiet)));
        assert!(started.elapsed() < Duration::from_secs(2));
        // A byte at a time is still a request.
        let bytes = b"GET /info RTSP/1.0\r\nCSeq: 3\r\n\r\n".to_vec();
        let request = read_request(&mut Trickle(bytes, 0), &mut Vec::new(), QUIET).unwrap();
        assert_eq!(request.cseq, Some(3));
    }

    #[test]
    fn an_answer_carries_the_cseq_and_the_length_of_its_body() {
        let bytes = Response::with_body("application/octet-stream", vec![1, 2, 3]).to_bytes("RTSP/1.0", Some(9));
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("RTSP/1.0 200 OK\r\n"));
        assert!(text.contains("CSeq: 9\r\n"));
        assert!(text.contains("Content-Length: 3\r\n\r\n"));
        assert!(bytes.ends_with(&[1, 2, 3]));
        let plain = Response::ok().to_bytes("RTSP/1.0", None);
        let text = String::from_utf8_lossy(&plain);
        assert!(!text.contains("Content-Length"));
        assert!(!text.contains("CSeq"));
        assert!(text.ends_with("\r\n\r\n"));
        assert_eq!(Response::status(401, "Unauthorized").code(), 401);
    }
}
