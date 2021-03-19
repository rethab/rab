use url::{Position, Url};

pub struct Response {
    pub status: u16,
    pub server: Option<String>, // Server header
}

impl Response {
    pub fn parse(resp: &[u8], status_only: bool) -> Result<Self, String> {
        let ascii_num = |c: u8| (c - 48) as u16;

        if let [a, b, c] = resp[9..12] {
            let status = ascii_num(a) * 100 + ascii_num(b) * 10 + ascii_num(c);
            let server = if status_only {
                None
            } else {
                parse_server(resp)
            };
            Ok(Response { status, server })
        } else {
            Err(format!(
                "Cannot parse as HTTP header: {}",
                String::from_utf8_lossy(resp)
            ))
        }
    }
}

fn parse_server(resp: &[u8]) -> Option<String> {
    String::from_utf8_lossy(resp)
        .split("\r\n")
        .find_map(|line| {
            if line.starts_with("Server:") {
                Some(line[8..].to_owned())
            } else {
                None
            }
        })
}

pub fn create_request(url: &Url, use_head: bool) -> String {
    let host = url.host_str().expect("Missing host");
    let path = &url[Position::BeforePath..];
    let method = if use_head { "HEAD" } else { "GET" };
    format!(
        "{} {} HTTP/1.0\r\nHost: {}\r\n{}\r\n\r\n",
        method, path, host, "Accept: */*"
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_status_code() {
        assert_eq!(
            200,
            Response::parse("HTTP/1.1 200 OK".as_bytes(), true)
                .unwrap()
                .status
        );
    }

    #[test]
    fn test_parse_server() {
        let google_response = "HTTP/1.1 200 OK\r\n\
          Date: Thu, 18 Mar 2021 19:24:37 GMT\r\n\
          P3P: CP=\"This is not a P3P policy! See g.co/p3phelp for more info.\"\r\n\
          Server: gws\r\n\
        ";
        assert_eq!(
            Some("gws".to_owned()),
            Response::parse(google_response.as_bytes(), false)
                .unwrap()
                .server
        );

        let google_response_simple = "HTTP/1.1 200 OK\r\nServer: gws\r\n";
        assert_eq!(
            Some("gws".to_owned()),
            Response::parse(google_response_simple.as_bytes(), false)
                .unwrap()
                .server
        );

        let no_server_response =
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json;charset=UTF-8\r\n";
        assert_eq!(
            None,
            Response::parse(no_server_response.as_bytes(), false)
                .unwrap()
                .server
        );

        assert_eq!(
            None,
            Response::parse(google_response.as_bytes(), true)
                .unwrap()
                .server
        );
    }
}
