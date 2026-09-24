pub fn append_path_segment(out: &mut String, value: &str) {
    append_percent_encoded(out, value.as_bytes());
}

pub fn append_query_pair(out: &mut String, first: &mut bool, key: &str, value: &str) {
    if *first {
        out.push('?');
        *first = false;
    } else {
        out.push('&');
    }
    append_percent_encoded(out, key.as_bytes());
    out.push('=');
    append_percent_encoded(out, value.as_bytes());
}

#[must_use]
pub fn format_bool(value: &bool) -> &'static str {
    if *value { "1" } else { "0" }
}
fn append_percent_encoded(out: &mut String, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for &byte in bytes {
        if is_unreserved(byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}
const fn is_unreserved(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
    )
}
