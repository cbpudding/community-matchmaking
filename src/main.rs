use bitbuffer::{BitReadBuffer, BitWriteStream, LittleEndian};
use crc::crc32;
use std::{
    collections::HashMap,
    convert::TryInto,
    fs::OpenOptions,
    io::Write,
    net::{SocketAddr, UdpSocket},
    time::{SystemTime, UNIX_EPOCH},
};

struct Client {
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

fn generate_challenge() -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    (now & 0xFFFFFFFF).try_into().unwrap()
}

fn handle_stateless(sock: &mut UdpSocket, addr: SocketAddr, data: &[u8]) -> u8 {
    let mut response = Vec::new();
    match data[4] {
        0x54 => {
            // A2S_INFO
            response.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            response.push(0x49); // Type
            response.push(0x11); // Protocol version
            response.extend_from_slice("Community Matchmaking Testing\0".as_bytes()); // Server name
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
            response.extend_from_slice(&27015u16.to_le_bytes()); // Port number
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
        sock.send_to(&response, addr).unwrap();
    }
    data[4]
}

fn handle_stateful(
    clients: &mut HashMap<SocketAddr, Client>,
    sock: &mut UdpSocket,
    addr: SocketAddr,
    data: &[u8],
) {
    // Get the client/victim
    let victim = match clients.get_mut(&addr) {
        Some(victim) => victim,
        None => {
            // Welcome our client to netchannel
            let mut buffer = Vec::new();
            buffer.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            buffer.push(0x42); // Type
            buffer.extend_from_slice("00000000000000\0".as_bytes()); // Padding
            sock.send_to(&buffer, addr).unwrap();
            // Create the client state since one doesn't exist
            clients.insert(addr, Client::new());
            clients.get_mut(&addr).unwrap()
        }
    };
    // Read header data
    let seq = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let ack = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let flags = data[8];
    let checksum = u16::from_le_bytes(data[9..11].try_into().unwrap());
    // Verify the checksum before we continue
    if valve_checksum(&data[11..]) == checksum {
        let rel = data[11];
        let mut off = 12;
        let choked = if flags & 0x10 != 0 {
            off += 1;
            Some(data[12])
        } else {
            None
        };
        let challenge = if flags & 0x20 != 0 {
            off += 4;
            Some(u32::from_le_bytes(data[(off - 4)..off].try_into().unwrap()))
        } else {
            None
        };
        if flags & 0x01 != 0 {
            // Set up the bit reader/writer
            let reader = BitReadBuffer::new(&data[off..], LittleEndian);
            let writer = BitWriteStream::new(LittleEndian);
            // Check which bit in the reliable state we need to flip
            victim.flip_rel(reader.read_int(0, 3).unwrap());
            // Check if data exists
            if reader.read_bool(3).unwrap() {
                let mut idx = 6;
                // Is this part of a multi-block structure?
                let multi = reader.read_bool(4).unwrap();
                println!("DEBUG: {:?}", multi);
                // Is the data compressed?
                let compressed = if reader.read_bool(5).unwrap() {
                    idx += 26;
                    Some(reader.read_int::<u32>(idx - 26, 26).unwrap())
                } else {
                    None
                };
                println!("DEBUG: {:?}", compressed);
                // What is the length of the message?
                let len = read_varint(&mut idx, &reader);
                // Finally, the message itself.
                let msg = reader.read_bytes(idx, len).unwrap();
                println!("DEBUG: {:?}", msg);
                // ...
            }
        }
    }

    // Attempt a stringcmd write
    //let string_cmd = [&[00], &[4u8] as &[u8], b"redirect", &[0]].concat(); // 4 (6 bits) + "redirect";
    let mut writer = BitWriteStream::new(LittleEndian);
    writer.write_int(0u8, 8).unwrap();
    writer.write_int(4u8, 6).unwrap();
    writer.write_bytes(b"redirect 192.168.0.2:27015").unwrap();
    writer.write_bytes(&[0]).unwrap();

    let string_cmd = writer.finish();

    let message = [
        &0u32.to_le_bytes() as &[u8],
        &0u32.to_le_bytes(),
        &[0x00],
        &valve_checksum(&string_cmd).to_le_bytes(),
        &string_cmd,
    ]
    .concat();

    sock.send_to(&message, addr).unwrap();
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

fn read_varint(idx: &mut usize, reader: &BitReadBuffer<LittleEndian>) -> usize {
    let mut count = 0;
    let mut result = 0;
    while {
        let temp = reader.read_int::<usize>(*idx, 8).unwrap();
        result |= (temp & 0x7F) << (7 * count);
        count += 1;
        *idx += 8;
        (temp & 0x80) != 0
    } {}
    result
}

fn valve_checksum(data: &[u8]) -> u16 {
    let mut result = crc32::checksum_ieee(data);
    result ^= result >> 16;
    result &= 0xFFFF;
    result as u16
}
