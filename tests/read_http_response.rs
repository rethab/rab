extern crate rab;
extern crate serial_test;

use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::rc::Rc;
use std::sync::mpsc::channel;
use std::time::Duration;

use hyper::body::Bytes;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response, Server};
use mio::net::TcpStream;
use serial_test::serial;
use tokio::sync::oneshot;
use tokio::task;
use tokio::task::JoinHandle;
use url::Url;

use rab::benchmarking::benchmark;
use rab::connection::Connection;
use rab::ctx::Ctx;
use rab::http::create_request;
use rab::reporting::Reporter;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn should_count_body_length() {
    let url = Url::parse("http://localhost:3000").expect("Invalid url");
    let (server, tx_done) = create_server(&url, || Response::new(Body::from("hello, world")));
    let ctx = (*bench_connection(&url)).0;
    tx_done.send(1).expect("Failed to signal done");
    assert_eq!(Some(12), ctx.doclen);
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn should_read_server_name() {
    let url = Url::parse("http://localhost:3000").expect("Invalid url");
    let (server, tx_done) = create_server(&url, || {
        Response::builder()
            .header("Server", "mysrv")
            .body(Body::from("foo"))
            .unwrap()
    });
    let ctx = (*bench_connection(&url)).0;
    tx_done.send(1).expect("Failed to signal done");
    assert_eq!(Some("mysrv".into()), ctx.server_name);
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn should_calculate_content_length_with_chunked_encoding() {
    let url = Url::parse("http://localhost:3000").expect("Invalid url");
    let (tx_started, rx_started) = channel();
    let (tx_done, rx_done) = oneshot::channel::<u8>();
    let addr = url.socket_addrs(|| None).unwrap()[0];
    let rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt build"),
    );
    let server = task::spawn(async move {
        let make_svc = make_service_fn(move |_conn| {
            let rt = rt.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |_| {
                    let rt = rt.clone();
                    async move {
                        let (mut sender, body) = Body::channel();
                        let resp = Response::builder().body(body).unwrap();
                        let rt = rt.clone();
                        rt.spawn(async move {
                            sender
                                .send_data(Bytes::from("hello, "))
                                .await
                                .expect("send data");
                            sender
                                .send_data(Bytes::from("world"))
                                .await
                                .expect("send data");
                        });
                        Ok::<_, Infallible>(resp)
                    }
                }))
            }
        });

        let s = Server::bind(&addr).serve(make_svc);
        if let Err(e) = tx_started.send(()) {
            eprintln!("Failed to signal server start: {}", e);
        }
        let graceful = s.with_graceful_shutdown(async {
            let signal = rx_done.await.expect("Failed to run test fasth enough");
            println!("Ready to shut down: {}", signal);
        });
        if let Err(e) = graceful.await {
            eprintln!("server error: {}", e);
        }
    });

    rx_started
        .recv_timeout(Duration::from_secs(2))
        .expect("Failed to start server fast enough");
    let conn = (*bench_connection(&url)).1;
    tx_done.send(1).expect("Failed to signal done");
    assert_eq!(12, conn.bytes_received);
    let _ = server.await;
}

fn bench_connection(url: &Url) -> Box<(Ctx, Connection<TcpStream>)> {
    let reporter = Rc::new(RefCell::new(Reporter::new(None)));
    let request = create_request(&url, false);
    let mut ctx = Ctx::new(request.into_bytes(), 1, 1).unwrap();
    let conn = Connection::new(
        &mut ctx,
        url.socket_addrs(|| None).unwrap()[0],
        Box::new(TcpStream::connect),
        reporter.clone(),
    )
    .expect("Failed to create connection");
    let token = conn.token;
    let mut connections = HashMap::new();
    connections.insert(conn.token, conn);

    benchmark(Duration::from_secs(5), &mut ctx, &mut connections, reporter)
        .expect("Failed benchmark");

    Box::new((ctx, connections.remove(&token).unwrap()))
}

fn create_server<R: 'static>(url: &Url, resp: R) -> (JoinHandle<()>, oneshot::Sender<u8>)
where
    R: Fn() -> Response<Body> + Send + Clone + Copy,
{
    let (tx_started, rx_started) = channel();
    let (tx_done, rx_done) = oneshot::channel::<u8>();
    let addr = url.socket_addrs(|| None).unwrap()[0];
    let handle = task::spawn(async move {
        let make_svc = make_service_fn(move |_conn| async move {
            Ok::<_, Infallible>(service_fn(
                move |_| async move { Ok::<_, Infallible>(resp()) },
            ))
        });

        let s = Server::bind(&addr).serve(make_svc);
        if let Err(e) = tx_started.send(()) {
            eprintln!("Failed to signal server start: {}", e);
        }
        let graceful = s.with_graceful_shutdown(async {
            let signal = rx_done.await.expect("Failed to run test fasth enough");
            println!("Ready to shut down: {}", signal);
        });
        if let Err(e) = graceful.await {
            eprintln!("server error: {}", e);
        }
    });

    rx_started
        .recv_timeout(Duration::from_secs(2))
        .expect("Failed to start server fast enough");
    (handle, tx_done)
}
