extern crate rab;

use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::rc::Rc;
use std::sync::mpsc::channel;
use std::time::Duration;

use hyper::{Body, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use mio::net::TcpStream;
use tokio::task;
use tokio::task::JoinHandle;
use url::Url;

use rab::benchmarking::benchmark;
use rab::connection::Connection;
use rab::ctx::Ctx;
use rab::http::create_request;
use rab::reporting::Reporter;

#[tokio::test(flavor = "multi_thread")]
async fn should_count_body_length() {
    let url = Url::parse("http://localhost:3000").expect("Invalid url");
    let server = create_server(&url, "hello, world");
    let conn = bench_connection(&url);
    assert_eq!(12, conn.bytes_received);
    let _ = server.await;
}

fn bench_connection(url: &Url) -> Connection<TcpStream> {
    let reporter = Rc::new(RefCell::new(Reporter::new(None)));
    let request = create_request(&url, false);
    let mut ctx = Ctx::new(request.as_bytes(), 1, 1).unwrap();
    let conn = Connection::new(
        &mut ctx,
        url.socket_addrs(|| None).unwrap()[0],
        Box::new(TcpStream::connect),
        reporter.clone(),
    ).expect("Failed to create connection");
    let token = conn.token;
    let mut connections = HashMap::new();
    connections.insert(conn.token, conn);

    benchmark(Duration::from_secs(5), &mut ctx, &mut connections, reporter).expect("Failed benchmark");

    connections.remove(&token).unwrap()
}

fn create_server(url: &Url, response: &'static str) -> JoinHandle<()> {
    let (tx, rx) = channel();
    let addr = url.socket_addrs(|| None).unwrap()[0];
    let handle = task::spawn(async move {
        let make_svc = make_service_fn(|_conn| async move {
            Ok::<_, Infallible>(service_fn(move |_| async move {
                Ok::<_, Infallible>(Response::new(Body::from(response)))
            }))
        });

        let s = Server::bind(&addr).serve(make_svc);
        if let Err(e) = tx.send(()) {
            eprintln!("Failed to signal server start: {}", e);
        }
        if let Err(e) = s.await {
            eprintln!("Failed to start server: {}", e);
        }
    });

    rx.recv_timeout(Duration::from_secs(2)).expect("Failed to start server fast enough");
    handle
}

