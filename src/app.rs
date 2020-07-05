//! A simple example of a chat app with SOCKS5 support.
use std::fs;
use std::io::Result;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time;

use std::io::stdin;
use std::net::{SocketAddr, TcpListener};

use socks::Socks5Stream;

type Chat = Arc<Mutex<talkers::Talker>>;
type Chats = Arc<Mutex<Vec<(usize, Chat)>>>;

/// Listens on a port, waits for and dispatches connections.
///
/// # Examples
///
/// Basic usage:
///
/// ```no_run
/// use talkers::app;
/// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
///
//
/// let bind_to = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 50505)); // bind on 0.0.0.0:50505
/// let proxy = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 9150)); // use SOCKS5 proxy on port 9150
///
/// app::start_server(bind_to, Some(proxy));
/// ```
pub fn start_server(bind_to: SocketAddr, proxy: Option<SocketAddr>) {
    let listener = TcpListener::bind(bind_to).expect("Could not listen on port");

    let chats = Arc::new(Mutex::new(vec![]));

    let cloned_chats = Arc::clone(&chats);

    thread::spawn(move || handle_commands(proxy, cloned_chats));

    eprintln!("Listening on {}.", bind_to);
    if let Some(proxy) = proxy {
        eprintln!("Using SOCKS5 proxy on {}.", proxy);
    }
    eprintln!("Type `/help` for a list of accepted commands.");

    for stream in listener.incoming() {
        if let Ok(s) = stream {
            new_connection(s, Arc::clone(&chats), false);
        }
    }
}

fn try_parse(buf: &str) -> Option<(usize, usize)> {
    let mut si = buf.trim().split(' ');

    if let Some(zs) = si.next() {
        if let Ok(z) = zs.parse::<usize>() {
            return Some((z, zs.len() + 1));
        }
    }

    None
}

fn handle_commands(proxy: Option<SocketAddr>, chats: Chats) {
    let mut buf = String::new();

    while stdin().read_line(&mut buf).is_ok() {
        if buf.starts_with("/new ") {
            if let Some(proxy) = proxy {
                if let Ok(ts) = Socks5Stream::connect(proxy, buf[5..].trim()) {
                    new_connection(ts.into_inner(), Arc::clone(&chats), true);
                } else {
                    eprintln!("Could not connect to remote socket via proxy.");
                }
            } else if let Ok(s) = TcpStream::connect(buf[5..].trim()) {
                new_connection(s, Arc::clone(&chats), true);
            } else {
                eprintln!("Could not connect to remote socket.");
            }
        } else if buf.starts_with("/file ") {
            if let Some((dest, offset)) = try_parse(&buf[6..]) {
                let filen = &buf[(offset + 6)..].trim_end();

                if let Ok(fm) = fs::metadata(filen) {
                    eprintln!("{} : Sending `{}` ({} octets) …", dest, filen, fm.len());
                    eprintln!("{} : (Until complete, you can't enter new commands.)", dest);

                    if send_file(Arc::clone(&chats), dest, filen, fm.len()).is_err() {
                        eprintln!("{} : The file could not be sent.", dest);
                    }
                } else {
                    eprintln!(
                        "{} : File `{}` could not be opened for reading. Ignoring.",
                        dest, filen
                    );
                }
            } else {
                eprintln!("You must use /file like this: `/file 2 file.ext`.");
            }
        } else if buf.starts_with("/close ") {
            if let Some((id, _)) = try_parse(&buf[7..]) {
                terminate(Arc::clone(&chats), id);
            } else {
                eprintln!("You must use /close like this: `/close 4`.");
            }
        } else if let Some((dest, offset)) = try_parse(&buf[1..]) {
            if send(Arc::clone(&chats), dest, &buf[(offset + 1)..]).is_err() {
                terminate(Arc::clone(&chats), dest);
            }
        } else if buf.starts_with("/help") {
            eprintln!("/--------------------------------------------------------------------\\");
            eprintln!("|  /new host:port       Connects to a talkers instance at host:port  |");
            eprintln!("|  /close k             Terminates the connection with chat k.       |");
            eprintln!("|  /file k file.ext     Sends the file `file.ext` to chat k.         |");
            eprintln!("|  /k message           Sends the message `message` to chat k.       |");
            eprintln!("\\--------------------------------------------------------------------/");
        } else {
            eprintln!("Invalid command. Ignoring. Type `/help` for help.");
        }

        buf.clear();
    }
}

fn new_connection(s: TcpStream, chats: Chats, inited_by_us: bool) {
    let peer = s.peer_addr().unwrap();

    let t1 = Arc::new(Mutex::new(talkers::Talker::new(s)));
    let t2 = Arc::clone(&t1);
    let t3 = Arc::clone(&t2);

    if let Ok(mut t) = t1.lock() {
        if (!inited_by_us && t.expect_handshake().is_ok() && t.perform_handshake().is_ok())
            || (inited_by_us && t.perform_handshake().is_ok() && t.expect_handshake().is_ok())
        {
            if let Some(id) = insert_as_next(chats, t2) {
                set_example_handlers(&mut t, id);

                println!("{} : Connection established with {}.", id, peer);
            }
        } else {
            return;
        }
    } else {
        return;
    }

    thread::spawn(move || {
        loop {
            {
                if let Ok(mut t) = t3.lock() {
                    if t.read_maybe().is_err() {
                        break;
                    }
                }
            } // unlock mutex (avoid deadlocks)
            thread::sleep(time::Duration::from_millis(125));
        }

        let _ = t3.lock().unwrap().close();
    });
}

fn terminate(chats: Chats, id: usize) {
    let mut chats = chats.lock().expect("Could not lock chats mutex");

    for (i, ref mut t) in chats.iter_mut() {
        if *i == id {
            let _ = t.lock().unwrap().close();

            break;
        }
    }
}

fn send(chats: Chats, id: usize, msg: &str) -> Result<()> {
    let mut chats = chats.lock().expect("Could not lock chats mutex");

    for (i, ref mut t) in chats.iter_mut() {
        if *i == id {
            t.lock().unwrap().send(msg)?;

            t.lock().unwrap().expect_hash()?;

            break;
        }
    }

    Ok(())
}

fn send_file(chats: Chats, id: usize, filen: &str, fsize: u64) -> Result<()> {
    let mut chats = chats.lock().expect("Could not lock chats mutex");

    for (i, ref mut t) in chats.iter_mut() {
        if *i == id {
            let mut fp = fs::File::open(filen)?;

            t.lock().unwrap().send_stream(&mut fp, fsize)?;

            t.lock().unwrap().expect_hash()?;

            break;
        }
    }

    Ok(())
}

fn insert_as_next(chats: Chats, talker: Chat) -> Option<usize> {
    let mut chats = chats.lock().ok()?;
    let this_id = if let Some((z, _)) = chats.last() {
        z + 1
    } else {
        1
    };

    chats.push((this_id, talker));

    Some(this_id)
}

/// These are example handlers for the app. Feel free to use and adapt them for your own projects.
fn set_example_handlers(t: &mut talkers::Talker, id: usize) {
    t.chat_close = Some(Box::new(move || println!("{} : Closed.", id)));
    t.msg_new = Some(Box::new(move |msg| println!("{} > {}", id, msg.trim_end())));
    t.file_incoming = Box::new(move |fsize| {
        println!(
            "{} : Incoming file transfer of {} octets. Accepting.",
            id, fsize
        );

        true // accept all file transfers
    });
    t.file_failed = Some(Box::new(move |_, e| {
        println!("{} : File transfer failed: {}", id, e)
    }));
    t.file_complete = Some(Box::new(move |filen| {
        println!("{} : File transfer of `{}` complete.", id, filen)
    }));
    t.file_hash_by_peer = Some(Box::new(move |_, hash| {
        println!("{} = peer {:x?}", id, hash)
    }));
    t.file_our_hash = Some(Box::new(move |_, hash| {
        println!("{} = hash {:x?}", id, hash)
    }));
    t.hash_of_sent = Some(Box::new(move |hash| println!("{} = true {:x?}", id, hash)));
    t.hash_rcvd = Some(Box::new(move |hash| println!("{} = rcvd {:x?}", id, hash)));
}
