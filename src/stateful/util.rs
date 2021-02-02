use bitbuffer::{BitReadStream, LittleEndian};
use crc::crc32;

pub fn read_varint(reader: &mut BitReadStream<LittleEndian>) -> usize {
    let mut count = 0;
    let mut result = 0;
    while {
        let temp = reader.read_int::<usize>(8).unwrap();
        result |= (temp & 0x7F) << (7 * count);
        count += 1;
        (temp & 0x80) != 0
    } {}
    result
}

pub fn valve_checksum(data: &[u8]) -> u16 {
    let mut result = crc32::checksum_ieee(data);
    result ^= result >> 16;
    result &= 0xFFFF;
    result as u16
}
