use bitbuffer::{BitReadBuffer, BitReadStream, BitWriteStream, LittleEndian};
use std::{
    collections::HashMap,
    convert::TryInto,
    net::{SocketAddr, UdpSocket},
};

use crate::{Client, NetChannel};

mod util;
use util::*;

pub mod messages;
use messages::{process_messages, Messages};

pub fn handle_stateful(
    clients: &mut HashMap<SocketAddr, Client>,
    sock: &mut UdpSocket,
    addr: SocketAddr,
    data: &[u8],
) {
    // Get the client/victim
    let mut victim = match clients.get_mut(&addr) {
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
    let mut ack = u32::from_le_bytes(data[4..8].try_into().unwrap());
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
        if flags & 0x01 != 0 {
            // Check which bit in the reliable state we need to flip
            victim.flip_rel(reader.read_int(3).unwrap());
            // Read both subchannels
            for stream_num in 0..2 {
                let msgs = parse_subchannel(&mut reader, &mut victim.netchannels[stream_num]);
                println!("{:#?}", &msgs);
                let packets = build_packets(
                    handle_messages(&mut victim, msgs),
                    &mut ack,
                    seq,
                    victim.reliable,
                    challenge.unwrap(),
                );
                for packet in packets {
                    println!("Sending packet...");
                    sock.send_to(&packet, addr).unwrap();
                }
            }
        }
        let msgs = process_messages(&mut reader);
        println!("{:#?}", &msgs);
        let packets = build_packets(
            handle_messages(&mut victim, msgs),
            &mut ack,
            seq,
            rel,
            challenge.unwrap(),
        );
        for packet in packets {
            println!("Sending packet...");
            sock.send_to(&packet, addr).unwrap();
        }
    } else {
        println!("WARNING: Valve Checksum Failed!");
    }
}

fn build_packets(
    messages: Vec<Messages>,
    seq: &mut u32,
    ack: u32,
    rel: u8,
    challenge: u32,
) -> Vec<Vec<u8>> {
    let mut packets = vec![];
    *seq += 1;
    // TODO: Compression for sent packets?

    println!("Seq {}\nAck {}", seq, ack);

    for message in messages {
        let mut writer = BitWriteStream::new(LittleEndian);
        match message {
            Messages::NET_DISCONNECT { reason } => {
                writer.write_int(1u8, 6).unwrap();
                writer.write_string(&reason, None).unwrap();
            }
            Messages::SVC_PRINT { message } => {
                writer.write_int(7u8, 6).unwrap();
                writer.write_string(&message, None).unwrap();
            }
            Messages::SVC_STRING_CMD { command } => {
                writer.write_int(4u8, 6).unwrap();
                writer.write_string(&command, None).unwrap();
            }
            Messages::NET_NOP => {}
            _ => todo!("TODO: Serialize {:#?}", message),
        }

        // Encapsulate packet
        let packet = writer.finish();
        println!("Data: {:#?}", packet);

        let body = [&[rel] as &[u8], &challenge.to_le_bytes(), &packet].concat();
        println!("Body: {:#?}", body);

        let full_packet = [
            &seq.to_le_bytes() as &[u8],
            &ack.to_le_bytes(),
            &[0x20],
            &valve_checksum(&body).to_le_bytes(),
            &body,
        ]
        .concat();
        println!("{:#?}", full_packet.clone());

        packets.push(full_packet);
    }

    packets
}

fn handle_messages(client: &mut Client, messages: Vec<Messages>) -> Vec<Messages> {
    let mut results = Vec::new();
    for msg in messages {
        match msg {
            Messages::NET_SET_CONVARS { convars } => {
                if let Some(method) = convars.get("cl_connectmethod") {
                    if method == "serverbrowser_favorites" {
                        results.append(&mut client.queued);
                    } else {
                        results.push(Messages::NET_DISCONNECT {
                            reason: "You must join this server from the favorites tab!".to_string(),
                        });
                    }
                } else {
                    results.push(Messages::NET_DISCONNECT {
                        reason: "You must join this server from the favorites tab!".to_string(),
                    });
                }
            }
            _ => println!("DEBUG: {:#?}", msg),
        }
    }
    results
}

fn parse_subchannel(
    reader: &mut BitReadStream<LittleEndian>,
    netchannel: &mut NetChannel,
) -> Vec<Messages> {
    // Check if the subchannel exists
    if reader.read_bool().unwrap() {
        // Is this part of a multi-block structure?
        let multi = reader.read_bool().unwrap();
        if multi {
            let start_fragment: u32 = reader.read_int(18).unwrap();
            let num_fragments: u8 = reader.read_int(3).unwrap();
            println!("Start: {}, Num: {}", start_fragment, num_fragments);

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

                let total_length: u32 = reader.read_int(26).unwrap();

                let mut total_fragments = total_length as usize / 256;
                if total_length % 256 != 0 {
                    total_fragments += 1;
                }

                *netchannel = NetChannel {
                    fragments: vec![vec![]; total_fragments],
                    num_fragments: total_fragments,
                    compressed: compressed.is_some(),
                    length: total_length as usize,
                };

                println!("len: {}", netchannel.fragments.len());
                println!("tlen: {}", netchannel.length);

                for i in 0..num_fragments {
                    let fragment = reader.read_bytes(256).unwrap();
                    netchannel.fragments[start_fragment as usize + i as usize] = fragment.to_vec();
                }
            } else if start_fragment as usize + num_fragments as usize == netchannel.num_fragments {
                for i in 0..num_fragments - 1 {
                    println!("Reading...");
                    let fragment = reader.read_bytes(256).unwrap();
                    netchannel.fragments[start_fragment as usize + i as usize] = fragment.to_vec();
                }
                netchannel.fragments[start_fragment as usize + num_fragments as usize - 1] =
                    reader.read_bytes(netchannel.length % 256).unwrap().to_vec();
            } else {
                for i in 0..num_fragments {
                    let fragment = reader.read_bytes(256).unwrap();
                    netchannel.fragments[start_fragment as usize + i as usize] = fragment.to_vec();
                }
            }

            // Check if all fragments have arived
            let mut done = true;
            for fragment in &netchannel.fragments {
                if fragment.len() == 0 {
                    done = false;
                }
            }
            if done {
                let mut data = vec![];
                for fragment in &netchannel.fragments {
                    data.extend(fragment);
                }
                let msg_buf = BitReadBuffer::new(&data, LittleEndian);
                let mut msg_reader = BitReadStream::new(msg_buf);

                let msgs = process_messages(&mut msg_reader);
                println!("{:#?}", msgs);
                msgs
            } else {
                vec![Messages::NET_NOP]
            }
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

            let msgs = process_messages(&mut msg_reader);
            println!("{:#?}", &msgs);
            msgs
        }
    } else {
        vec![]
    }
}
