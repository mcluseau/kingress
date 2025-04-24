use super::*;

use std::io::Cursor;

#[tokio::test]
async fn test_req() -> Result<()> {
    let req = b"\
        GET /req-path?n1=v1&v2=v2 HTTP/1.1\r\n\
        Host: my-host.local\r\n\
        User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:136.0) Gecko/20100101 Firefox/172.0\r\n\
        Accept: */*\r\n\
        Accept-Language: en-US,en;q=0.5\r\n\
        Accept-Encoding: gzip, deflate\r\n\
        Referer: http://novit.io\r\n\
        Connection: keep-alive\r\n\
        Cookie: name=dmFsdWUK\r\n\
        Priority: u=4\r\n\
        \r\n";

    let mut input = BufReader::new(Cursor::new(req));
    let mut r = Reader::new(&mut input, None);

    r.request_line(1024).await?;
    loop {
        match r.header_name().await? {
            HeaderRead::EndOfHeader => break,
            _ => {}
        }
        r.skip_header_value(1024).await?;
    }

    assert_eq!(Some(0), r.content_length());

    Ok(())
}

#[tokio::test]
async fn test_resp() -> Result<()> {
    let req = b"\
        HTTP/1.1 200 OK\r\n\
        Content-Type: application/json; charset=utf-8\r\n\
        X-Custom: CustomValue\r\n\
        Date: Tue, 22 Apr 2025 20:53:54 GMT\r\n\
        Transfer-Encoding: chunked\r\n\
        \r\n\
        response\n\
        content";

    let mut input = BufReader::new(Cursor::new(req));
    let mut r = Reader::new(&mut input, None);

    r.status_line(1024).await?;

    use std::str::from_utf8;

    for (n, v) in &[
        ("Content-Type", "application/json; charset=utf-8"),
        ("X-Custom", "CustomValue"),
        ("Date", "Tue, 22 Apr 2025 20:53:54 GMT"),
        ("Transfer-Encoding", "chunked"),
    ] {
        let name = match r.header_name().await? {
            HeaderRead::EndOfHeader => panic!("unexpected end of header"),
            HeaderRead::Name(name) => name,
        };

        assert_eq!(n, &from_utf8(&name).unwrap());

        let value = r.header_value(&name, 1024).await?;
        assert_eq!(v, &from_utf8(&value).unwrap());
    }

    assert_eq!(HeaderRead::EndOfHeader, r.header_name().await?);

    assert_eq!(None, r.content_length());

    Ok(())
}
