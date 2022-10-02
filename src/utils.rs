use crate::{Header, PacketType, Rcode, Reply};
// use bit_vec::{self, BitVec};

/// gets a u16 based on the bit start point
// pub fn get_u16_from_packets(packets: &[u8], start_point: usize) -> u16 {
//     let mut result_bytes: [u8; 2] = [0, 0];
//     let end_point: usize = start_point + 2;
//     result_bytes.copy_from_slice(&packets[start_point..end_point]);
//     u16::from_be_bytes(result_bytes)
// }
// gets a u8 based on the bit start point
// pub fn get_u8_from_bits(bits: &BitVec, start_point: usize, bit_count: usize) -> u8 {
//     let mut output: u8 = 0;
//     for index in start_point..start_point + bit_count {
//         output = output << <u8>::from(bits.get(index).unwrap());
//         println!("{} {}", bits.get(index).unwrap(), output);
//     }
//     output
// }

#[test]
fn test_convert_u16_to_u8s_be() {
    let testval: u16 = 1;
    assert_eq!(convert_u16_to_u8s_be(testval), [0, 1]);
    let testval: u16 = 256;
    assert_eq!(convert_u16_to_u8s_be(testval), [1, 0]);
    let testval: u16 = 65535;
    assert_eq!(convert_u16_to_u8s_be(testval), [255, 255]);
}

pub fn convert_u16_to_u8s_be(integer: u16) -> [u8; 2] {
    [(integer >> 8) as u8, integer as u8]
}

#[test]
fn test_convert_u32_to_u8s_be() {
    let testval: u32 = 1;
    assert_eq!(convert_u32_to_u8s_be(testval), [0, 0, 0, 1]);
    let testval: u32 = 256;
    assert_eq!(convert_u32_to_u8s_be(testval), [0, 0, 1, 0]);
    let testval: u32 = 2_u32.pow(31);
    assert_eq!(convert_u32_to_u8s_be(testval), [128, 0, 0, 0]);
    // most significant bit test
    assert_eq!(0b10101010, 170);
}
// we might find a use for this yet
pub fn convert_u32_to_u8s_be(integer: u32) -> [u8; 4] {
    [
        (integer >> 24) as u8,
        (integer >> 16) as u8,
        (integer >> 8) as u8,
        integer as u8,
    ]
}

// #[test]
// fn test_convert_i32_to_u8s_be() {
//     let mut testval: i32 = 1;
//     assert_eq!(convert_i32_to_u8s_be(testval), [0, 0, 0, 1]);
//     testval = 256;
//     assert_eq!(convert_i32_to_u8s_be(testval), [0, 0, 1, 0]);
//     testval = 2_i32.pow(30);
//     debug!("testval 2_i32 ^ 30 = {}", testval);
//     assert_eq!(convert_i32_to_u8s_be(testval), [64, 0, 0, 0]);
//     testval = -32768;
//     assert_eq!(convert_i32_to_u8s_be(testval), [255, 255, 128, 0]);

//     // random test of most significant bit things
//     assert_eq!(0b10101010, 170);
// }
// pub fn convert_i32_to_u8s_be(integer: i32) -> [u8; 4] {
//     [
//         (integer >> 24) as u8,
//         (integer >> 16) as u8,
//         (integer >> 8) as u8,
//         integer as u8,
//     ]
// }

pub fn vec_find(item: u8, search: &[u8]) -> Option<usize> {
    for (index, curr_byte) in search.iter().enumerate() {
        if &(item as u8) == curr_byte {
            return Some(index);
        }
    }
    None
}

/// turn the NAME field into the bytes for a response
///
/// so example.com turns into
///
/// (7)example(3)com(0)
///
/// compress_target is the index of the octet in the response to point the response at
/// which should typically be the qname in the question bytes
pub fn name_as_bytes(name: Vec<u8>, compress_target: Option<u16>) -> Vec<u8> {
    if let Some(target) = compress_target {
        // we need the first two bits to be 1, to mark it as compressed
        // 4.1.4 RFC1035 - https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.4
        let result: u16 = 0b1100000000000000 | target as u16;
        return convert_u16_to_u8s_be(result).to_vec();
    }

    let mut result: Vec<u8> = vec![];
    // if somehow it's a weird bare domain then we don't have to do much it
    if !name.contains(&46) {
        result.push(name.len() as u8);
        result.extend(name);
    } else {
        let mut next_dot: usize = match vec_find(46, &name) {
            Some(value) => value,
            None => return result,
        };
        let mut name_bytes: Vec<u8> = name.to_vec();
        let mut keep_looping = true;
        let mut current_position: usize = 0;
        // add the first segment length
        result.push(next_dot as u8);

        while keep_looping {
            if next_dot == current_position {
                name_bytes = name_bytes.to_vec()[current_position + 1..].into();
                next_dot = match vec_find(46, &name_bytes) {
                    Some(value) => value,
                    None => name_bytes.len(),
                };
                current_position = 0;
                // this should be the
                // debug!(". - {:?} ({:?})", next_dot, name_bytes);
                result.push(next_dot as u8);
            } else {
                // we are processing bytes
                result.push(name_bytes[current_position]);
                // debug!("{:?} {:?}", current_position, name_bytes.as_bytes()[current_position]);
                current_position += 1;
            }
            if current_position == name_bytes.len() {
                keep_looping = false;
            }
        }
    }
    // make sure we have a trailing null
    // if !name.ends_with('.') {
    result.push(0);
    // }

    result
}

/// Want a generic empty reply with an ID and an RCODE? Here's your function.
pub fn reply_builder(id: u16, rcode: Rcode) -> Result<Reply, String> {
    let header = Header {
        id,
        qr: PacketType::Answer,
        rcode,
        ..Default::default()
    };
    Ok(Reply {
        header,
        question: None,
        answers: vec![],
        authorities: vec![],
        additional: vec![],
    })
}
