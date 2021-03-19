use std::collections::HashMap;
use std::time::{Duration, Instant};

use mio::Token;
use url::Url;

use crate::connection::{ConnectionState, Ctx};

pub struct Reporter {
    connections: HashMap<Token, ConnectionStats>,
    started: Option<Instant>,
    finished: Option<Instant>,
}

struct ConnectionStats {
    state: State,
    times: Vec<Duration>,
}

#[derive(Debug)]
enum State {
    Unconnected,
    Connecting,
    Connected,
    Read(Instant),
}

impl Reporter {
    pub fn new() -> Self {
        Reporter {
            connections: HashMap::new(),
            started: None,
            finished: None,
        }
    }

    pub fn start(&mut self) {
        self.started = Some(Instant::now());
    }

    pub fn end(&mut self) {
        self.finished = Some(Instant::now());
    }

    pub fn connection_state_changed(&mut self, conn: &Token, new_state: &ConnectionState) {
        let stats = self.get_or_insert(conn);

        use ConnectionState::*;
        use State::*;
        match (&stats.state, new_state) {
            (Connected, READ) => {
                stats.state = Read(Instant::now());
            }
            (Read(started), UNCONNECTED) => {
                stats.times.push(Instant::now() - *started);
                stats.state = Unconnected;
            }
            (_, CONNECTED) => {
                stats.state = Connected;
            }
            (_, CONNECTING) => {
                stats.state = Connecting;
            }
            invalid => panic!(
                "Invalid state transition from {:?} to {:?}",
                invalid.0, invalid.1
            ),
        }
    }

    fn get_or_insert(&mut self, conn: &Token) -> &mut ConnectionStats {
        if !self.connections.contains_key(conn) {
            self.connections.insert(
                *conn,
                ConnectionStats {
                    state: State::Unconnected,
                    times: vec![],
                },
            );
        }
        self.connections
            .get_mut(conn)
            .expect("Just inserted, must be present!")
    }

    pub fn print(&self, url: &Url, ctx: &Ctx) {
        println!(
            "Server Software:\t{}",
            ctx.server_name.as_ref().unwrap_or(&String::new())
        );
        println!("Server Hostname:\t{}", url.host_str().unwrap());
        println!("Server Port:\t\t{}", url.port_or_known_default().unwrap());
        println!();

        println!("Concurrency Level:\t{}", ctx.concurrency);
        let time_spent = self.finished.unwrap() - self.started.unwrap();
        println!(
            "Time taken for tests:\t{}.{:03} seconds",
            time_spent.as_secs(),
            time_spent.as_millis() % 1000
        );
        println!(
            "Complete requests:\t{}",
            ctx.unsuccessful_responses + ctx.successful_responses
        );
        println!("Failed requests:\t{}", ctx.failed_responses);
        println!("Non-2xx responses:\t{}", ctx.unsuccessful_responses);

        println!();

        let mut all_times: Vec<Duration> = self
            .connections
            .iter()
            .flat_map(|(_, c)| c.times.clone())
            .collect();
        all_times.sort_unstable();

        if all_times.len() > 1 {
            println!("Percentage of the requests served within a certain time (ms)");

            for percentage in [50, 66, 75, 80, 90, 95, 98, 99].iter() {
                let idx = all_times.len() / 100 * percentage;
                println!("{}%\t{}", percentage, all_times[idx].as_millis());
            }
            if let Some(longest) = all_times.last() {
                println!("100%\t{} (longest request)", longest.as_millis());
            }
        }
    }
}
