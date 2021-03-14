extern crate structopt;

use std::collections::HashMap;
use std::error::Error;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr};
use std::time::{Duration, Instant};

use mio::{Events, Interest, Poll, Token};
use mio::event::Event;
use mio::net::TcpStream;
use structopt::StructOpt;
use url::{Position, Url};

use crate::ConnectionState::{CONNECTED, CONNECTING, READ, UNCONNECTED};

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

struct Ctx<'a> {
    poll: Poll,
    token: Token,
    sent_requests: usize,
    successful_responses: usize,
    max_requests: usize,
    payload: &'a [u8],
}

impl<'a> Ctx<'a> {
    fn new(payload: &'a [u8], max_requests: usize, poll: Poll) -> Ctx<'a> {
        Ctx {
            poll,
            token: Token(0),
            sent_requests: 0,
            successful_responses: 0,
            max_requests,
            payload,
        }
    }

    fn poll(&mut self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        self.poll.poll(events, timeout)
    }

    fn register(&mut self, conn: &mut Connection) -> io::Result<()> {
        self.poll.registry().register(&mut conn.stream, conn.token, Interest::READABLE | Interest::WRITABLE)
    }

    fn next_token(&mut self) -> Token {
        let next = self.token.0;
        self.token.0 += 1;
        Token(next)
    }

    fn send_more(&self) -> bool {
        self.max_requests > self.sent_requests
    }
}


struct Connection {
    addr: SocketAddr,
    stream: TcpStream,
    state: ConnectionState,
    token: Token,
    bytes_sent: usize,
    bytes_received: usize,
    sent_requests: usize,
    request_started: Option<Instant>,
    times: Vec<Duration>,
}

impl Connection {
    fn new(addr: SocketAddr, ctx: &mut Ctx) -> io::Result<Connection> {
        let client = TcpStream::connect(addr)?;
        let token = ctx.next_token();
        let mut connection = Connection {
            addr,
            stream: client,
            state: UNCONNECTED,
            token,
            bytes_sent: 0,
            bytes_received: 0,
            sent_requests: 0,
            request_started: None,
            times: vec![],
        };
        ctx.register(&mut connection)?;
        connection.set_state(CONNECTING);
        Ok(connection)
    }

    fn reset(&mut self, ctx: &mut Ctx) -> io::Result<()> {
        self.disconnect(ctx)?;
        self.stream = TcpStream::connect(self.addr)?;
        self.set_state(CONNECTING);
        ctx.register(self)
    }

    fn disconnect(&mut self, ctx: &Ctx) -> io::Result<()> {
        ctx.poll.registry().deregister(&mut self.stream)?;
        self.stream.shutdown(Shutdown::Both)?;
        self.set_state(UNCONNECTED);
        Ok(())
    }

    fn set_state(&mut self, new_state: ConnectionState) {
        println!("Connection[{}] {:?} --> {:?}", self.token.0, self.state, new_state);
        self.state = new_state;
        self.measure_request();
    }

    fn measure_request(&mut self) {
        match self.state {
            READ => self.request_started = Some(Instant::now()),
            UNCONNECTED => {
                let start = self.request_started.take().expect("should have request_started field");
                let elapsed = Instant::now() - start;
                self.times.push(elapsed);
            }
            CONNECTING => {}
            CONNECTED => {}
        }
    }
}

#[derive(PartialEq, Debug)]
enum ConnectionState {
    UNCONNECTED,
    CONNECTING,
    CONNECTED,
    READ,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut opt = Opts::from_args();

    if opt.concurrency > opt.requests {
        panic!("Cannot use concurrency level greater than total number of requests");
    }

    if opt.timelimit.is_some() {
        opt.requests = 50000;
    }

    let poll = Poll::new()?;

    let addr: SocketAddr = create_socket_addr(&opt.url);

    let req = create_request(&opt.url);
    let request = req.as_bytes();

    let mut ctx = Ctx::new(request, opt.requests, poll);

    let mut connections = HashMap::new();

    for _ in 0..opt.concurrency {
        let connection = Connection::new(addr, &mut ctx)?;
        connections.insert(connection.token, connection);
    }

    let start = Instant::now();
    let time_limit = Duration::from_secs(opt.timelimit.unwrap_or(u64::max_value()));
    let mut time_left = time_limit;
    let mut events = Events::with_capacity(128);
    while ctx.successful_responses < opt.requests {
        ctx.poll(&mut events, Some(time_left))?;

        for event in events.iter() {
            let token = event.token();
            match connections.get_mut(&token) {
                Some(mut connection) => handle_connection_event(event, &mut ctx, &mut connection)?,
                None => unreachable!(),
            }
        }

        let elapsed = Instant::now() - start;
        if elapsed > time_limit {
            eprintln!("Timelimit exceeded");
            break;
        } else {
            time_left = time_limit - elapsed;
        }
    }

    let time_spent = Instant::now() - start;
    println!("Took {}s {}ms", time_spent.as_secs(), time_spent.as_millis() % 1000);
    println!("Sent {} requests over {} connections", ctx.sent_requests, connections.len());

    let mut all_times: Vec<Duration> = connections.iter().flat_map(|(_, c)| c.times.clone()).collect();
    all_times.sort_unstable();

    println!("Percentage of the requests served within a certain time (ms)");
    for percentage in [50, 66, 75, 80, 90, 95, 98, 99].iter() {
        let idx = all_times.len() / 100 * percentage;
        println!("{}%\t{}", percentage, all_times[idx].as_millis());
    }
    if let Some(longest) = all_times.last() {
        println!("{}%\t{} (longest request)", 100, longest.as_millis());
    }

    Ok(())
}

fn create_socket_addr(url: &Url) -> SocketAddr {
    let port = url.port().unwrap_or_else(|| if url.scheme() == "https" { 443 } else { 80 });
    let host = url.host_str().expect("Missing host");
    let addr = format!("{}:{}", host, port);
    addr.parse().unwrap_or_else(|_| panic!("Failed to parse {} as SocketAddr", addr))
}

fn handle_connection_event(event: &Event, ctx: &mut Ctx, conn: &mut Connection) -> io::Result<()> {
    if event.is_writable() && conn.state == CONNECTING {
        conn.set_state(CONNECTED);
    }

    if event.is_writable() && ctx.send_more() && conn.state == CONNECTED {
        write_request(ctx, conn)?;
    }

    if event.is_readable() {
        let mut bytes_read = 0;
        let mut buf = vec![0; 4096];
        let mut done = false;
        loop {
            match conn.stream.read(&mut buf[bytes_read..]) {
                Ok(0) => {
                    done = true;
                    break;
                }
                Ok(n) => {
                    bytes_read += n;
                    if bytes_read == buf.len() {
                        buf.resize(buf.len() + 1024, 0);
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
        }

        if bytes_read != 0 {
            let received_data = &buf[..bytes_read];
            if let Ok(str) = std::str::from_utf8(received_data) {
                let head = &str[..40];
                let tail = &str[str.len() - 10..];
                println!("Data: {:?}..{:?}", head, tail);
            } else {
                eprintln!("Failed to decode received data");
            }
        }

        if done {
            ctx.successful_responses += 1;
            conn.bytes_received += bytes_read;
            conn.reset(ctx)?;
        }
    }
    Ok(())
}

fn write_request(ctx: &mut Ctx, conn: &mut Connection) -> io::Result<()> {
    conn.stream.write_all(ctx.payload)?;
    ctx.sent_requests += 1;
    conn.sent_requests += 1;
    conn.bytes_sent += ctx.payload.len();
    conn.set_state(READ);
    Ok(())
}

fn create_request(url: &Url) -> String {
    let host = url.host_str().expect("Missing host");
    let path = &url[Position::BeforePath..];
    format!("GET {} HTTP/1.0\r\nHost: {}\r\n{}\r\n\r\n", path, host, "Accept: */*")
}
