use std::collections::HashMap;
use std::time::{Duration, Instant};

use mio::Token;
use url::Url;

use crate::connection::ConnectionState;
use crate::ctx::Ctx;

pub struct Reporter {
    heartbeatres: Option<usize>,
    done: usize,
    connections: HashMap<Token, ConnectionStats>,
    started: Option<Instant>,
    finished: Option<Instant>,
}

struct ConnectionStats {
    state: State,
    times: Vec<Duration>,
    ctimes: Vec<Duration>, // connection times
}

#[derive(Debug)]
enum State {
    Unconnected,
    Connecting(Instant),
    Connected,
    Read(Instant),
}

impl Reporter {
    pub fn new(heartbeatres: Option<usize>) -> Self {
        Reporter {
            heartbeatres,
            done: 0,
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
                self.done += 1;
                self.print_heartbeat();
            }
            (Connecting(started), CONNECTED) => {
                stats.ctimes.push(Instant::now() - *started);
                stats.state = Connected;
            }
            (_, CONNECTING) => {
                stats.state = Connecting(Instant::now());
            }
            invalid => panic!(
                "Invalid state transition from {:?} to {:?}",
                invalid.0, invalid.1
            ),
        }
    }

    fn print_heartbeat(&self) {
        if let Some(heartbeatres) = self.heartbeatres {
            if self.done % heartbeatres == 0 {
                println!("Completed {} requests", self.done);
            }
        }
    }

    fn get_or_insert(&mut self, conn: &Token) -> &mut ConnectionStats {
        if !self.connections.contains_key(conn) {
            self.connections.insert(
                *conn,
                ConnectionStats {
                    state: State::Unconnected,
                    times: vec![],
                    ctimes: vec![],
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

        println!("Document Path:\t{}", url.path());
        if let Some(doclen) = ctx.doclen {
            println!("Document Length:\t{} bytes", doclen);
        }
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
        self.print_connection_times();
        println!();
        self.print_response_times();
    }

    fn print_connection_times(&self) {
        let mut ctimes: Vec<Duration> = self
            .connections
            .iter()
            .flat_map(|(_, c)| c.ctimes.clone())
            .collect();

        if ctimes.is_empty() {
            return;
        }

        ctimes.sort_unstable();

        println!("Connection Times (ms)");
        println!("\t\tmin  mean[+/-sd] median   max");
        println!(
            "Connect:\t{: >3}{: >5.0}{: >6.1}{: >5}{: >10}",
            min(&ctimes),
            mean(&ctimes),
            std_dev(&ctimes),
            median(&ctimes),
            max(&ctimes)
        );
    }

    fn print_response_times(&self) {
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

fn min(times: &[Duration]) -> u128 {
    times.first().unwrap().as_millis()
}

fn mean(times: &[Duration]) -> f32 {
    let sum: Duration = times.iter().sum();
    sum.as_millis() as f32 / times.len() as f32
}

fn std_dev(times: &[Duration]) -> f32 {
    let mean = mean(times);
    let variance = times
        .iter()
        .map(|t| (t.as_millis() as f32 - mean).powf(2.0))
        .sum::<f32>()
        / (times.len() - 1) as f32;
    variance.sqrt()
}

fn median(times: &[Duration]) -> u128 {
    times[times.len() / 2].as_millis()
}

fn max(times: &[Duration]) -> u128 {
    times.last().unwrap().as_millis()
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_min() {
        assert_eq!(1, min(&[d(1), d(2), d(3)]));
        assert_eq!(2, min(&[d(2), d(3)]));
    }

    #[test]
    fn test_mean() {
        assert_eq!(2.0, mean(&[d(1), d(2), d(3)]));
        assert_eq!(2.5, mean(&[d(2), d(3)]));
        assert_eq!(5.0, mean(&[d(4), d(5), d(6)]));
        assert_eq!(6.0, mean(&[d(1), d(1), d(1), d(3), d(24)]));
        assert_eq!(5.5, mean(&[d(5), d(6)]));
    }

    #[test]
    fn test_std_dev() {
        assert_eq!(1.9148543, std_dev(&[d(3), d(5), d(7), d(7)]));
        assert_eq!(
            2.13809,
            std_dev(&[d(2), d(4), d(4), d(4), d(5), d(5), d(7), d(9)])
        );
    }

    #[test]
    fn test_median() {
        assert_eq!(1, median(&[d(1)]));
        assert_eq!(2, median(&[d(1), d(2)]));
        assert_eq!(2, median(&[d(1), d(2), d(3)]));
        assert_eq!(5, median(&[d(4), d(5), d(6)]));
        assert_eq!(1, median(&[d(1), d(1), d(1), d(3), d(24)]));
    }

    #[test]
    fn test_max() {
        assert_eq!(3, max(&[d(1), d(2), d(3)]));
        assert_eq!(3, max(&[d(2), d(3)]));
    }

    fn d(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }
}
