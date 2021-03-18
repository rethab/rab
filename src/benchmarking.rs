use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Write};
use std::time::{Duration, Instant};

use mio::event::Event;
use mio::{Events, Token};

use crate::connection::ConnectionState::{CONNECTED, CONNECTING, READ};
use crate::connection::{Connection, Ctx};
use crate::http::Response;
use mio::net::TcpStream;

pub fn benchmark(
    timelimit: Duration,
    ctx: &mut Ctx,
    connections: &mut HashMap<Token, Connection>,
) -> io::Result<()> {
    let start = Instant::now();
    let mut time_left = timelimit;
    let mut events = Events::with_capacity(128);
    while ctx.expect_more_responses() {
        ctx.poll(&mut events, Some(time_left))?;

        for event in events.iter() {
            let token = event.token();
            match connections.get_mut(&token) {
                Some(mut connection) => handle_connection_event(event, ctx, &mut connection)?,
                None => unreachable!(),
            }
        }

        let elapsed = Instant::now() - start;
        if elapsed > timelimit {
            eprintln!("Timelimit exceeded");
            break;
        } else {
            time_left = timelimit - elapsed;
        }
    }
    Ok(())
}

fn handle_connection_event(event: &Event, ctx: &mut Ctx, conn: &mut Connection) -> io::Result<()> {
    if event.is_writable() && conn.state == CONNECTING {
        conn.set_state(CONNECTED);
    }

    if event.is_writable() && ctx.send_more() && conn.state == CONNECTED {
        write_request(ctx, conn)?;
    }

    if event.is_readable() {
        let mut buf = vec![0; 4096];
        let (done, bytes_read) = read_from_stream(&mut conn.stream, &mut buf);

        if bytes_read != 0 {
            record_response(&buf[..bytes_read], conn, ctx);
            conn.bytes_read(bytes_read);
        }

        if done {
            conn.finish_request();
            conn.reset(ctx)?;
        }
    }
    Ok(())
}

fn read_from_stream(stream: &mut TcpStream, buf: &mut Vec<u8>) -> (bool, usize) {
    let mut bytes_read = 0;
    loop {
        match stream.read(&mut buf[bytes_read..]) {
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

fn record_response(received_data: &[u8], conn: &Connection, ctx: &mut Ctx) {
    if !conn.is_reading_response() {
        // first bytes, check http response code

        // first response from this server, store server name
        let parse_server_name = ctx.server_name.is_none();
        if let Ok(resp) = Response::parse(received_data, !parse_server_name) {
            if parse_server_name {
                ctx.server_name = Some(resp.server.unwrap_or_default());
            }
            if (200..300).contains(&resp.status) {
                ctx.successful_response();
            } else {
                eprintln!("HTTP Response Code {}", resp.status);
                ctx.unsuccessful_response();
            }
        } else {
            eprintln!("Failed to parse HTTP Header");
            ctx.failed_response();
        }
    }
}

fn write_request(ctx: &mut Ctx, conn: &mut Connection) -> io::Result<()> {
    conn.stream.write_all(ctx.payload)?;
    ctx.sent_requests += 1;
    conn.sent_requests += 1;
    conn.bytes_sent += ctx.payload.len();
    conn.set_state(READ);
    Ok(())
}
