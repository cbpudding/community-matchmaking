use ::chrono::Local;
use ::fern::Dispatch;
use ::log::{error, LevelFilter};
use std::{
    collections::HashMap,
    convert::TryInto,
    error::Error,
    fs::File,
    io::Read,
    net::{SocketAddr, UdpSocket},
    time::SystemTime,
};

use snap::raw::Decoder;

mod matchmaking;
use matchmaking::{matchmaking_tick, MatchmakingConfig};

mod stateless;
use stateless::handle_stateless;

mod stateful;
use stateful::{handle_stateful, messages::Messages};

pub struct NetChannel {
    fragments: Vec<Vec<u8>>,
    compressed: bool,
    num_fragments: usize,
    length: usize,
}

pub struct Client {
    joined: SystemTime,
    name: Option<String>,
    netchannels: [NetChannel; 2],
    queued: Vec<Messages>,
    reliable: u8,
    pub state: ClientState,
}

impl Client {
    /// Flip a bit in the reliable state
    pub fn flip_rel(&mut self, n: usize) {
        self.reliable ^= 1 << n;
    }

    /// Return the time the client joined
    pub fn joined(&self) -> SystemTime {
        self.joined
    }

    /// Returns the name of the client
    pub fn name(&self) -> Option<String> {
        self.name.clone()
    }

    /// Create a new client state
    pub fn new() -> Self {
        Self {
            joined: SystemTime::now(),
            name: None,
            queued: vec![],
            reliable: 0,
            netchannels: [
                NetChannel {
                    fragments: vec![],
                    num_fragments: 0,
                    length: 0,
                    compressed: false,
                },
                NetChannel {
                    fragments: vec![],
                    num_fragments: 0,
                    length: 0,
                    compressed: false,
                },
            ],
            state: ClientState::Fresh,
        }
    }

    /// Sets the name of the client
    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }
}

/// The state the client is currently in
#[derive(PartialEq)]
pub enum ClientState {
    Fresh, // New client, hasn't been confirmed yet.
    Confirmed, // Confirmed to have joined from the favorites tab.
    Redirected, // The client has been redirected to another server.
}

fn handle_request(
    config: &MatchmakingConfig,
    clients: &mut HashMap<SocketAddr, Client>,
    sock: &mut UdpSocket,
    addr: SocketAddr,
    data: &[u8],
) -> Result<(), Box<dyn Error>> {
    if data.len() > 4 {
        let header = u32::from_le_bytes(data[0..4].try_into().unwrap());
        if header == 0xFFFFFFFF {
            handle_stateless(config, sock, addr, data)?;
        } else if header == 0xFFFFFFFD {
            let mut decompressor = Decoder::new();
            let decompressed = decompressor.decompress_vec(&data[8..])?;

            handle_stateful(clients, sock, addr, &decompressed);
        } else if header != 0xFFFFFFFE {
            handle_stateful(clients, sock, addr, data);
        }
    }
    Ok(())
}

fn main() {
    Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}][{}] {}",
                Local::now().format("%H:%M:%S%.3f"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(if cfg!(debug_assertions) {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        })
        .chain(::std::io::stderr())
        .apply()
        .unwrap();
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
            if let Err(e) = handle_request(&config, &mut clients, &mut sock, addr, &buffer[..len]) {
                error!("{}", e);
            }
            matchmaking_tick(&config, &mut last_tick, &mut clients);
        }
    }
}
