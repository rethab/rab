use url::{Position, Url};

pub fn parse_response(header: &[u8]) -> Result<u16, String> {
    let ascii_num = |c: u8| (c - 48) as u16;

    if let [a, b, c] = header[9..12] {
        Ok(ascii_num(a) * 100 + ascii_num(b) * 10 + ascii_num(c))
    } else {
        Err(format!(
            "Cannot parse as HTTP header: {}",
            String::from_utf8_lossy(header)
        ))
    }
}

pub fn create_request(url: &Url) -> String {
    let host = url.host_str().expect("Missing host");
    let path = &url[Position::BeforePath..];
    format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\n{}\r\n\r\n",
        path, host, "Accept: */*"
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_response() {
        assert_eq!(200, parse_response("HTTP/1.1 200 OK".as_bytes()).unwrap());
    }
}
