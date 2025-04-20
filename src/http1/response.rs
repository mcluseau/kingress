use super::Writer;

pub fn status(status: &str) -> Vec<u8> {
    plain(status, status)
}

pub fn plain(status: &str, message: &str) -> Vec<u8> {
    let mut w = Writer::new();

    w.append_str("HTTP/1.1 ");
    w.append_str(status);
    w.crlf();

    w.header("Content-Type", "text/plain");

    let message = message.as_bytes();
    let mut w = w.content_length(message.len() + 1);

    w.extend_from_slice(message);
    w.push(b'\n');
    w
}

pub fn redirect(target_url: &str) -> Vec<u8> {
    let mut w = Writer::new();

    w.append_str("HTTP/1.1 301 Moved Permanently\r\n");
    w.header("Location", target_url);
    w.header("Content-Type", "text/html");

    let mut content = format!("<a href=\"{target_url}\">Moved Permanently</a>.\n").into_bytes();

    let mut w = w.content_length(content.len());
    w.append(&mut content);
    w
}
