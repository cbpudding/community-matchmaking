use bitbuffer::{BitReadBuffer, BitReadStream, BitWriteStream, LittleEndian};
use std::{
    collections::HashMap,
    convert::TryInto,
    net::{SocketAddr, UdpSocket},
};

use crate::Client;

mod util;
use util::*;

mod messages;
use messages::{process_messages, Messages};

pub fn handle_stateful(
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

        let read_buf = BitReadBuffer::new(&data[off..], LittleEndian);
        let mut reader = BitReadStream::new(read_buf);
        let mut writer = BitWriteStream::new(LittleEndian);
        if flags & 0x01 != 0 {
            // Check which bit in the reliable state we need to flip
            victim.flip_rel(reader.read_int(3).unwrap());
            // Read both subchannels
            for _ in 0..2 {
                handle_messages(parse_subchannel(&mut reader));
                // TODO: Write data
            }
        }
        handle_messages(process_messages(&mut reader));
    } else {
        println!("WARNING: Valve Checksum Failed!");
    }
}

fn handle_messages(messages: Vec<Messages>) -> Vec<Messages> {
    let mut results = Vec::new();
    for msg in messages {
        match msg {
            Messages::NET_SET_CONVARS { convars } => {
                if let Some(method) = convars.get("cl_connectmethod") {
                    if method == "" {
                        // TODO: We're good!
                    } else {
                        results.push(Messages::NET_DISCONNECT {
                            reason: "You must join this server from the favorites tab!".to_string()
                        });
                    }
                } else {
                    results.push(Messages::NET_DISCONNECT {
                        reason: "You must join this server from the favorites tab!".to_string()
                    });
                }
            }
            _ => println!("DEBUG: {:#?}", msg)
        }
    }
    results
}

fn parse_subchannel(reader: &mut BitReadStream<LittleEndian>) -> Vec<Messages> {
    // Check if the subchannel exists
    if reader.read_bool().unwrap() {
        // Is this part of a multi-block structure?
        let multi = reader.read_bool().unwrap();
        if multi {
            let start_fragment: u32 = reader.read_int(18).unwrap();
            let num_fragments: u8 = reader.read_int(3).unwrap();

            if start_fragment == 0 {
                // Is the fragment a file?
                let filename = if reader.read_bool().unwrap() {
                    // Tranfer id, filename
                    Some((
                        reader.read_int::<u32>(32),
                        reader.read_string(None).unwrap(),
                    ))
                } else {
                    None
                };

                // Is the fragment compressed?
                let compressed = if reader.read_bool().unwrap() {
                    unimplemented!("Compression is not yet implemented!");
                    Some(reader.read_int::<u32>(26))
                } else {
                    None
                };

                let length: u32 = reader.read_int(26).unwrap();

                todo!("Multi-reading");
            }

            vec![]
        } else {
            // Is the data compressed?
            let compressed = if reader.read_bool().unwrap() {
                unimplemented!("Compression is not yet implemented!");
                Some(reader.read_int::<u32>(26).unwrap())
            } else {
                None
            };

            // What is the length of the message?
            let len = read_varint(reader);
            // Finally, the message itself.
            let msg = reader.read_bytes(len).unwrap();

            let msg_buf = BitReadBuffer::new(&msg, LittleEndian);
            let mut msg_reader = BitReadStream::new(msg_buf);

            process_messages(&mut msg_reader)
        }
    } else {
        vec![]
    }
}
