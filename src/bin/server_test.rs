use async_std::task;
use std::sync::mpsc::channel;
use jetson_network::launch;

// {"payload":{"Drive":{"left":5,"right":5}}}

fn main() {
    let (sr, rr) = channel();
    let (_ss, rs) = channel();

    task::spawn(launch(sr, rs));

    loop {
        let msg = rr.recv().unwrap();
        println!("{:?}", msg);
    }
}