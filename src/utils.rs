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

/// turn the NAME field into the bytes for a response
///
/// so example.com turns into
///
/// (7)example(3)com(0)
///
/// compress_target is the index of the octet in the response to point the response at
/// which should typically be the qname in the question bytes
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
    // if let Some(comp_ref) = compress_reference {
    //     let comp_ref_name = comp_ref.name.as_bytes().to_vec();
    //     if name == name_as_bytes(comp_ref_name.clone(), None, None) {
    //         eprintln!("we can just yeet back the thing!");
    //         if let Some(target) = compress_target {
    //             let result: u16 = 0b1100000000000000 | target as u16;
    //             return result.to_be_bytes().to_vec();
    //             // we need the first two bits to be 1, to mark it as compressed
    //             // 4.1.4 RFC1035 - https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.4

    //         }
    //     } else {
    //         eprintln!("{name:?} != {:?}", name_as_bytes(comp_ref_name, None, None))
    //     }
    // }

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
    result.push(0);

    if let (None, None) = (compress_reference, compress_target) {
        eprintln!("no targets, returning!");
        return result;
    };

    eprintln!("We did the conversion bit and got {:?}", result);
    if let Some(ct) = compress_reference {
        eprintln!("you gave me {ct:?} as a compression reference");
        if &result == ct {
            eprintln!("The thing we're converting is the same as the compression reference!");
            // return a pointer to the target_byte (probably the name in the header)
            if let Some(target) = compress_target {
                let result: u16 = 0b1100000000000000 | target as u16;
                return result.to_be_bytes().to_vec();
            } else {
                panic!("you didn't give us a target, dude!")
            }
        }
        if result.ends_with(ct) {
            eprintln!("the name ends with the target! woo!");
            // Ok, we've gotten this far. We need to slice off the "front" of the string and return that.
            result.truncate(result.len() - ct.len());
            eprintln!("The result is trimmed and now {:?}", from_utf8(&result));
            // then we need to return the pointer to the tail
            if let Some(target) = compress_target {
                let pointer_bytes: u16 = 0b1100000000000000 | target as u16;
                result.extend(pointer_bytes.to_be_bytes());
            } else {
                panic!("no compression target and we totally could have compressed this.")
            }

            eprintln!("The result is trimmed and now {:?}", &result);
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

/// pass this a string and get the reversed version in a Vec<u8>
pub fn name_reversed(name: &str) -> Vec<u8> {
    name.as_bytes().to_vec()
}

#[cfg(test)]
mod test_name_reversed {
    use super::name_reversed;
    #[test]
    fn testit() {
        let foo = "hello world";
        let bar = name_reversed(foo);
        assert_eq!(bar.len(), 11);
        assert_eq!(bar[0], 'd' as u8);
        assert_eq!(bar[10], 'h' as u8);
    }
}
