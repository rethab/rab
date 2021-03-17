use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Write};
use std::time::{Duration, Instant};

use mio::{Events, Token};
use mio::event::Event;

use crate::connection::{Connection, Ctx};
use crate::connection::ConnectionState::{CONNECTED, CONNECTING, READ};

pub fn benchmark(
    timelimit: Duration,
    ctx: &mut Ctx,
    connections: &mut HashMap<Token, Connection>,
) -> io::Result<()> {
    let start = Instant::now();
    let mut time_left = timelimit;
    let mut events = Events::with_capacity(128);
    while ctx.successful_responses < ctx.max_requests {
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
