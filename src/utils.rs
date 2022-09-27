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
#[cfg(test)]
pub fn convert_u32_to_u8s_be(integer: u32) -> [u8; 4] {
    [
        (integer >> 24) as u8,
        (integer >> 16) as u8,
        (integer >> 8) as u8,
        integer as u8,
    ]
}

#[test]
fn test_convert_i32_to_u8s_be() {
    let mut testval: i32 = 1;
    assert_eq!(convert_i32_to_u8s_be(testval), [0, 0, 0, 1]);
    testval = 256;
    assert_eq!(convert_i32_to_u8s_be(testval), [0, 0, 1, 0]);
    testval = 2_i32.pow(30);
    eprintln!("testval 2_i32 ^ 30 = {}", testval);
    assert_eq!(convert_i32_to_u8s_be(testval), [64, 0, 0, 0]);
    testval = -32768;
    assert_eq!(convert_i32_to_u8s_be(testval), [255, 255, 128, 0]);

    // random test of most significant bit things
    assert_eq!(0b10101010, 170);
}
pub fn convert_i32_to_u8s_be(integer: i32) -> [u8; 4] {
    [
        (integer >> 24) as u8,
        (integer >> 16) as u8,
        (integer >> 8) as u8,
        integer as u8,
    ]
}

/// turn the NAME field into the bytes for a response
///
/// so example.com turns into
///
/// (7)example(3)com(0)
pub fn name_as_bytes(name: String) -> Vec<u8> {
    let mut result: Vec<u8> = vec![];
    // eprintln!("name_as_bytes: {:?}", name);
    // if somehow it's a weird bare domain then YOLO it
    if !name.contains('.') {
        result.push(name.len() as u8);
        for b in name.as_bytes() {
            result.push(b.to_owned())
        }
    } else {
        let mut name_bytes = name.clone();
        let mut keep_looping = true;
        let mut next_dot: usize = name.find('.').unwrap();
        let mut current_position: usize = 0;
        // add the first segment length
        result.push(next_dot as u8);

        while keep_looping {
            if next_dot == current_position {
                name_bytes = name_bytes[current_position + 1..].into();
                next_dot = match name_bytes.find('.') {
                    Some(value) => value as usize,
                    None => name_bytes.len(),
                };
                current_position = 0;
                // this should be the
                // eprintln!(". - {:?} ({:?})", next_dot, name_bytes);
                result.push(next_dot as u8);
            } else {
                // we are processing bytes
                result.push(name_bytes.as_bytes()[current_position]);
                // eprintln!("{:?} {:?}", current_position, name_bytes.as_bytes()[current_position]);
                current_position += 1;
            }
            if current_position == name_bytes.len() {
                keep_looping = false;
            }
        }
    }
    // trailing null
    result.push(0);

    result
}
