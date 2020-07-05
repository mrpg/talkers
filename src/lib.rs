//! This crate contains the mechanics for building a simple TCP chat with support for multiple connections at once. Messages with length of up to 1048576 octets are supported, as well as much larger file transfers. Integrity checking is embedded into *talkers*. Included is a sample high-latency chat application (the *talkers* chat program) that supports proxying over SOCKS5 (e.g. to use Tor onion services).
//!
//! A "message" is any valid UTF-8 string (of up to 1048576 octets); a "file" is any string of octets. *talkers* allows customization using closures or function pointers that are invoked when certain events occur.
//!
//! This library is in an early stage and very much a work in progress. There might be major breaking changes as well as missing features and bugs. All contributions and forks are appreciated.

use std::cmp::min;
use std::convert::TryInto;
use std::fs::File;
use std::io::prelude::*;
use std::io::{Error, ErrorKind, Result};
use std::net::{Shutdown, TcpStream};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::Digest;

type Hash = [u8; 32];

/// This struct contains the connection to one *talkers* peer. It must be constructed with `Talker::new(s)`, but the callbacks in the public fields can be set directly.
pub struct Talker {
    s: TcpStream,
    queue: Option<u8>,
    closed: bool,

    /// Invoked when the connection is closed.
    pub chat_close: Option<Box<dyn Fn() + Send>>,

    /// Invoked when a new message is received.
    pub msg_new: Option<Box<dyn Fn(String) + Send>>,

    /// Invoked when a file transfer has been announced by the peer. Called with the announced size. Must return a bool indicating whether or not to accept the file transfer. By default, file transfers are not accepted (except in the example app).
    pub file_incoming: Box<dyn Fn(usize) -> bool + Send>,

    /// Invoked when a file transfer has failed. Called with the name of the transfer file and the error.
    pub file_failed: Option<Box<dyn Fn(String, Error) + Send>>,

    /// Invoked when a file transfer has succeeded. Called with the name of the transfer file.
    pub file_complete: Option<Box<dyn Fn(String) + Send>>,

    /// Invoked upon learning the intended hash of the file from the peer.
    pub file_hash_by_peer: Option<Box<dyn Fn(String, Hash) + Send>>,

    /// Invoked upon having calculated the hash of the received file.
    pub file_our_hash: Option<Box<dyn Fn(String, Hash) + Send>>,

    /// Invoked with the hash of the message or file that we sent.
    pub hash_of_sent: Option<Box<dyn Fn(Hash) + Send>>,

    /// Invoked upon receiving a hash from the peer.
    pub hash_rcvd: Option<Box<dyn Fn(Hash) + Send>>,

    /// Invoked if the peer tried to send a message or file that is too large.
    pub payload_too_large: Option<Box<dyn Fn(usize) + Send>>,

    /// Invoked if the peer sent an invalid instruction. Useful for debugging.
    pub invalid_instr: Option<Box<dyn Fn(u8) + Send>>,
}

impl Talker {
    /// Constructs a new `Talker` instance from a TcpStream. The callbacks are set to "do nothing", and to reject file transfers.
    pub fn new(s: TcpStream) -> Self {
        Talker {
            s,
            queue: None,
            closed: false, // assumes that the connection is initially open
            chat_close: None,
            msg_new: None,
            file_incoming: Box::new(|_| false),
            file_failed: None,
            file_complete: None,
            file_hash_by_peer: None,
            file_our_hash: None,
            hash_of_sent: None,
            hash_rcvd: None,
            invalid_instr: None,
            payload_too_large: None,
        }
    }

    /// Shuts down the connection with a *talkers* peer.
    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        } else if let Some(ref f) = self.chat_close {
            f();
            self.closed = true;
        }

        self.s.shutdown(Shutdown::Both)
    }

    /// Reads from the *talkers* peer and checks whether the buffer read is a *talkers* handshake. Should be invoked if a connection was made with us.
    pub fn expect_handshake(&mut self) -> Result<()> {
        let mut buf = [0; 8];

        self.s.read_exact(&mut buf)?;

        if &buf == b"/talkers" {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::InvalidData, "Invalid handshake"))
        }
    }

    /// Performs our half of the *talkers* handshake with the peer. Should be invoked if we initiated the connection or if we received a handshake.
    pub fn perform_handshake(&mut self) -> Result<()> {
        self.s.write_all(b"/talkers")
    }

    /// Reads precisely one instruction from the peer and process it accordingly.
    pub fn read_once(&mut self) -> Result<bool> {
        let mut instr = [0; 1];

        if let Some(ch) = self.queue {
            instr[0] = ch;
        } else {
            let n = match self.s.read(&mut instr[0..1]) {
                Ok(m) => m,
                Err(e) => match e.kind() {
                    ErrorKind::WouldBlock => return Ok(false),
                    _ => return Err(e),
                },
            };

            if n == 0 {
                return Err(Error::new(
                    ErrorKind::NotConnected,
                    "Lost connection with peer",
                )); // todo?
            }
        }

        let instr = instr[0];
        let mut msg = Vec::new();

        let mut is_file = false;
        let mut skip = true;

        let mut hasher = sha2::Sha256::new();
        let mut fp;

        fp = None;

        if instr == 33 || instr == 35 {
            // message or file
            if instr == 35 {
                // is file
                is_file = true;
            }
            let mut n_bytes = 0;
            let mut j = 1;

            self.s
                .set_nonblocking(false)
                .expect("Could not set TcpStream to blocking");

            let mut ch = self.s.try_clone()?.bytes();
            let mut filen = String::new();

            loop {
                // read length of payload until space or newline
                if let Some(Ok(ch)) = ch.next() {
                    if (ch >= 48 && ch <= 57) || ch == 10 || ch == 32 {
                        if ch == 10 || ch == 32 {
                            skip = false; // everything seems ok so far
                            break; // stop reading length
                        } else {
                            n_bytes *= 10;
                            n_bytes += usize::from(ch - 48);
                        }
                    } else {
                        break;
                    }
                }

                j += 1;

                if j >= 16 {
                    // maximum payload length is approx. 10000 TB
                    break;
                }
            }

            if !skip && is_file {
                skip = !(self.file_incoming)(n_bytes);
            }

            if !skip && is_file {
                filen = format!(
                    "transfer_{}",
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                );

                if let Ok(f) = File::create(&filen) {
                    fp = Some(f);
                } else if let Some(ref f) = self.file_failed {
                    f(
                        filen.clone(),
                        Error::new(ErrorKind::PermissionDenied, "Could not open transfer file"),
                    );
                }

                let mut buf = [0; 1024];

                while let Ok(()) = self.s.read_exact(&mut buf[..min(n_bytes, 1024)]) {
                    // read from stream
                    let n = min(n_bytes, 1024);

                    n_bytes -= n;

                    if is_file {
                        // basically the same as above, but from the fresh buffer
                        if let Some(ref mut fp) = fp {
                            if fp.write_all(&buf[..n]).is_err() {
                                if let Some(ref f) = self.file_failed {
                                    f(
                                        filen.clone(),
                                        Error::new(
                                            ErrorKind::PermissionDenied,
                                            "Could not write to transfer file",
                                        ),
                                    );
                                }
                            }
                        }

                        hasher.update(&buf[..n]);
                    }

                    if n_bytes == 0 {
                        break;
                    }
                }

                if let Some(ref f) = self.file_complete {
                    f(filen.clone());
                }

                if let Ok(()) = self.s.read_exact(&mut buf[..33]) {
                    if let Some(ref f) = self.file_hash_by_peer {
                        f(filen.clone(), buf[1..33].try_into().unwrap());
                    }
                }
            } else if !skip && !is_file {
                if n_bytes <= 1024 * 1024 {
                    msg.resize(n_bytes, 0);

                    if let Ok(()) = self.s.read_exact(&mut msg[..n_bytes]) {
                        hasher.update(&msg);

                        // message finished
                        if let Some(ref f) = &self.msg_new {
                            f(String::from_utf8_lossy(&msg).into_owned());
                        }

                        // clear message
                        msg.clear();
                    }
                } else {
                    // payload too large
                    if let Some(ref f) = &self.payload_too_large {
                        f(n_bytes);
                    }
                }
            }

            let mut entire_hash = vec![61];
            entire_hash.extend_from_slice(&hasher.finalize());

            self.s
                .write_all(&entire_hash)
                .expect("Could not send hash to peer");

            if is_file {
                if let Some(ref f) = &self.file_our_hash {
                    f(filen, entire_hash[1..].try_into().unwrap());
                }
            }

            return Ok(true);
        } else if let Some(ref f) = &self.invalid_instr {
            f(instr);
        }

        Ok(false)
    }

    /// Sets the TCP connection to non-blocking and invokes `read_once`. This has the effect that a instruction might be read from the peer or not. If one is read, it will be processed in blocking mode. If not, this function returns immediately without blocking. Useful if called in a loop. Note that each invocation reads and processes at most one instruction.
    pub fn read_maybe(&mut self) -> Result<bool> {
        self.s.set_nonblocking(true)?;

        let ret = self.read_once();
        self.s.set_nonblocking(false)?;

        ret
    }

    /// Instructs the peer that a message will be forthcoming and transmits the message.
    pub fn send(&mut self, msg: &str) -> Result<()> {
        let mut hasher = sha2::Sha256::new();

        self.s.write_all(format!("!{}\n", msg.len()).as_bytes())?;
        self.s.write_all(msg.as_bytes())?;

        hasher.update(msg.as_bytes());

        if let Some(ref f) = self.hash_of_sent {
            f(hasher.finalize().try_into().unwrap());
        }

        Ok(())
    }

    /// Send a stream to the peer. While this method technically accepts all streams that implement `Read`, *talkers* currently only has dedicated support for files.
    pub fn send_stream<T, U>(&mut self, stream: &mut T, len: U) -> Result<()>
    where
        T: Read,
        U: std::fmt::Display,
    {
        let mut hasher = sha2::Sha256::new();
        let mut buf = [0; 1024];

        self.s.write_all(format!("#{}\n", len).as_bytes())?;

        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            self.s.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
        }

        let mut entire_hash = vec![61];
        entire_hash.extend_from_slice(&hasher.finalize());
        self.s.write_all(&entire_hash)?;

        if let Some(ref f) = self.hash_of_sent {
            f(entire_hash[1..].try_into().unwrap());
        }

        Ok(())
    }

    /// Blocks until a hash has been received. If no hash, but some other instruction, is received, that instruction is written into an internal queue so that it can be processed by subsequent calls to `read_once`. Returns `Ok(())` if a hash was received and an Err variant if not.
    pub fn expect_hash(&mut self) -> Result<()> {
        self.s.set_nonblocking(false)?;

        let mut buf = [0; 33];

        if let Ok(()) = self.s.read_exact(&mut buf[..1]) {
            if buf[0] == b'=' {
                self.s.read_exact(&mut buf[1..])?;

                if let Some(ref f) = self.hash_rcvd {
                    f(buf[1..33].try_into().unwrap());
                }

                return Ok(());
            } else {
                self.queue = Some(buf[0] as u8);
            }
        }

        Err(Error::new(ErrorKind::Other, "No hash transmitted"))
    }
}
