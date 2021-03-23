use std::cell::RefCell;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::rc::Rc;

use mio::Token;

use ConnectionState::{CONNECTING, UNCONNECTED};

use super::connection::ConnectionState::READ;
use super::ctx::Ctx;
use super::reporting::Reporter;
use mio::event::Source;
use std::mem;
use std::net::SocketAddr;

pub struct Connection<S> {
    pub token: Token,
    addr: SocketAddr,
    stream: S,
    factory: Box<dyn Fn(SocketAddr) -> io::Result<S>>,
    pub state: ConnectionState,
    bytes_sent: usize,
    pub bytes_received: usize,
    sent_requests: usize,
    reading_response: bool,
    reporter: Rc<RefCell<Reporter>>,
}

impl<S> Connection<S>
where
    S: Read + Write + Source,
{
    pub fn new(
        ctx: &mut Ctx,
        addr: SocketAddr,
        factory: Box<dyn Fn(SocketAddr) -> io::Result<S>>,
        reporter: Rc<RefCell<Reporter>>,
    ) -> io::Result<Connection<S>> {
        let token = ctx.next_token();
        let mut connection = Connection {
            addr,
            stream: factory(addr)?,
            factory,
            state: UNCONNECTED,
            token,
            bytes_sent: 0,
            bytes_received: 0,
            sent_requests: 0,
            reading_response: false,
            reporter,
        };
        ctx.register(token, &mut connection.stream)?;
        connection.set_state(CONNECTING);
        Ok(connection)
    }

    pub fn reset(&mut self, ctx: &mut Ctx) -> io::Result<()> {
        ctx.deregister(&mut self.stream)?;
        let _ = mem::replace(&mut self.stream, (self.factory)(self.addr)?);
        // prev stream should be dropped here
        self.set_state(UNCONNECTED);
        self.set_state(CONNECTING);
        ctx.register(self.token, &mut self.stream)
    }
}

impl<S: Read> Connection<S> {
    pub fn read_all(&mut self, buf: &mut Vec<u8>) -> (bool, usize) {
        let mut bytes_read = 0;
        loop {
            match self.stream.read(&mut buf[bytes_read..]) {
                Ok(0) => {
                    return (true, bytes_read);
                }
                Ok(n) => {
                    bytes_read += n;
                    if bytes_read == buf.len() {
                        buf.resize(buf.len() + 1024, 0);
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return (false, bytes_read),
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return (false, bytes_read),
            };
        }
    }
}

impl<S> Connection<S> {
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

impl<S> Connection<S>
where
    S: Write,
{
    pub fn send_request(&mut self, ctx: &mut Ctx) -> io::Result<()> {
        self.stream.write_all(&ctx.payload)?;
        ctx.sent_requests += 1;
        self.sent_requests += 1;
        self.bytes_sent += ctx.payload.len();
        self.set_state(READ);
        Ok(())
    }
}

#[derive(PartialEq, Debug)]
pub enum ConnectionState {
    UNCONNECTED,
    CONNECTING,
    CONNECTED,
    READ,
}
