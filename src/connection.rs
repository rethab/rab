use std::cell::RefCell;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr};
use std::rc::Rc;

use mio::net::TcpStream;
use mio::Token;

use ConnectionState::{CONNECTING, UNCONNECTED};

use crate::connection::ConnectionState::READ;
use crate::ctx::Ctx;
use crate::reporting::Reporter;

pub struct Connection {
    pub token: Token,
    addr: SocketAddr,
    stream: TcpStream,
    pub state: ConnectionState,
    bytes_sent: usize,
    bytes_received: usize,
    sent_requests: usize,
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
        ctx.register(token, &mut connection.stream)?;
        connection.set_state(CONNECTING);
        Ok(connection)
    }

    pub fn reset(&mut self, ctx: &mut Ctx) -> io::Result<()> {
        self.disconnect(ctx)?;
        self.stream = TcpStream::connect(self.addr)?;
        self.set_state(CONNECTING);
        ctx.register(self.token, &mut self.stream)
    }

    fn disconnect(&mut self, ctx: &Ctx) -> io::Result<()> {
        ctx.deregister(&mut self.stream)?;
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

    pub fn send_request(&mut self, ctx: &mut Ctx) -> io::Result<()> {
        self.stream.write_all(ctx.payload)?;
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
