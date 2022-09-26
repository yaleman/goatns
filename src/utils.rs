
// use bit_vec::{self, BitVec};

/// gets the query ID from the question packets
pub fn get_query_id(packets: &[u8]) -> u16 {
    get_u16_from_packets(packets, 0)
}
/// gets a u16 based on the bit start point
pub fn get_u16_from_packets(packets: &[u8], start_point: usize) -> u16 {
    let mut result_bytes: [u8;2] = [0,0];
    let end_point: usize = start_point + 2;
    result_bytes.copy_from_slice(&packets[start_point..end_point]);
    u16::from_be_bytes(result_bytes)
}
// gets a u8 based on the bit start point
// pub fn get_u8_from_bits(bits: &BitVec, start_point: usize, bit_count: usize) -> u8 {
//     let mut output: u8 = 0;
//     for index in start_point..start_point + bit_count {
//         output = output << <u8>::from(bits.get(index).unwrap());
//         println!("{} {}", bits.get(index).unwrap(), output);
//     }
//     output
// }
