//! Receive talkers messages for a few seconds and store them in a Vec.

use talkers;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let bind_to = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 50505));

    let listener = TcpListener::bind(bind_to).expect("Could not listen on port");

    let msgs = Arc::new(Mutex::new(vec![]));

    if let Some(Ok(s)) = listener.incoming().next() {
        let mut t = talkers::Talker::new(s);
        let msgs = Arc::clone(&msgs);
        t.msg_new = Some(Box::new(move |msg| {
            msgs.lock().unwrap().push((
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                msg,
            ))
        }));

        if t.expect_handshake().is_ok() && t.perform_handshake().is_ok() {
            thread::spawn(move || {
                for _ in 0..80 {
                    if t.read_maybe().is_err() {
                        eprintln!("debug: droppin' out");
                        break;
                    }

                    thread::sleep(time::Duration::from_millis(125));
                }
                let _ = t.close();
            })
            .join()
            .expect("Could not join thread");
        }

        eprintln!("closed connection");
    }

    eprintln!("Received messages:");

    for (time, el) in &*msgs.lock().unwrap() {
        eprintln!("{}\t{:?}", time, el);
    }
}
