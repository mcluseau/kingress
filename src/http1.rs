use core::ops::Range;
use std::str;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

pub mod response;
pub mod status;

#[cfg(test)]
mod test;

const METHODS_WITHOUT_BODY: &[&[u8]] =
    &[b"GET", b"OPTIONS", b"HEAD", b"DELETE", b"CONNECT", b"TRACE"];

struct HeaderSummary {
    content_length: Option<u64>,
    transfer_encoding_is_chunked: bool,
    connection_is_close: bool,
}
impl HeaderSummary {
    fn new() -> Self {
        Self {
            content_length: None,
            transfer_encoding_is_chunked: false,
            connection_is_close: false,
        }
    }

    fn process_header(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        if name.eq_ignore_ascii_case(b"Content-Length") {
            let v = str::from_utf8(value).map_err(|_| Error::InvalidInput)?;
            let v = v.parse().map_err(|_| Error::InvalidInput)?;
            self.content_length = Some(v);
        } else if name.eq_ignore_ascii_case(b"Transfer-Encoding") {
            self.transfer_encoding_is_chunked = value.eq_ignore_ascii_case(b"chunked");
        } else if name.eq_ignore_ascii_case(b"Connection") {
            self.connection_is_close = value.eq_ignore_ascii_case(b"close");
        }
        Ok(())
    }

    fn request_length(&self, method: &[u8]) -> Option<u64> {
        if self.transfer_encoding_is_chunked {
            return None;
        }
        if let Some(length) = self.content_length {
            return Some(length);
        }

        if (METHODS_WITHOUT_BODY.iter()).any(|&m| method.eq_ignore_ascii_case(m)) {
            return Some(0);
        }

        if self.connection_is_close {
            return None;
        }
        Some(0)
    }

    fn response_length(&self, status_code: u16) -> Option<u64> {
        match status_code {
            101 => return None, // switching protocols
            100..=199 | 204 | 304 => return Some(0),
            _ => {}
        }
        if self.transfer_encoding_is_chunked {
            return None;
        }
        self.content_length
    }
}

enum HeaderKind {
    None,
    Request { method: Vec<u8> },
    Response { status_code: u16 },
}

pub struct Reader<'t, R> {
    reader: &'t mut R,
    limit: Option<usize>,
    count: usize,

    kind: HeaderKind,
    summary: HeaderSummary,
}

impl<'t, R: BytePeekRead> Reader<'t, R> {
    pub fn new(reader: &'t mut R, limit: Option<usize>) -> Self {
        Self {
            reader,
            limit,
            count: 0,
            kind: HeaderKind::None,
            summary: HeaderSummary::new(),
        }
    }

    fn ranger<S: RangerStorage + Default>(&mut self, limit: usize) -> Ranger<R, S> {
        let limit = self.limit.map_or(limit, |global_limit| {
            limit.min(global_limit.checked_sub(self.count).unwrap_or(0))
        });
        Ranger::new(self.reader, limit)
    }

    pub async fn request_line(&mut self, limit: usize) -> Result<RequestLine> {
        let mut ranger = self.ranger(limit);

        let parts: [Range<usize>; 3] = [
            ranger.range_to_and_skip_sp(b' ').await?,
            ranger.range_to_and_skip_sp(b' ').await?,
            ranger.range_to(b'\r').await?,
        ];
        ranger.expect(b'\n').await?;

        let (raw, count) = ranger.done();
        self.count += count;

        let rl = RequestLine(LineParts { raw, parts });

        let method = rl.method().to_vec();
        self.kind = HeaderKind::Request { method };

        Ok(rl)
    }

    pub async fn status_line(&mut self, limit: usize) -> Result<StatusLine> {
        let mut ranger = self.ranger(limit);

        let parts: [Range<usize>; 3] = [
            ranger.range_to_and_skip_sp(b' ').await?,
            ranger.range_to_and_skip_sp(b' ').await?,
            ranger.range_to(b'\r').await?,
        ];
        ranger.expect(b'\n').await?;

        let (raw, count) = ranger.done();
        self.count += count;

        let sl = StatusLine(LineParts { raw, parts });

        let status_code = str::from_utf8(sl.status_code()).map_err(|_| Error::InvalidInput)?;
        let status_code = status_code.parse().map_err(|_| Error::InvalidInput)?;
        self.kind = HeaderKind::Response { status_code };

        Ok(sl)
    }

    pub async fn header_name(&mut self) -> Result<HeaderRead> {
        let mut ranger = self.ranger::<Vec<u8>>(40);

        if ranger.peek().await? == b'\r' {
            ranger.read().await?;
            ranger.expect(b'\n').await?;
            return Ok(HeaderRead::EndOfHeader);
        }

        let name = ranger.range_to_and_skip_sp(b':').await?;

        let (mut raw, count) = ranger.done();
        self.count += count;

        raw.truncate(name.end);

        Ok(HeaderRead::Name(raw))
    }

    pub async fn header_value(&mut self, name: &[u8], limit: usize) -> Result<Vec<u8>> {
        let (raw, range) = self.read_header_value::<Vec<u8>>(limit).await?;
        let value = &raw[range];

        // parse some headers
        self.summary.process_header(name, value)?;

        Ok(value.into())
    }

    pub async fn skip_header_value(&mut self, limit: usize) -> Result<()> {
        self.read_header_value::<Discard>(limit).await?;
        Ok(())
    }

    async fn read_header_value<S: RangerStorage + Default>(
        &mut self,
        limit: usize,
    ) -> Result<(S, Range<usize>)> {
        let mut ranger = self.ranger(limit);

        ranger.skip_sp().await?;
        ranger.start_range();

        let value = loop {
            while ranger.peek().await? != b'\r' {
                ranger.read().await?;
            }
            let r = ranger.range();
            ranger.expect(b'\r').await?;
            ranger.expect(b'\n').await?;
            if ranger.next_is_sp().await? {
                continue;
            }
            break r;
        };

        let (storage, count) = ranger.done();
        self.count += count;

        Ok((storage, value))
    }

    pub fn content_length(&self) -> Option<u64> {
        match self.kind {
            HeaderKind::None => None,
            HeaderKind::Request { ref method } => self.summary.request_length(method),
            HeaderKind::Response { status_code } => self.summary.response_length(status_code),
        }
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

    pub fn status(&mut self, status: &[u8]) {
        const PREFIX: &[u8] = b"HTTP/1.1 ";
        self.0.reserve(PREFIX.len() + status.len() + 2);
        self.append_slice(PREFIX);
        self.append_slice(status);
        self.crlf();
    }

    pub fn header(&mut self, name: &str, value: &str) {
        self.header_raw(name.as_bytes(), value.as_bytes())
    }

    pub fn header_raw(&mut self, name: &[u8], value: &[u8]) {
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

    pub fn content(self, content: String) -> Vec<u8> {
        let mut content = content.into_bytes();
        let mut ret = self.content_length(content.len());
        ret.append(&mut content);
        ret
    }
}

#[allow(async_fn_in_trait)]
pub trait BytePeekRead {
    async fn read_byte(&mut self) -> std::io::Result<Option<u8>>;
    async fn peek_byte(&mut self) -> std::io::Result<Option<u8>>;
}

impl<R: AsyncRead + Unpin> BytePeekRead for BufReader<R> {
    async fn read_byte(&mut self) -> std::io::Result<Option<u8>> {
        let r = self.peek_byte().await;
        if let Ok(Some(_)) = r {
            self.consume(1);
        }
        r
    }
    async fn peek_byte(&mut self) -> std::io::Result<Option<u8>> {
        self.fill_buf().await.map(|buf| buf.first().cloned())
    }
}

pub struct CopyingBytePeekRead<'t, R, W> {
    reader: &'t mut R,
    writer: &'t mut W,
    read_buf: Vec<u8>,
}

impl<'t, R, W: AsyncWrite + Unpin> CopyingBytePeekRead<'t, BufReader<R>, W> {
    pub fn new(reader: &'t mut BufReader<R>, writer: &'t mut W) -> Self {
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

impl<'t, R: AsyncRead + Unpin, W: AsyncWrite + Unpin> BytePeekRead
    for CopyingBytePeekRead<'t, BufReader<R>, W>
{
    async fn read_byte(&mut self) -> std::io::Result<Option<u8>> {
        let Some(b) = self.peek_byte().await? else {
            return Ok(None);
        };
        self.reader.consume(1);
        self.read_buf.push(b);
        Ok(Some(b))
    }

    async fn peek_byte(&mut self) -> std::io::Result<Option<u8>> {
        let b = {
            let buf = self.reader.buffer();
            if buf.is_empty() {
                self.flush().await?;
                self.reader.fill_buf().await?.first()
            } else {
                buf.first()
            }
        };
        Ok(b.cloned())
    }
}

#[derive(Debug)]
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

trait RangerStorage {
    fn push(&mut self, b: u8);
}

#[derive(Default)]
struct Discard();
impl RangerStorage for Discard {
    fn push(&mut self, _b: u8) {}
}

impl RangerStorage for Vec<u8> {
    fn push(&mut self, b: u8) {
        Vec::push(self, b)
    }
}

struct Ranger<'t, R, S> {
    reader: &'t mut R,
    storage: S,
    limit: usize,
    count: usize,
    start: usize,
}
impl<'t, R: BytePeekRead, S: RangerStorage + Default> Ranger<'t, R, S> {
    fn new(reader: &'t mut R, limit: usize) -> Self {
        Self {
            reader,
            storage: Default::default(),
            limit,
            count: 0,
            start: 0,
        }
    }

    fn done(self) -> (S, usize) {
        (self.storage, self.count)
    }

    async fn read(&mut self) -> Result<u8> {
        if self.count == self.limit {
            return Err(Error::LimitReached);
        }
        let Some(b) = self.reader.read_byte().await? else {
            return Err(Error::InvalidInput);
        };
        self.storage.push(b);
        self.count += 1;
        Ok(b)
    }

    async fn peek(&mut self) -> Result<u8> {
        if self.count == self.limit {
            return Err(Error::LimitReached);
        }
        let Some(b) = self.reader.peek_byte().await? else {
            return Err(Error::InvalidInput);
        };
        Ok(b)
    }

    fn start_range(&mut self) {
        self.start = self.count;
    }
    fn range(&self) -> Range<usize> {
        self.start..self.count
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
        Ok(self.start..self.count - 1) // range without the separator
    }

    async fn expect(&mut self, expected: u8) -> Result<()> {
        if self.read().await? != expected {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }

    async fn next_is_sp(&mut self) -> Result<bool> {
        Ok(b" \t".contains(&self.peek().await?))
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

pub struct StatusLine(LineParts<3>);
impl StatusLine {
    pub fn proto(&self) -> &[u8] {
        self.0.range(0)
    }
    pub fn status_code(&self) -> &[u8] {
        self.0.range(1)
    }
    pub fn status(&self) -> &[u8] {
        self.0.range(2)
    }
    pub fn into_raw(self) -> Vec<u8> {
        self.0.raw
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum HeaderRead {
    Name(Vec<u8>),
    EndOfHeader,
}
