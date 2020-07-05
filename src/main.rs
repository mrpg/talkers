mod app;

use std::env;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

fn main() {
    let mut bind_to = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 50505));
    let mut proxy = None;

    let mut args = env::args();
    let appname = args.next().unwrap();

    while let Some(arg) = args.next() {
        if arg == "-x" || arg == "--proxy" {
            if let Some(arg) = args.next() {
                if let Ok(b) = arg.parse::<SocketAddr>() {
                    proxy = Some(b);
                } else if let Ok(port) = arg.parse() {
                    proxy = Some(SocketAddr::V4(SocketAddrV4::new(
                        Ipv4Addr::new(127, 0, 0, 1),
                        port,
                    )));
                } else {
                    help(&appname);
                    panic!(
                        "Could not parse proxy address (should be something like `127.0.0.1:9150` or a port)."
                    );
                }
            } else {
                help(&appname);
                panic!("Please specify the proxy (e.g. `127.0.0.1:9150` or a port).");
            }
        } else if let Ok(b) = arg.parse() {
            bind_to = b;
        } else if let Ok(port) = arg.parse() {
            bind_to = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port));
        } else {
            help(&appname);
            panic!("Invalid argument `{}`.", arg);
        }
    }

    app::start_server(bind_to, proxy);
}

fn help(appname: &str) {
    eprintln!("talkers 0.1.0");
    eprintln!("-------------");
    eprintln!();
    eprintln!("USAGE:\t{} [-x [host:]port]] [[bhost:]bport]", appname);
    eprintln!();
    eprintln!("ARGUMENTS:");
    eprintln!("      -x [host:]port]:  Specifies a SOCKS5 proxy to be used.");
    eprintln!(" --proxy [host:]port]:  If only a port is specified, 127.0.0.1");
    eprintln!("                        is assumed as the host.");
    eprintln!();
    eprintln!("       [bhost:]bport]:  Specifies the address on which talkers");
    eprintln!("                        will bind. If only a port is specified,");
    eprintln!("                        talkers will bind on 0.0.0.0.");
    eprintln!();
}
