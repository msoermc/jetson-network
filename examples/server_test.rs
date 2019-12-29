use async_std::task;
use async_std::sync::{channel, Sender};
use jetson_network::launch;
use jetson_network::prelude::{SendingMessage, SendingMessagePayload};
use std::time::Duration;
use log::LevelFilter;
use std::io::Write;
// {"payload":{"Drive":{"left":5,"right":5}}}

async fn ping(sender: Sender<SendingMessage>) {
    loop {
        sender.send(SendingMessage { payload: SendingMessagePayload::Test("Hello".to_string()) }).await;
        task::sleep(Duration::from_secs(1)).await;
    }
}

fn main() {
    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Debug)
        .format(|buf, record| {
            writeln!(buf, "[{}]\tModule: {}\tLine: {}\t{}",
                     record.level(),
                     record.module_path().expect("Couldn't find module path"),
                     record.line().expect("Couldn't find line"),
                     record.args())
        }).init();

    let (sr, rr) = channel(100);
    let (ss, rs) = channel(100);

    task::spawn(launch(sr, rs, "0.0.0.0:1776"));
    task::spawn(ping(ss));

    loop {
        let msg = task::block_on(rr.recv()).unwrap();
        println!("{:?}", msg);
    }
}