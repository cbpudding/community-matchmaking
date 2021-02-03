use bitbuffer::{BitReadStream, LittleEndian};
use std::{collections::HashMap, mem};

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum Messages {
    NET_NOP,
    NET_DISCONNECT { reason: String },
    NET_SET_CONVARS { convars: HashMap<String, String> },
    NET_SIGNON_STATE { state: u8, spawn_count: i32 },
}

pub fn process_messages(reader: &mut BitReadStream<LittleEndian>) -> Vec<Messages> {
    let mut messages = vec![];

    loop {
        if reader.bits_left() < 6 {
            break;
        }
        let msg_type: u8 = reader.read_int(6).unwrap();
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
            _ => unimplemented!("MESSAGE TYPE: {}", msg_type),
        }
    }

    messages
}
