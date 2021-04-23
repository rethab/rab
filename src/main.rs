extern crate rab;
extern crate structopt;

use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::net::SocketAddr;
use std::rc::Rc;
use std::str::FromStr;
use std::time::Duration;

use structopt::StructOpt;
use url::Url;

use mio::net::TcpStream;
use rab::benchmarking::benchmark;
use rab::connection::Connection;
use rab::ctx::Ctx;
use rab::http;
use rab::http::HttpVersion;
use rab::reporting::Reporter;

#[derive(StructOpt, Debug)]
#[structopt(name = "rab", about = "A drop-in replacement ApacheBench")]
struct Opts {
    #[structopt(
        short,
        long,
        default_value = "1",
        help = "Number of multiple requests to make at a time"
    )]
    concurrency: usize,

    #[structopt(
        short = "n",
        long,
        default_value = "1",
        help = "Number of requests to perform"
    )]
    requests: usize,

    #[structopt(short = "i", help = "Use HEAD instead of GET")]
    use_head: bool,

    #[structopt(
        short,
        long,
        help = "Seconds to max. to spend on benchmarking\nThis implies -n 50000"
    )]
    timelimit: Option<u64>,

    #[structopt(help = "[http[s]://]hostname[:port]/path")]
    url: LenientUrl,

    #[structopt(long = "http1.0", help = "Use HTTP 1.0 instead of 1.1")]
    http1_0: bool,

    #[structopt(
        short = "q",
        help = "Do not show progress when doing more than 150 requests"
    )]
    quiet: bool,
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
        Url::parse(&url)
            .map_err(|_| "invalid URL".to_string())
            .map(LenientUrl)
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

    let http_version = decide_version(&opt);
    let req = http::create_request(&opt.url.0, opt.use_head, http_version);

    let heartbeatres = if opt.quiet || opt.requests <= 150 {
        None
    } else {
        Some(100.max(opt.requests / 10))
    };
    let reporter = Rc::new(RefCell::new(Reporter::new(heartbeatres)));
    let mut ctx = Ctx::new(req.into_bytes(), opt.requests, opt.concurrency)?;

    let mut connections = HashMap::new();

    for _ in 0..opt.concurrency {
        let factory = Box::new(TcpStream::connect);
        let connection = Connection::<TcpStream>::new(&mut ctx, addr, factory, reporter.clone())?;
        connections.insert(connection.token, connection);
    }

    println!(
        "Benchmarking {} (be patient)",
        opt.url.0.host_str().unwrap()
    );
    println!();

    benchmark(timelimit, &mut ctx, &mut connections, reporter.clone())?;

    if heartbeatres.is_some() {
        println!("Finished {} requests", ctx.total_responses());
        println!();
    }

    reporter.borrow().print(&opt.url.0, &ctx);

    Ok(())
}

fn decide_version(opts: &Opts) -> HttpVersion {
    if opts.http1_0 {
        HttpVersion::V1_0
    } else {
        HttpVersion::V1_1
    }
}

fn create_socket_addr(url: &Url) -> io::Result<SocketAddr> {
    url.socket_addrs(|| url.port_or_known_default())
        .map(|ss| ss[0])
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_create_socket_addr() {
        let localhost_port: SocketAddr = "[::1]:8080".parse().unwrap();
        let localhost_http: SocketAddr = "[::1]:80".parse().unwrap();
        let localhost_https: SocketAddr = "[::1]:443".parse().unwrap();
        assert_eq!(
            localhost_port,
            create_socket_addr(&parse_url("localhost:8080").0).unwrap()
        );
        assert_eq!(
            localhost_port,
            create_socket_addr(&parse_url("http://localhost:8080").0).unwrap()
        );
        assert_eq!(
            localhost_http,
            create_socket_addr(&parse_url("http://localhost").0).unwrap()
        );
        assert_eq!(
            localhost_http,
            create_socket_addr(&parse_url("http://localhost:80").0).unwrap()
        );
        assert_eq!(
            localhost_https,
            create_socket_addr(&parse_url("https://localhost").0).unwrap()
        );
        assert_eq!(
            localhost_https,
            create_socket_addr(&parse_url("https://localhost:443").0).unwrap()
        );
    }

    #[test]
    fn test_lenient_url_from_str() {
        assert_eq!(
            "http",
            LenientUrl::from_str("localhost").unwrap().0.scheme()
        );
        assert_eq!(
            "localhost",
            LenientUrl::from_str("localhost")
                .unwrap()
                .0
                .host_str()
                .unwrap()
        );
    }

    fn parse_url(url: &str) -> LenientUrl {
        LenientUrl::from_str(url).unwrap()
    }
}
