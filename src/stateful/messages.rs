use bitbuffer::{BitReadStream, LittleEndian};
use log::error;
use std::error::Error;
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

pub fn process_messages(
    reader: &mut BitReadStream<LittleEndian>,
) -> Result<Vec<Messages>, Box<dyn Error>> {
    let mut messages = vec![];

    loop {
        if reader.bits_left() < 6 {
            break;
        }
        let msg_type: u8 = reader.read_int(6)?;
        match msg_type {
            // NET_NOP
            0 => messages.push(Messages::NET_NOP),
            // NET_DISCONNECT
            1 => {
                let reason = reader.read_string(None)?;
                messages.push(Messages::NET_DISCONNECT {
                    reason: reason.to_string(),
                });
            }
            // NET_SET_CONVARS
            5 => {
                let num: u8 = reader.read_int(8)?;

                let mut convars = HashMap::with_capacity(num.into());

                for _ in 0..num {
                    let key = reader.read_string(None)?;
                    let value = reader.read_string(None)?;

                    convars.insert(key.to_string(), value.to_string());
                }
                messages.push(Messages::NET_SET_CONVARS { convars });
            }
            // NET_SIGNON_STATE
            6 => {
                let state: u8 = reader.read_int(8)?;
                let spawn_count: i32 = unsafe { mem::transmute::<u32, i32>(reader.read_int(32)?) };

                messages.push(Messages::NET_SIGNON_STATE { state, spawn_count });
            }
            _ => {
                error!("An unknown message typ was encountered: {}", msg_type);
                return Ok(vec![]);
            }
        };
    }

    Ok(messages)
}
