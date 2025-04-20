use core::ops::Range;
use std::str;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

pub mod response;

pub struct Reader<R> {
    reader: R,
    limit: Option<usize>,
    count: usize,

    pub content_length: Option<u64>,
}

impl<R: BytePeekRead> Reader<R> {
    pub fn new(reader: R, limit: Option<usize>) -> Self {
        Self {
            reader,
            limit,
            count: 0,
            content_length: None,
        }
    }

    pub fn done(self) -> R {
        self.reader
    }

    fn ranger(&mut self, limit: usize) -> Ranger<R> {
        let limit = self.limit.map_or(limit, |global_limit| {
            limit.min(global_limit.checked_sub(self.count).unwrap_or(0))
        });
        Ranger::new(&mut self.reader, limit)
    }

    pub async fn request_line(&mut self, limit: usize) -> Result<RequestLine> {
        let mut ranger = self.ranger(limit);

        let parts: [Range<usize>; 3] = [
            ranger.range_to_and_skip_sp(b' ').await?,
            ranger.range_to_and_skip_sp(b' ').await?,
            ranger.range_to(b'\r').await?,
        ];
        ranger.expect(b'\n').await?;

        let raw = ranger.done();
        self.count += raw.len();
        Ok(RequestLine(LineParts { raw, parts }))
    }

    pub async fn status_line(&mut self, limit: usize) -> Result<StatusLine> {
        let mut ranger = self.ranger(limit);

        let parts: [Range<usize>; 2] = [
            ranger.range_to_and_skip_sp(b' ').await?,
            ranger.range_to(b'\r').await?,
        ];
        ranger.expect(b'\n').await?;

        let raw = ranger.done();
        self.count += raw.len();
        Ok(StatusLine(LineParts { raw, parts }))
    }

    pub async fn header(&mut self, limit: usize) -> Result<HeaderRead> {
        let mut ranger = self.ranger(limit);

        if ranger.peek().await? == b'\r' {
            ranger.read().await?;
            ranger.expect(b'\n').await?;
            return Ok(HeaderRead::EndOfHeader);
        }

        let name = ranger.range_to_and_skip_sp(b':').await?;

        ranger.start_range();
        let value = loop {
            let r = ranger.to(b'\r').await?;
            ranger.expect(b'\n').await?;
            if !ranger.next_is_sp().await? {
                break r;
            }
        };

        let raw = ranger.done();
        self.count += raw.len();

        let hdr = Header(LineParts {
            raw,
            parts: [name, value],
        });

        // parse headers we care about
        let name = hdr.name();
        if b"Content-Length".eq_ignore_ascii_case(name) {
            let s = str::from_utf8(hdr.value()).map_err(|_| Error::InvalidInput)?;
            self.content_length = Some(s.parse().map_err(|_| Error::InvalidInput)?);
        }

        // all good!
        Ok(HeaderRead::Header(hdr))
    }
}

pub struct Writer(Vec<u8>);
impl Writer {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
    pub fn done(mut self) -> Vec<u8> {
        self.crlf();
        self.0
    }

    pub fn append(&mut self, mut v: Vec<u8>) {
        self.0.append(&mut v);
    }
    pub fn append_slice(&mut self, v: &[u8]) {
        self.0.extend_from_slice(v);
    }
    pub fn append_string(&mut self, v: String) {
        self.0.append(&mut v.into_bytes());
    }
    pub fn append_str(&mut self, v: &str) {
        self.0.extend_from_slice(v.as_bytes());
    }

    pub fn crlf(&mut self) {
        self.append_slice(b"\r\n");
    }

    pub fn header(&mut self, name: &str, value: &str) {
        let name = name.as_bytes();
        let value = value.as_bytes();

        self.0.reserve(name.len() + 2 + value.len() + 2);

        self.append_slice(name);
        self.append_slice(b": ");
        self.append_slice(value);
        self.append_slice(b"\r\n");
    }

    pub fn content_length(mut self, length: usize) -> Vec<u8> {
        self.header("Content-Length", &length.to_string());
        self.crlf();
        self.0.reserve(length);
        self.0
    }
}

#[allow(async_fn_in_trait)]
pub trait BytePeekRead {
    async fn read_byte(&mut self) -> std::io::Result<u8>;
    async fn peek_byte(&mut self) -> std::io::Result<u8>;
}

impl<R: AsyncRead + Unpin> BytePeekRead for &mut BufReader<R> {
    async fn read_byte(&mut self) -> std::io::Result<u8> {
        self.read_u8().await
    }
    async fn peek_byte(&mut self) -> std::io::Result<u8> {
        self.fill_buf().await.map(|buf| buf[0])
    }
}

pub struct CopyingBytePeekRead<R, W> {
    reader: R,
    writer: W,
    read_buf: Vec<u8>,
}

impl<R, W: AsyncWrite + Unpin> CopyingBytePeekRead<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            read_buf: Vec::new(),
        }
    }

    pub async fn flush(&mut self) -> std::io::Result<()> {
        if !self.read_buf.is_empty() {
            self.writer.write_all(&self.read_buf).await?;
            self.read_buf.clear();
        }
        Ok(())
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> BytePeekRead
    for CopyingBytePeekRead<&mut BufReader<R>, W>
{
    async fn read_byte(&mut self) -> std::io::Result<u8> {
        let b = self.peek_byte().await?;
        self.reader.consume(1);
        Ok(b)
    }

    async fn peek_byte(&mut self) -> std::io::Result<u8> {
        let b = {
            let buf = self.reader.buffer();
            if buf.is_empty() {
                self.flush().await?;
                self.reader.fill_buf().await?[0]
            } else {
                buf[0]
            }
        };
        Ok(b)
    }
}

pub enum Error {
    IO(std::io::Error),
    LimitReached,
    InvalidInput,
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::IO(e)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IO(e) => write!(f, "io error: {e}"),
            Self::LimitReached => f.write_str("limit reached"),
            Self::InvalidInput => f.write_str("invalid input"),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

struct Ranger<'t, R> {
    raw: Vec<u8>,
    reader: &'t mut R,
    limit: usize,
    start: usize,
}
impl<'t, R: BytePeekRead> Ranger<'t, R> {
    fn new(reader: &'t mut R, limit: usize) -> Self {
        Self {
            raw: Vec::with_capacity(limit.min(4096 /* page size */)),
            reader,
            limit,
            start: 0,
        }
    }

    fn done(self) -> Vec<u8> {
        self.raw
    }

    async fn read(&mut self) -> Result<u8> {
        if self.raw.len() == self.limit {
            return Err(Error::LimitReached);
        }
        let b = self.reader.read_byte().await?;
        self.raw.push(b);
        Ok(b)
    }

    async fn peek(&mut self) -> Result<u8> {
        if self.raw.len() == self.limit {
            return Err(Error::LimitReached);
        }
        Ok(self.reader.peek_byte().await?)
    }

    fn start_range(&mut self) {
        self.start = self.raw.len();
    }

    async fn range_to(&mut self, search: u8) -> Result<Range<usize>> {
        self.start_range();
        self.to(search).await
    }

    async fn range_to_and_skip_sp(&mut self, search: u8) -> Result<Range<usize>> {
        let r = self.range_to(search).await?;
        self.skip_sp().await?;
        Ok(r)
    }

    async fn to(&mut self, sep: u8) -> Result<Range<usize>> {
        loop {
            match self.read().await? {
                b if b == sep => break,
                b'\r' | b'\n' => return Err(Error::InvalidInput),
                _ => {}
            }
        }
        Ok(self.start..self.raw.len() - 1) // range without the separator
    }

    async fn expect(&mut self, expected: u8) -> Result<()> {
        if self.read().await? != expected {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }

    async fn next_is_sp(&mut self) -> Result<bool> {
        Ok(match self.peek().await? {
            b' ' | b'\t' => true,
            _ => false,
        })
    }

    async fn skip_sp(&mut self) -> Result<()> {
        while self.next_is_sp().await? {
            self.read().await?;
        }
        Ok(())
    }
}

struct LineParts<const N: usize> {
    raw: Vec<u8>,
    parts: [Range<usize>; N],
}
impl<const N: usize> LineParts<N> {
    fn range(&self, i: usize) -> &[u8] {
        self.raw
            .get(self.parts[i].clone())
            .expect("range should be in line")
    }
}

pub struct RequestLine(LineParts<3>);
impl RequestLine {
    pub fn method(&self) -> &[u8] {
        self.0.range(0)
    }
    pub fn path(&self) -> &[u8] {
        self.0.range(1)
    }
    pub fn proto(&self) -> &[u8] {
        self.0.range(2)
    }
    pub fn into_raw(self) -> Vec<u8> {
        self.0.raw
    }
}

pub struct StatusLine(LineParts<2>);
impl StatusLine {
    pub fn proto(&self) -> &[u8] {
        self.0.range(0)
    }
    pub fn status(&self) -> &[u8] {
        self.0.range(1)
    }
    pub fn into_raw(self) -> Vec<u8> {
        self.0.raw
    }
}

pub struct Header(LineParts<2>);
impl Header {
    pub fn name(&self) -> &[u8] {
        self.0.range(0)
    }
    pub fn value(&self) -> &[u8] {
        self.0.range(1)
    }
    pub fn into_raw(self) -> Vec<u8> {
        self.0.raw
    }

    pub fn is(&self, name: &[u8]) -> bool {
        name.eq_ignore_ascii_case(self.name())
    }
    pub fn parse<T: str::FromStr>(&self) -> Result<T> {
        (str::from_utf8(self.value()).ok())
            .and_then(|s| s.parse().ok())
            .ok_or(Error::InvalidInput)
    }
}

pub enum HeaderRead {
    Header(Header),
    EndOfHeader,
}
impl HeaderRead {
    pub fn into_raw(self) -> Vec<u8> {
        match self {
            Self::Header(hdr) => hdr.into_raw(),
            Self::EndOfHeader => b"\r\n".into(),
        }
    }
}
