use core::ops::Range;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

pub struct Reader<R> {
    reader: R,
    limit: usize,
    count: usize,
}

impl<R: AsyncBufRead + Unpin> Reader<R> {
    pub fn new(reader: R, limit: usize) -> Self {
        Self {
            reader,
            limit,
            count: 0,
        }
    }

    fn ranger(&mut self, limit: usize) -> Ranger<R> {
        let avail_imit = self.limit.checked_sub(self.count).unwrap_or(0);
        Ranger::new(self, limit.min(avail_imit))
    }

    fn done(&mut self, raw: Vec<u8>) -> Vec<u8> {
        self.count += raw.len();
        raw
    }

    pub async fn request_line(&mut self, limit: usize) -> Result<RequestLine> {
        let mut ranger = self.ranger(limit);

        let parts: [Range<usize>; 3] = [
            ranger.range_to_and_skip_sp(b' ').await?,
            ranger.range_to_and_skip_sp(b' ').await?,
            ranger.range_to(b'\r').await?,
        ];
        ranger.expect(b'\n').await?;

        Ok(RequestLine(LineParts {
            raw: ranger.done(),
            parts,
        }))
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

        Ok(HeaderRead::Header(Header(LineParts {
            raw: ranger.done(),
            parts: [name, value],
        })))
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

    pub fn append(&mut self, mut v: Vec<u8>) {
        self.0.append(&mut v);
    }
    pub fn append_string(&mut self, v: String) {
        self.0.append(&mut v.into_bytes());
    }
    pub fn append_slice(&mut self, v: &[u8]) {
        self.0.extend_from_slice(v);
    }

    pub fn header(&mut self, name: &str, value: &str) {
        self.append_slice(name.as_bytes());
        self.append_slice(b": ");
        self.append_slice(value.as_bytes());
        self.append_slice(b"\r\n");
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
    reader: &'t mut Reader<R>,
    limit: usize,
    start: usize,
}
impl<'t, R: AsyncBufRead + Unpin> Ranger<'t, R> {
    fn new(reader: &'t mut Reader<R>, limit: usize) -> Self {
        Self {
            raw: Vec::with_capacity(limit.min(4096 /* page size */)),
            reader,
            limit,
            start: 0,
        }
    }

    fn done(self) -> Vec<u8> {
        self.reader.count += self.raw.len();
        self.raw
    }

    async fn read(&mut self) -> Result<u8> {
        if self.raw.len() == self.limit {
            return Err(Error::LimitReached);
        }
        let b = self.reader.reader.read_u8().await?;
        self.raw.push(b);
        Ok(b)
    }

    async fn peek(&mut self) -> Result<u8> {
        if self.raw.len() == self.limit {
            return Err(Error::LimitReached);
        }
        Ok(self.reader.reader.fill_buf().await.map(|buf| buf[0])?)
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
