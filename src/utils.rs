use crate::{Header, PacketType, Rcode, Reply};
use log::debug;

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
        return result.to_be_bytes().to_vec();
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
        match byte.len() {
            2 => {
                debug!(
                    "{:02x} {:02x} {:#010b} {:#010b} {:3} {:3}",
                    byte[0], byte[1], byte[0], byte[1], byte[0], byte[1],
                );
            }
            _ => {
                debug!("{:02x}    {:#010b}    {:3}", byte[0], byte[0], byte[0],);
            }
        }
    }
}

/// pass this a string and get the reversed version in a Vec<u8>
pub fn name_reversed(name: &str) -> Vec<u8> {
    let mut response = name.as_bytes().to_vec();
    response.reverse();
    response
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
