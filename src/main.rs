use std::{
    collections::HashMap,
    convert::TryInto,
    fs::OpenOptions,
    io::Write,
    net::{SocketAddr, UdpSocket},
    time::{SystemTime, UNIX_EPOCH},
};

mod state;

use state::State;

fn generate_challenge() -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    (now & 0xFFFFFFFF).try_into().unwrap()
}

fn handle_stateless(sock: &mut UdpSocket, addr: SocketAddr, data: &[u8]) -> Option<u8> {
    let mut response = Vec::new();
    match data[4] {
        0x54 => {
            // A2S_INFO
            response.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            response.push(0x49); // Type
            response.push(0x11); // Protocol version
            response.extend_from_slice("Community Matchmaking Testing".as_bytes()); // Server name
            response.push(0);
            response.extend_from_slice("matchmaking".as_bytes());
            response.push(0);
            response.extend_from_slice("tf".as_bytes()); // Game folder
            response.push(0);
            response.extend_from_slice("Team Fortress 2".as_bytes()); // Game name
            response.push(0);
            response.extend_from_slice(&440u16.to_le_bytes()); // Game ID
            response.push(0); // Number of players
            response.push(24); // Max players
            response.push(0); // Number of bots
            response.push(0x64); // Server type(Dedicated)
            response.push(0x6C); // Server environment(Linux)
            response.push(0); // Server visibility(Public)
            response.push(0); // VAC Support(Disabled)
            response.extend_from_slice("0".as_bytes()); // Game version
            response.push(0);
            response.push(0xA1); // Extra Data Flags
            response.extend_from_slice(&27015u16.to_le_bytes()); // Port number
            response.extend_from_slice("breadpudding,matchmaking".as_bytes()); // Keywords
            response.push(0);
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
            response.extend_from_slice("000000".as_bytes()); // Padding
            response.push(0);
        }
        _ => {}
    }
    if response.len() > 0 {
        sock.send_to(&response, addr).unwrap();
    }
    Some(data[4])
}

fn read_bit(data: &[u8], idx: &mut usize) -> bool {
    let victim = data[*idx >> 3] & (1 << (*idx & 3));
    *idx += 1;
    victim != 0
}

fn read_bits(data: &[u8], idx: &mut usize, n: usize) -> usize {
    let mut result = 0;
    for _ in 0..n {
        result = (result << 1) | (read_bit(data, idx) as usize);
    }
    result
}

fn read_bytes(data: &[u8], idx: &mut usize, n: usize) -> Vec<u8> {
    let mut result = Vec::new();
    for _ in 0..n {
        result.push(read_bits(data, idx, 8) as u8);
    }
    result
}

fn read_varint(data: &[u8], idx: &mut usize) -> usize {
    let mut count = 0;
    let mut result = 0;
    while {
        let temp = read_bits(data, idx, 8);
        result |= (temp & 0x7F) << (7 * count);
        count += 1;
        (temp & 0x80) != 0
    } {}
    result
}

fn handle_stateful(
    states: &mut HashMap<SocketAddr, State>,
    sock: &mut UdpSocket,
    addr: SocketAddr,
    data: &[u8],
) {
    let (state, new) = match states.get_mut(&addr) {
        Some(state) => (state, false),
        None => {
            states.insert(addr, State::new());
            (states.get_mut(&addr).unwrap(), true)
        }
    };
    // Send our last stateless packet if this is a new connection
    if new {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        buffer.push(0x42); // Type
        buffer.extend_from_slice("00000000000000\0".as_bytes()); // Padding
        sock.send_to(&buffer, addr).unwrap();
    }
    let mut response = Vec::new();
    let mut idx = 0; // Bit index
    let mut off = 12; // Subchannel offset
    let seq = u32::from_le_bytes(data[0..4].try_into().unwrap()); // Packet sequence
    let ack = u32::from_le_bytes(data[4..8].try_into().unwrap()); // Acknowledged sequence
    let flags = data[8];
    let checksum = u16::from_le_bytes(data[9..11].try_into().unwrap());
    state.set_rel(data[11]); // Set the client's relative state
    if flags & 0x10 != 0 {
        // Check if the choked flag is set
        // TODO: Handle choked packets
        off += 1;
        todo!()
    }
    if flags & 0x20 != 0 {
        // Check if we are being challenged
        // TODO: Don't skip the challenge
        off += 4;
        // We're ignoring this for now
    }
    // Check if we have subchannel data
    if flags & 0x01 != 0 {
        // Grab just the subchannel data
        let sub = &data[off..];
        // Update our reliable state
        state.flip_rel(read_bits(sub, &mut idx, 3));
        // Does the data exist?
        if read_bit(sub, &mut idx) {
            // Is the data fragmented?
            if read_bit(sub, &mut idx) {
                // Is the data compressed?
                if read_bit(sub, &mut idx) {
                    // TODO: Read compressed subchannel data
                    todo!()
                } else {
                    let len = read_varint(data, &mut idx);
                    let subdata = read_bytes(data, &mut idx, len);
                    println!("{:?}", subdata);
                    // ...
                    todo!()
                }
            } else {
                // TODO: Read fragmented subchannel data
                todo!()
            }
        }
    }
    sock.send_to(&response, addr).unwrap();
}

fn handle_request(
    state: &mut HashMap<SocketAddr, State>,
    sock: &mut UdpSocket,
    addr: SocketAddr,
    data: &[u8],
) -> Option<u8> {
    if data.len() > 4 {
        let header = u32::from_le_bytes(data[0..4].try_into().unwrap());
        if header == 0xFFFFFFFF {
            handle_stateless(sock, addr, data)
        } else if header != 0xFFFFFFFE {
            handle_stateful(state, sock, addr, data);
            None
        } else {
            None
        }
    } else {
        None
    }
}

fn main() {
    let mut log = OpenOptions::new().append(true).open("log.csv").unwrap();
    let mut sock = UdpSocket::bind("0.0.0.0:27015").unwrap();
    let mut state = HashMap::<SocketAddr, State>::new();
    loop {
        let mut buffer = vec![0; 1400];
        if let Ok((len, addr)) = sock.recv_from(&mut buffer) {
            let start = SystemTime::now();
            let kind = handle_request(&mut state, &mut sock, addr, &buffer[..len]);
            let time = SystemTime::now().duration_since(start).unwrap().as_micros();
            match kind {
                Some(kind) => {
                    println!("{}: Request({:#0x}) took {}\u{00B5}s", addr, kind, time);
                    write!(log, "{},{:#0x},{}\n", addr, kind, time).unwrap();
                }
                None => {
                    println!("{}: Unknown request took {}\u{00B5}s", addr, time);
                    write!(log, "{},,{}\n", addr, time).unwrap();
                }
            }
        }
    }
}
