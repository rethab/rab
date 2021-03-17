use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Write};
use std::time::{Duration, Instant};

use mio::event::Event;
use mio::{Events, Token};

use crate::connection::ConnectionState::{CONNECTED, CONNECTING, READ};
use crate::connection::{Connection, Ctx};
use crate::http;

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
            if !conn.is_reading_response() {
                // first bytes, check http response code
                if let Ok(resp_code) = http::parse_response(received_data) {
                    if (200..300).contains(&resp_code) {
                        ctx.successful_response();
                    } else {
                        eprintln!("HTTP Response Code {}", resp_code);
                        ctx.unsuccessful_response();
                    }
                } else {
                    eprintln!("Failed to parse HTTP Header");
                    ctx.failed_response();
                }
            }
            conn.bytes_read(bytes_read);
        }

        if done {
            conn.finish_request();
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
