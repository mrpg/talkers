# talkers

[![Crates.io](https://img.shields.io/crates/v/bus.svg)](https://crates.io/crates/talkers)
[![Documentation](https://docs.rs/bus/badge.svg)](https://docs.rs/talkers/)

See [the documentation] for more details.

  [the documentation]: https://docs.rs/talkers/

## Using the app

Simply run `cargo run --release`.

You can run `cargo run --release -- --help` to see which arguments are supported. For example, `cargo run --release -- -x 9150` would listen on 0.0.0.0:50505 for incoming connections (the default), but use the SOCKS5 proxy on port 9150 to connect to peers.

## How to use in your own project

To get started, it's easiest to take a look at the ["record" example] as well as the [app itself]. The app is a minimalist yet full-fledged CLI chat application that can connect using a SOCKS5 proxy. The ["record" example] waits for a connection and then records all messages received within a few seconds in a Vec.

  ["record" example]: examples/record.rs
  [app itself]: src/app.rs

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.