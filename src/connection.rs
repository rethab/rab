use std::cell::RefCell;
use std::io;
use std::net::{Shutdown, SocketAddr};
use std::rc::Rc;
use std::time::Duration;

use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token};

use ConnectionState::{CONNECTING, UNCONNECTED};

use crate::reporting::Reporter;

pub struct Ctx<'a> {
    pub successful_responses: usize,
    pub unsuccessful_responses: usize,
    pub failed_responses: usize,
    pub sent_requests: usize,
    pub payload: &'a [u8],
    pub concurrency: usize,
    pub server_name: Option<String>,
    max_requests: usize,
    poll: Poll,
    token: Token,
}

impl<'a> Ctx<'a> {
    pub fn new(payload: &'a [u8], max_requests: usize, concurrency: usize) -> io::Result<Ctx<'a>> {
        Ok(Ctx {
            poll: Poll::new()?,
            token: Token(0),
            sent_requests: 0,
            successful_responses: 0,
            unsuccessful_responses: 0,
            failed_responses: 0,
            server_name: None,
            max_requests,
            concurrency,
            payload,
        })
    }

    pub fn expect_more_responses(&self) -> bool {
        let total_responses =
            self.failed_responses + self.successful_responses + self.unsuccessful_responses;
        total_responses < self.max_requests
    }

    pub fn successful_response(&mut self) {
        self.successful_responses += 1;
    }

    pub fn unsuccessful_response(&mut self) {
        self.unsuccessful_responses += 1;
    }

    pub fn failed_response(&mut self) {
        self.failed_responses += 1;
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
    reading_response: bool,
    reporter: Rc<RefCell<Reporter>>,
}

impl Connection {
    pub fn new(
        addr: SocketAddr,
        ctx: &mut Ctx,
        reporter: Rc<RefCell<Reporter>>,
    ) -> io::Result<Connection> {
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
            reading_response: false,
            reporter,
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

    pub fn finish_request(&mut self) {
        self.reading_response = false;
    }

    pub fn bytes_read(&mut self, nbytes: usize) {
        self.reading_response = true;
        self.bytes_received += nbytes;
    }

    pub fn is_reading_response(&self) -> bool {
        self.reading_response
    }

    pub fn set_state(&mut self, new_state: ConnectionState) {
        self.state = new_state;
        self.reporter
            .borrow_mut()
            .connection_state_changed(&self.token, &self.state);
    }
}

#[derive(PartialEq, Debug)]
pub enum ConnectionState {
    UNCONNECTED,
    CONNECTING,
    CONNECTED,
    READ,
}
