use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

use mio::event::{Event, Source};
use mio::{Events, Token};

use super::connection::Connection;
use super::connection::ConnectionState::{Connected, Connecting};
use super::ctx::Ctx;
use super::http::Response;
use super::reporting::Reporter;
use std::io::{Read, Write};

pub fn benchmark<S: Write + Read + Source>(
    timelimit: Duration,
    ctx: &mut Ctx,
    connections: &mut HashMap<Token, Connection<S>>,
    reporter: Rc<RefCell<Reporter>>,
) -> io::Result<()> {
    let start = Instant::now();
    let mut time_left = timelimit;
    let mut events = Events::with_capacity(128);

    reporter.borrow_mut().start();

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

    reporter.borrow_mut().end();

    Ok(())
}

pub fn handle_connection_event<S: Write + Read + Source>(
    event: &Event,
    ctx: &mut Ctx,
    conn: &mut Connection<S>,
) -> io::Result<()> {
    if event.is_writable() && conn.state == Connecting {
        conn.set_state(Connected);
    }

    if event.is_writable() && ctx.send_more() && conn.state == Connected {
        conn.send_request(ctx)?;
    }

    if event.is_readable() {
        let mut buf = vec![0; 4096];
        let (done, bytes_read) = conn.read_all(&mut buf);

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

fn record_response<S>(received_data: &[u8], conn: &Connection<S>, ctx: &mut Ctx) {
    if !conn.is_reading_response() {
        // first bytes, check http response code

        // first response from this server, store some things
        let first_response = ctx.server_name.is_none();

        if let Ok(resp) = Response::parse(received_data, !first_response) {
            if first_response {
                ctx.server_name = Some(resp.server.unwrap_or_default());
                ctx.doclen = resp.body_length;
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
