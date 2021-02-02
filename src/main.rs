use std::{
    collections::HashMap,
    convert::TryInto,
    fs::OpenOptions,
    io::Write,
    net::{SocketAddr, UdpSocket},
    time::SystemTime,
};

mod stateless;
use stateless::handle_stateless;

mod stateful;
use stateful::handle_stateful;

pub struct Client {
    reliable: u8,
}

impl Client {
    /// Flip a bit in the reliable state
    pub fn flip_rel(&mut self, n: usize) {
        self.reliable ^= 1 << n;
    }

    /// Create a new client state
    pub fn new() -> Self {
        Self { reliable: 0 }
    }
}

enum RequestType {
    Unknown,
    Stateless(u8),
    Stateful,
}

fn handle_request(
    clients: &mut HashMap<SocketAddr, Client>,
    sock: &mut UdpSocket,
    addr: SocketAddr,
    data: &[u8],
) -> RequestType {
    if data.len() > 4 {
        let header = u32::from_le_bytes(data[0..4].try_into().unwrap());
        if header == 0xFFFFFFFF {
            RequestType::Stateless(handle_stateless(sock, addr, data))
        } else if header != 0xFFFFFFFE {
            handle_stateful(clients, sock, addr, data);
            RequestType::Stateful
        } else {
            RequestType::Unknown
        }
    } else {
        RequestType::Unknown
    }
}

fn main() {
    let mut clients = HashMap::<SocketAddr, Client>::new();
    let mut log = OpenOptions::new().append(true).open("log.csv").unwrap();
    let mut sock = UdpSocket::bind("0.0.0.0:27015").unwrap();
    loop {
        let mut buffer = vec![0; 1400];
        if let Ok((len, addr)) = sock.recv_from(&mut buffer) {
            let start = SystemTime::now();
            let kind = handle_request(&mut clients, &mut sock, addr, &buffer[..len]);
            let time = SystemTime::now().duration_since(start).unwrap().as_micros();
            match kind {
                RequestType::Stateless(kind) => {
                    println!(
                        "{}: Stateless request({:#0x}) took {}\u{00B5}s",
                        addr, kind, time
                    );
                    write!(log, "{},{:#0x},{}\n", addr, kind, time).unwrap();
                }
                RequestType::Stateful => {
                    println!("{}: Stateful request took {}\u{00B5}s", addr, time);
                    write!(log, "{},,{}\n", addr, time).unwrap();
                }
                RequestType::Unknown => {
                    println!("{}: Unknown request took {}\u{00B5}s", addr, time);
                    write!(log, "{},,{}\n", addr, time).unwrap();
                }
            }
        }
    }
}
