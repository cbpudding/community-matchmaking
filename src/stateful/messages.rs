use bitbuffer::BitReadBuffer;
use bitbuffer::{BitReadStream, LittleEndian};
use std::{collections::HashMap, mem};

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum Messages {
    NET_NOP,
    NET_DISCONNECT {
        reason: String,
    },
    NET_SET_CONVARS {
        convars: HashMap<String, String>,
    },
    NET_SIGNON_STATE {
        state: u8,
        spawn_count: i32,
    },

    SVC_PRINT {
        message: String,
    },
    SVC_SERVER_INFO {
        protocol: u16,
        server_count: u32,
        hltv: bool,
        dedicated: bool,
        max_classes: u16,
        md5_map: [u8; 16],
        player_slot: u8,
        max_clients: u8,
        tick_interval: u32,
        os: char,
        game_dir: String,
        map_name: String,
        sky_name: String,
        host_name: String,
        replay: bool,
    },
    SVC_STRING_CMD {
        command: String,
    },
}

#[repr(u8)]
pub enum KeyValueTypes {
    TypeNone = 0,
    TypeString,
    TypeInt,
    TypeFloat,
    TypePtr,
    TypeWstring,
    TypeColor,
    TypeUint64,
    TypeNumtypes,
}

pub fn process_messages(reader: &mut BitReadStream<LittleEndian>) -> Vec<Messages> {
    let mut messages = vec![];

    loop {
        if reader.bits_left() < 6 {
            break;
        }
        let msg_type: u8 = reader.read_int(6).unwrap();
        println!("Msg type: {}", msg_type);
        match msg_type {
            // NET_NOP
            0 => messages.push(Messages::NET_NOP),
            // NET_DISCONNECT
            1 => {
                let reason = reader.read_string(None).unwrap();
                messages.push(Messages::NET_DISCONNECT {
                    reason: reason.to_string(),
                });
            }
            // NET_SET_CONVARS
            5 => {
                let num: u8 = reader.read_int(8).unwrap();

                let mut convars = HashMap::with_capacity(num.into());

                for _ in 0..num {
                    let key = reader.read_string(None).unwrap();
                    let value = reader.read_string(None).unwrap();

                    convars.insert(key.to_string(), value.to_string());
                }
                messages.push(Messages::NET_SET_CONVARS { convars });
            }
            // NET_SIGNON_STATE
            6 => {
                let state: u8 = reader.read_int(8).unwrap();
                let spawn_count: i32 =
                    unsafe { mem::transmute::<u32, i32>(reader.read_int(32).unwrap()) };

                messages.push(Messages::NET_SIGNON_STATE { state, spawn_count });
            }
            // SVC_PRINT
            7 => {
                let data = reader.read_string(None).unwrap();
                println!("{:#?}", data);
            }
            8 => {
                let protocol: u16 = reader.read_int(16).unwrap();
                let server_count: u32 = reader.read_int(32).unwrap();
                let is_hltv = reader.read_bool().unwrap();
                let dedicated = reader.read_bool().unwrap();
                reader.skip_bits(32).unwrap(); // Deprecated CRC checksum
                let max_classes: u16 = reader.read_int(16).unwrap();

                let md5_map = reader.read_bytes(16).unwrap();
                let player_slot: u8 = reader.read_int(8).unwrap();
                let max_clients: u8 = reader.read_int(8).unwrap();
                let tick_interval: u32 = reader.read_int(32).unwrap(); // Float - parse somehow
                let os: char = reader.read_int::<u8>(8).unwrap() as char;
                let game_dir = reader.read_string(None).unwrap();
                let map_name = reader.read_string(None).unwrap();
                let sky_name = reader.read_string(None).unwrap();
                let host_name = reader.read_string(None).unwrap();
                let is_replay = reader.read_bool().unwrap();

                println!("protocol: {}", protocol);
                println!("server count: {}", server_count);
                println!("hltv: {}", is_hltv);
                println!("dedicated: {}", dedicated);
                println!("max classes: {}", max_classes);
                println!("md5: {:?}", md5_map);
                println!("Player slot: {}", player_slot);
                println!("Max clients: {}", max_clients);
                println!("os: {}", os);
                println!("tick interval: {}", tick_interval);
                println!("game dir: {}", game_dir);
                println!("map name: {}", map_name);
                println!("sky name: {}", sky_name);
                println!("host name: {}", host_name);
                println!("Is replay: {}", is_replay);
            }
            // NET_TICK
            3 => {
                let tick: u32 = reader.read_int(32).unwrap();
                let host_frametime = reader.read_int::<u16>(16).unwrap() as f32 / 100000f32;
                let host_frametime_std = reader.read_int::<u16>(16).unwrap() as f32 / 100000f32;

                println!("tick: {}", tick);
                println!("frametime: {}", host_frametime);
                println!("frametime std: {}", host_frametime_std);
            }
            // SVC_CREATE_STRING_TABLE
            12 => {
                println!("--- STRINGTABLE ---");
                let filenames = reader.read_int::<u8>(8).unwrap() == ':' as u8;
                if !filenames {
                    reader
                        .set_pos(reader.bit_len() - reader.bits_left() - 8)
                        .unwrap();
                }
                println!("Is filenames: {}", filenames);
                println!("tablename: {}", reader.read_string(None).unwrap());
                let max_entries: u16 = reader.read_int(16).unwrap();
                println!("max entires: {}", max_entries);
                let bits = (|mut x: u16| {
                    let mut y = 0;
                    while (x >>= 1) == () && x != 0 {
                        y += 1;
                    }
                    y
                })(max_entries);
                let num_entries: u64 = reader.read_int(bits as usize + 1).unwrap();
                println!("num entries: {}", num_entries);

                let length = super::util::read_varint(reader);
                println!("Length: {}", length);

                let data_size: u16;
                let data_size_bits: u8;
                if reader.read_bool().unwrap() {
                    // Fixed size
                    data_size = reader.read_int(12).unwrap();
                    data_size_bits = reader.read_int(4).unwrap();
                } else {
                    data_size = 0;
                    data_size_bits = 0;
                }

                println!("Data size: {}", data_size);
                println!("Data size bits: {}", data_size_bits);

                let compressed = reader.read_bool().unwrap();
                println!("Compressed: {}", compressed);

                let mut data = reader.read_bits(length).unwrap();
                std::fs::write("test.bin", &data.read_bytes(data.bit_len() / 8).unwrap()).unwrap();
                println!("--- END STRINGTABLE ---")
            }
            // CLC_CmdKeyValues
            16 => {
                let byte_len: u32 = reader.read_int(32).unwrap();
                let data = reader.read_bytes(byte_len as usize).unwrap();

                let kv_buf = BitReadBuffer::new(&data, LittleEndian);
                let mut kv_reader = BitReadStream::new(kv_buf);

                loop {
                    println!("test");
                    let v_type: u8 = kv_reader.read_int(8).unwrap();
                    if v_type <= 8 {
                        panic!();
                    }
                    if v_type == unsafe { std::mem::transmute(KeyValueTypes::TypeNumtypes) } {
                        break;
                    }

                    let token = kv_reader.read_string(None).unwrap();
                    println!("Key: {}", token);

                    match unsafe { std::mem::transmute(v_type) } {
                        KeyValueTypes::TypeNone => {}

                        _ => {
                            panic!("Unimplemented: {}", v_type);
                        }
                    }
                }
            }
            _ => unimplemented!("MESSAGE TYPE: {}", msg_type),
        };
    }

    messages
}
