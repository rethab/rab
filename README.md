![main](https://github.com/rethab/rab/actions/workflows/rust.yml/badge.svg)


# RAB - Rust Apache Bench

The goal of this program is to become a drop-in replacement for [apache bench (ab)](https://en.wikipedia.org/wiki/ApacheBench).

## Background
This program is based on [mio](https://docs.rs/mio), which is a library for non-blocking IO.
On linux, that would be [epoll](https://man7.org/linux/man-pages/man7/epoll.7.html).

A notable difference with `ab`, and the reason I wrote this in the first place, is that `ab` does not start shooting at full throttle right away.
Instead, it first "tests" the connection by awaiting the first request. Only once that succeeds, the remaining connections are started ([read more](https://mail-archives.apache.org/mod_mbox/httpd-users/202103.mbox/browser)).

## Usage
Fire 10 requests over two connections:

```bash
cargo run -- -c 2 -n 10  "google.com"
```

Show Options:

```bash
cargo run -- --help
```

## Sample Output
```bash
> cargo run --quiet -- -c 100 -n 10000  "localhost:8080"
Concurrency Level:	100
Time taken for tests:	0.988 seconds
Complete requests:	10000
Failed requests:	0
Non-2xx responses:	0

Percentage of the requests served within a certain time (ms)
50%	9
66%	10
75%	10
80%	10
90%	10
95%	12
98%	13
99%	13
100%	17 (longest request)
```
