macro_rules! status {
    ($text:literal, $name:ident) => {
        pub const $name: &[u8] = $text;
    };
}

status!(b"200 OK", OK);
status!(b"204 No Content", NO_CONTENT);
status!(b"301 Moved Permanently", MOVED_PERMANENTLY);
status!(b"403 Forbidden", FORBIDDEN);
status!(b"400 Bad Request", BAD_REQUEST);
status!(b"404 Not Found", NOT_FOUND);
status!(b"413 Content Too Large", CONTENT_TOO_LARGE);
status!(b"414 URI Too Long", URI_TOO_LONG);
status!(b"502 Bad Gateway", BAD_GATEWAY);
status!(b"503 Service Unavailable", SERVICE_UNAVAILABLE);
