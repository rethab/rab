extern crate structopt;

use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
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

    url: Url,
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

    let addr: SocketAddr = create_socket_addr(&opt.url);

    let req = create_request(&opt.url);
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

fn create_socket_addr(url: &Url) -> SocketAddr {
    let port = url
        .port()
        .unwrap_or_else(|| if url.scheme() == "https" { 443 } else { 80 });
    let host = url.host_str().expect("Missing host");
    let addr = format!("{}:{}", host, port);
    addr.parse()
        .unwrap_or_else(|_| panic!("Failed to parse {} as SocketAddr", addr))
}

fn create_request(url: &Url) -> String {
    let host = url.host_str().expect("Missing host");
    let path = &url[Position::BeforePath..];
    format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\n{}\r\n\r\n",
        path, host, "Accept: */*"
    )
}
