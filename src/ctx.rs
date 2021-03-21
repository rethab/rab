use std::io;
use std::time::Duration;

use mio::event::Source;
use mio::{Events, Interest, Poll, Token};

pub struct Ctx<'a> {
    pub successful_responses: usize,
    pub unsuccessful_responses: usize,
    pub failed_responses: usize,
    pub sent_requests: usize,
    pub payload: &'a [u8],
    pub concurrency: usize,
    pub server_name: Option<String>,
    pub doclen: Option<usize>,
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
            doclen: None,
            max_requests,
            concurrency,
            payload,
        })
    }

    pub fn total_responses(&self) -> usize {
        self.failed_responses + self.successful_responses + self.unsuccessful_responses
    }

    pub fn expect_more_responses(&self) -> bool {
        self.total_responses() < self.max_requests
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

    pub fn register<S: Source>(&mut self, token: Token, source: &mut S) -> io::Result<()> {
        self.poll
            .registry()
            .register(source, token, Interest::READABLE | Interest::WRITABLE)
    }

    pub fn deregister<S: Source>(&self, source: &mut S) -> io::Result<()> {
        self.poll.registry().deregister(source)
    }

    pub fn next_token(&mut self) -> Token {
        let next = self.token.0;
        self.token.0 += 1;
        Token(next)
    }

    pub fn send_more(&self) -> bool {
        self.max_requests > self.sent_requests
    }
}
