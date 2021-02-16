use ::std::{
    collections::HashMap,
    convert::TryInto,
    fs::File,
    io::Read,
    net::{SocketAddr, UdpSocket},
    time::SystemTime,
};

mod matchmaking;
use matchmaking::{matchmaking_tick, MatchmakingConfig};

mod stateless;
use stateless::handle_stateless;

mod stateful;
use stateful::{handle_stateful, messages::Messages};

pub struct Client {
    queued: Vec<Messages>,
    reliable: u8,
    sent: bool,
}

impl Client {
    /// Flip a bit in the reliable state
    pub fn flip_rel(&mut self, n: usize) {
        self.reliable ^= 1 << n;
    }

    /// Returns whether the client has been sent to another server
    pub fn been_sent(&self) -> bool {
        self.sent
    }

    /// Create a new client state
    pub fn new() -> Self {
        Self {
            queued: vec![],
            reliable: 0,
            sent: false,
        }
    }

    /// Indicates that the client has been sent to another server
    pub fn sent(&mut self) {
        self.sent = true;
    }
}

fn handle_request(
    clients: &mut HashMap<SocketAddr, Client>,
    sock: &mut UdpSocket,
    addr: SocketAddr,
    data: &[u8],
) {
    if data.len() > 4 {
        let header = u32::from_le_bytes(data[0..4].try_into().unwrap());
        if header == 0xFFFFFFFF {
            handle_stateless(sock, addr, data);
        } else if header != 0xFFFFFFFE {
            handle_stateful(clients, sock, addr, data);
        }
    }
}

fn main() {
    let mut mm_config = File::open("matchmaking.toml").unwrap();
    let mut buffer = String::new();
    mm_config.read_to_string(&mut buffer).unwrap();
    let config = ::toml::de::from_str::<MatchmakingConfig>(&buffer).unwrap();
    let mut clients = HashMap::<SocketAddr, Client>::new();
    let mut sock = UdpSocket::bind(config.bind_addr()).unwrap();
    let mut last_tick = SystemTime::now();
    loop {
        let mut buffer = vec![0; 1400];
        if let Ok((len, addr)) = sock.recv_from(&mut buffer) {
            handle_request(&mut clients, &mut sock, addr, &buffer[..len]);
            matchmaking_tick(&config, &mut last_tick, &mut clients);
        }
    }
}
