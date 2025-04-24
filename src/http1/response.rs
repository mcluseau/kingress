use super::{status, Writer};

pub fn status(status: &[u8]) -> Vec<u8> {
    plain(status, status)
}

pub fn plain(status: &[u8], message: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();

    w.status(status);

    w.header("Content-Type", "text/plain");

    let mut w = w.content_length(message.len() + 1);

    w.extend_from_slice(message);
    w.push(b'\n');
    w
}

pub fn redirect(target_url: &str) -> Vec<u8> {
    let mut w = Writer::new();

    w.status(status::MOVED_PERMANENTLY);
    w.append_str("HTTP/1.1 301 Moved Permanently\r\n");
    w.header("Location", target_url);
    w.header("Content-Type", "text/html");

    w.content(format!("<a href=\"{target_url}\">Moved Permanently</a>.\n"))
}
