use a2s::A2SClient;
use serde::Deserialize;
use std::{
    collections::HashMap,
    error::Error,
    fmt::{self, Display, Formatter},
    net::{Ipv4Addr, SocketAddr},
    time::SystemTime,
};

use crate::{stateful::messages::Messages, Client};

#[derive(Deserialize)]
struct GenericOptions {
    address: Ipv4Addr,
    port: u16,
}

impl GenericOptions {
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

#[derive(Deserialize)]
pub struct MatchmakingConfig {
    matchmaking: GenericOptions,
    servers: HashMap<String, Server>,
}

impl MatchmakingConfig {
    pub fn bind_addr(&self) -> String {
        self.matchmaking.bind_addr()
    }
    pub fn port(&self) -> u16 {
        self.matchmaking.port
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash)]
struct Server {
    address: Ipv4Addr,
    port: u16,
}

impl Server {
    pub fn slots(&self) -> Result<usize, Box<dyn Error>> {
        // Query server info
        let client = A2SClient::new().unwrap();
        let info = client.info((self.address, self.port))?;
        // Return the number of empty slots on the server
        Ok(info.max_players as usize - info.players as usize)
    }

    pub fn score(&self) -> Result<isize, Box<dyn Error>> {
        // Query server info
        let client = A2SClient::new().unwrap();
        let info = client.info((self.address, self.port))?;
        // Score the server based on certain criteria
        let mut score = 0;
        // Reward servers for having players but reject full servers
        if info.players < info.max_players {
            return Err(Box::new(ServerError::ServerFull));
        } else if info.players >= 6 {
            score += info.players as isize;
        }
        score -= (info.max_players as isize - 24).abs(); // Punish servers from straying from the 24 maxplayer limit
        score -= info.bots as isize; // Remove one point per bot
        Ok(score)
    }
}

#[derive(Debug)]
enum ServerError {
    ServerFull,
}

impl Display for ServerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for ServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

pub fn matchmaking_tick(
    config: &MatchmakingConfig,
    last: &mut SystemTime,
    clients: &mut HashMap<SocketAddr, Client>,
) {
    let now = SystemTime::now();
    if now.duration_since(*last).unwrap().as_secs() >= 1 {
        *last = now;
        let mut scored = Vec::new();
        let players: Vec<&mut Client> = clients.values_mut().collect();
        for server in config.servers.values() {
            if let Ok(s) = server.score() {
                scored.push((server, s));
            }
        }
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        let servers: Vec<&Server> = scored.iter().map(|v| v.0).collect();
        // TODO: Look for candidates and redirect
        for p in players {
            println!("DEBUG(mm): Redirected!");
            p.queued.push(Messages::SVC_STRING_CMD {
                command: format!("redirect {}:{}", servers[0].address, servers[0].port),
            });
        }
    }
}
