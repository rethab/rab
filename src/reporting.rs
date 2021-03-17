use std::collections::HashMap;
use std::time::Duration;

use mio::Token;

use crate::connection::{Connection, Ctx};

pub fn report(time_spent: Duration, ctx: &Ctx, connections: HashMap<Token, Connection>) {
    println!(
        "Took {}s {}ms",
        time_spent.as_secs(),
        time_spent.as_millis() % 1000
    );
    println!(
        "Sent {} requests over {} connections",
        ctx.sent_requests,
        connections.len()
    );

    let mut all_times: Vec<Duration> = connections
        .iter()
        .flat_map(|(_, c)| c.times.clone())
        .collect();
    all_times.sort_unstable();

    println!("Percentage of the requests served within a certain time (ms)");
    for percentage in [50, 66, 75, 80, 90, 95, 98, 99].iter() {
        let idx = all_times.len() / 100 * percentage;
        println!("{}%\t{}", percentage, all_times[idx].as_millis());
    }
    if let Some(longest) = all_times.last() {
        println!("100%\t{} (longest request)", longest.as_millis());
    }
}