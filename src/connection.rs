use std::io;
use std::net::{Shutdown, SocketAddr};
use std::time::{Duration, Instant};

use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token};

use ConnectionState::{CONNECTED, CONNECTING, READ, UNCONNECTED};

pub struct Ctx<'a> {
    pub successful_responses: usize,
    poll: Poll,
    token: Token,
    pub sent_requests: usize,
    pub(crate) max_requests: usize,
    pub payload: &'a [u8],
}

impl<'a> Ctx<'a> {
    pub fn new(payload: &'a [u8], max_requests: usize) -> io::Result<Ctx<'a>> {
        Ok(Ctx {
            poll: Poll::new()?,
            token: Token(0),
            sent_requests: 0,
            successful_responses: 0,
            max_requests,
            payload,
        })
    }

    pub fn poll(&mut self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        self.poll.poll(events, timeout)
    }

    fn register(&mut self, conn: &mut Connection) -> io::Result<()> {
        self.poll.registry().register(
            &mut conn.stream,
            conn.token,
            Interest::READABLE | Interest::WRITABLE,
        )
    }

    fn next_token(&mut self) -> Token {
        let next = self.token.0;
        self.token.0 += 1;
        Token(next)
    }

    pub fn send_more(&self) -> bool {
        self.max_requests > self.sent_requests
    }
}

pub struct Connection {
    pub token: Token,
    addr: SocketAddr,
    pub stream: TcpStream,
    pub state: ConnectionState,
    pub bytes_sent: usize,
    pub bytes_received: usize,
    pub sent_requests: usize,
    request_started: Option<Instant>,
    pub times: Vec<Duration>,
}

impl Connection {
    pub fn new(addr: SocketAddr, ctx: &mut Ctx) -> io::Result<Connection> {
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

    pub fn reset(&mut self, ctx: &mut Ctx) -> io::Result<()> {
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

    pub fn set_state(&mut self, new_state: ConnectionState) {
        println!(
            "Connection[{}] {:?} --> {:?}",
            self.token.0, self.state, new_state
        );
        self.state = new_state;
        self.measure_request();
    }

    fn measure_request(&mut self) {
        match self.state {
            READ => self.request_started = Some(Instant::now()),
            UNCONNECTED => {
                let start = self
                    .request_started
                    .take()
                    .expect("should have request_started field");
                let elapsed = Instant::now() - start;
                self.times.push(elapsed);
            }
            CONNECTING => {}
            CONNECTED => {}
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum ConnectionState {
    UNCONNECTED,
    CONNECTING,
    CONNECTED,
    READ,
}