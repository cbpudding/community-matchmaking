use std::{
    convert::TryInto,
    error::Error,
    net::{SocketAddr, UdpSocket},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::matchmaking::MatchmakingConfig;

pub fn generate_challenge() -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    (now & 0xFFFFFFFF) as u32
}

pub fn handle_stateless(
    config: &MatchmakingConfig,
    sock: &mut UdpSocket,
    addr: SocketAddr,
    data: &[u8],
) -> Result<(), Box<dyn Error>> {
    let mut response = Vec::new();
    match data[4] {
        0x54 => {
            // A2S_INFO
            response.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            response.push(0x49); // Type
            response.push(0x11); // Protocol version
            response.extend_from_slice(format!("{}\0", config.hostname()).as_bytes()); // Server name
            response.extend_from_slice("matchmaking\0".as_bytes());
            response.extend_from_slice("tf\0".as_bytes()); // Game folder
            response.extend_from_slice("Team Fortress 2\0".as_bytes()); // Game name
            response.extend_from_slice(&440u16.to_le_bytes()); // Game ID
            response.push(0); // Number of players
            response.push(24); // Max players
            response.push(0); // Number of bots
            response.push(0x64); // Server type(Dedicated)
            response.push(0x6C); // Server environment(Linux)
            response.push(0); // Server visibility(Public)
            response.push(0); // VAC Support(Disabled)
            response.extend_from_slice("0\0".as_bytes()); // Game version
            response.push(0xA1); // Extra Data Flags
            response.extend_from_slice(&config.bind_addr().1.to_le_bytes()); // Port number
            response.extend_from_slice("breadpudding,matchmaking\0".as_bytes()); // Keywords
            response.extend_from_slice(&440u64.to_le_bytes()); // Game ID
        }
        0x55 => {
            // A2S_PLAYER
            response.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            let challenge = u32::from_le_bytes(data[5..9].try_into().unwrap());
            if challenge == 0xFFFFFFFF {
                response.push(0x41); // Type
                response.extend_from_slice(&generate_challenge().to_le_bytes());
            // Challenge
            } else {
                response.push(0x44); // Type
                response.push(0); // Number of players
            }
        }
        0x56 => {
            // A2S_RULES
            response.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            let challenge = u32::from_le_bytes(data[5..9].try_into().unwrap());
            if challenge == 0xFFFFFFFF {
                response.push(0x4A); // Type
                response.extend_from_slice(&generate_challenge().to_le_bytes());
            // Challenge
            } else {
                response.push(0x45); // Type
                response.push(0); // Number of rules
            }
        }
        0x6B => {
            // C2S_CONNECT
            let challenge = u32::from_le_bytes(data[17..21].try_into().unwrap());
            response.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            response.push(0x42); // Type
            response.extend_from_slice(&challenge.to_le_bytes()); // Challenge
            response.extend_from_slice("0000000000\0".as_bytes()); // Padding
        }
        0x71 => {
            // A2S_GETCHALLENGE
            let challenge = u32::from_le_bytes(data[5..9].try_into().unwrap());
            response.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            response.push(0x41); // Type
            response.extend_from_slice(&0x5A4F4933u32.to_le_bytes()); // Magic version
            response.extend_from_slice(&generate_challenge().to_le_bytes()); // Server's challenge
            response.extend_from_slice(&challenge.to_le_bytes()); // Client's challenge
            response.extend_from_slice(&3u32.to_le_bytes()); // Authentication method
            response.extend_from_slice(&0u16.to_le_bytes()); // Steam2 Encryption Key
            response.extend_from_slice(&0u64.to_le_bytes()); // Steam ID
            response.push(1); // SteamServer Secure
            response.extend_from_slice("000000\0".as_bytes()); // Padding
        }
        _ => {}
    }
    if response.len() > 0 {
        sock.send_to(&response, addr)?;
    }
    Ok(())
}
