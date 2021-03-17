extern crate structopt;

use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::{Duration, Instant};

use structopt::StructOpt;
use url::{Position, Url};

use connection::{Connection, Ctx};

use crate::benchmarking::benchmark;
use crate::reporting::report;

mod benchmarking;
mod connection;
mod reporting;

#[derive(StructOpt, Debug)]
struct Opts {
    #[structopt(short, long, default_value = "1")]
    concurrency: usize,

    #[structopt(short = "n", long, default_value = "1")]
    requests: usize,

    #[structopt(short, long)]
    timelimit: Option<u64>,

    url: LenientUrl,
}

#[derive(Debug)]
struct LenientUrl(Url);

impl FromStr for LenientUrl {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url: String = if !s.starts_with("http://") && !s.starts_with("https://") {
            format!("http://{}", s)
        } else {
            s.to_owned()
        };
        Url::parse(&url).map_err(|_| format!("invalid URL")).map(LenientUrl)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut opt = Opts::from_args();

    if opt.concurrency > opt.requests {
        panic!("Cannot use concurrency level greater than total number of requests");
    }

    if opt.timelimit.is_some() {
        opt.requests = 50000;
    }

    let timelimit = Duration::from_secs(opt.timelimit.unwrap_or(u64::max_value()));

    let addr: SocketAddr = create_socket_addr(&opt.url.0)?;

    let req = create_request(&opt.url.0);
    let request = req.as_bytes();

    let mut ctx = Ctx::new(request, opt.requests)?;

    let mut connections = HashMap::new();

    for _ in 0..opt.concurrency {
        let connection = Connection::new(addr, &mut ctx)?;
        connections.insert(connection.token, connection);
    }

    let start = Instant::now();
    benchmark(timelimit, &mut ctx, &mut connections)?;
    let time_spent = Instant::now() - start;

    report(time_spent, &ctx, connections);

    Ok(())
}

fn create_socket_addr(url: &Url) -> io::Result<SocketAddr> {
    url.socket_addrs(|| url.port_or_known_default()).map(|ss| ss[0])
}

fn create_request(url: &Url) -> String {
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
    fn test_create_socket_addr() {
        let localhost_port: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let localhost_http: SocketAddr = "127.0.0.1:80".parse().unwrap();
        let localhost_https: SocketAddr = "127.0.0.1:443".parse().unwrap();
        assert_eq!(localhost_port, create_socket_addr(&parse_url("localhost:8080").0).unwrap());
        assert_eq!(localhost_port, create_socket_addr(&parse_url("http://localhost:8080").0).unwrap());
        assert_eq!(localhost_http, create_socket_addr(&parse_url("http://localhost").0).unwrap());
        assert_eq!(localhost_http, create_socket_addr(&parse_url("http://localhost:80").0).unwrap());
        assert_eq!(localhost_https, create_socket_addr(&parse_url("https://localhost").0).unwrap());
        assert_eq!(localhost_https, create_socket_addr(&parse_url("https://localhost:443").0).unwrap());
    }

    #[test]
    fn test_lenient_url_from_str() {
        assert_eq!("http", LenientUrl::from_str("localhost").unwrap().0.scheme());
        assert_eq!("localhost", LenientUrl::from_str("localhost").unwrap().0.host_str().unwrap());
    }

    fn parse_url(url: &str) -> LenientUrl {
        LenientUrl::from_str(url).unwrap()
    }
}
