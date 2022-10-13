use crate::{Header, PacketType, Rcode, Reply};
use log::debug;
use std::str::from_utf8;

pub fn vec_find(item: u8, search: &[u8]) -> Option<usize> {
    for (index, curr_byte) in search.iter().enumerate() {
        if &(item as u8) == curr_byte {
            return Some(index);
        }
    }
    None
}

/// does the conversion from "example.com" to "7example3com" BUT DOES NOT DO THE TRAILING NULL BECAUSE REASONS
fn seven_dot_three_conversion(name: &[u8]) -> Vec<u8> {
    let mut result: Vec<u8> = vec![];

    // TODO: reimplement this with slices and stuff
    let mut next_dot: usize = match vec_find(46, name) {
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
            if next_dot != 0 {
                result.push(next_dot as u8);
                // eprintln!("pushing next_dot {}", next_dot as u8);
            }
        } else {
            // we are processing bytes
            // eprintln!("pushing {}", name_bytes[current_position]);
            result.push(name_bytes[current_position]);
            // debug!("{:?} {:?}", current_position, name_bytes.as_bytes()[current_position]);
            current_position += 1;
        }
        if current_position == name_bytes.len() {
            keep_looping = false;
        }
    }
    result
}

/*
turn the NAME field into the bytes for a response

so example.com turns into

(7)example(3)com(0)

compress_target is the index of the octet in the response to point the response at
which should typically be the qname in the question bytes

compress_reference is the vec of bytes of the compression target, ie this is the kind of terrible thing you should do
name_as_bytes(
    "lol.example.com".as_bytes().to_vec(),
    Some(12),
    Some("example.com".as_bytes().to_vec())
)
*/
pub fn name_as_bytes(
    name: Vec<u8>,
    compress_target: Option<u16>,
    compress_reference: Option<&Vec<u8>>,
) -> Vec<u8> {
    eprintln!("################################");
    match from_utf8(&name) {
        Ok(nstr) => eprintln!("name_as_bytes name={nstr:?} compress_target={compress_target:?} compress_reference={compress_reference:?}"),
        Err(_) =>  eprintln!("name_as_bytes name={name:?} compress_target={compress_target:?} compress_reference={compress_reference:?}"),
    };

    // if we're given a compression target and no reference just compress it and return
    if let (Some(target), None) = (compress_target, compress_reference) {
        debug!("we got a compress target ({target}) but no reference we're just going to compress");
        // we need the first two bits to be 1, to mark it as compressed
        // 4.1.4 RFC1035 - https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.4
        let result: Vec<u8> = (0b1100000000000000 | target as u16).to_be_bytes().into();
        debug!("result of name_as_bytes {result:?}");
        return result;
    };

    let mut result: Vec<u8> = vec![];
    // if somehow it's a weird bare domain then we don't have to do much it
    if !name.contains(&46) {
        result.push(name.len() as u8);
        result.extend(&name);
        result.push(0); // null pad the name
        return result;
    }

    result = seven_dot_three_conversion(&name);

    if compress_target.is_none() {
        eprintln!("no compression target, adding the trailing null and returning!");
        result.push(0);
        return result;
    };

    if let Some(ct) = compress_reference {
        eprintln!("you gave me {ct:?} as a compression reference");

        if &name == ct {
            eprintln!("The thing we're converting is the same as the compression reference!");
            // return a pointer to the target_byte (probably the name in the header)
            if let Some(target) = compress_target {
                let result: u16 = 0b1100000000000000 | target as u16;
                return result.to_be_bytes().to_vec();
            } else {
                panic!("you didn't give us a target, dude!")
            }
        }
        if name.ends_with(ct) {
            eprintln!("the name ends with the target! woo!");
            // Ok, we've gotten this far. We need to slice off the "front" of the string and return that.
            result = name.clone();
            result.truncate(name.len() - ct.len());
            eprintln!("The result is trimmed and now {:?}", from_utf8(&result));

            // do the 7.3 conversion
            result = seven_dot_three_conversion(&result);
            eprintln!("7.3converted: {:?}", from_utf8(&result));

            // then we need to return the pointer to the tail
            if let Some(target) = compress_target {
                let pointer_bytes: u16 = 0b1100000000000000 | target as u16;
                result.extend(pointer_bytes.to_be_bytes());
            } else {
                panic!("no compression target and we totally could have compressed this.")
            }

            eprintln!("The result is trimmed and now {:?}", result);
        } else {
            eprintln!("no trimming :(")
        }
    }
    eprintln!("Final result {result:?}");
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

/// Want a generic empty reply with an ID and an RCODE? Here's your function.
pub fn reply_nxdomain(id: u16) -> Result<Reply, String> {
    let header = Header {
        id,
        qr: PacketType::Answer,
        rcode: Rcode::NameError,
        ancount: 0,
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

/// dumps the bytes out as if you were using some kind of fancy packet-dumper
pub fn hexdump(bytes: Vec<u8>) {
    for byte in bytes.chunks(2) {
        let byte0_alpha = match byte[0].is_ascii_alphanumeric() {
            true => from_utf8(byte[0..1].into()).unwrap(),
            false => " ",
        };
        match byte.len() {
            2 => {
                let byte1_alpha = match byte[1].is_ascii_alphanumeric() {
                    true => from_utf8(byte[1..2].into()).unwrap(),
                    false => " ",
                };

                debug!(
                    "{:02x} {:02x} {:#010b} {:#010b} {:3} {:3} {byte0_alpha} {byte1_alpha}",
                    byte[0], byte[1], byte[0], byte[1], byte[0], byte[1],
                );
            }
            _ => {
                debug!(
                    "{:02x}    {:#010b}    {:3} {byte0_alpha}",
                    byte[0], byte[0], byte[0],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::name_as_bytes;
    use std::str::from_utf8;

    #[test]
    pub fn test_name_bytes_simple_compress() {
        let expected_result: Vec<u8> = vec![192, 12];

        let test_result = name_as_bytes("example.com".as_bytes().to_vec(), Some(12), None);
        assert_eq!(expected_result, test_result);
    }
    #[test]
    pub fn test_name_bytes_no_compress() {
        let expected_result: Vec<u8> =
            vec![7, 101, 120, 97, 109, 112, 108, 101, 3, 99, 111, 109, 0];

        let test_result = name_as_bytes("example.com".as_bytes().to_vec(), None, None);
        assert_eq!(expected_result, test_result);
    }

    #[test]
    pub fn test_name_bytes_with_compression() {
        // 7example3com0
        // let example_com: Vec<u8> = vec![7, 101, 120, 97, 109, 112, 108, 101, 3, 99, 111, 109, 0];
        let example_com = "example.com".as_bytes().to_vec();
        let test_input = "lol.example.com".as_bytes().to_vec();

        // let test_input: Vec<u8> = vec![3, 108, 111, 108, 7, 101, 120, 97, 109, 112, 108, 101, 3, 99, 111, 109, 0];

        let expected_result: Vec<u8> = vec![3, 108, 111, 108, 192, 12];

        eprintln!("{:?}", from_utf8(&example_com));
        eprintln!("{:?}", from_utf8(&test_input));

        let result = name_as_bytes(test_input, Some(12), Some(&example_com));

        assert_eq!(result, expected_result);
    }
}
