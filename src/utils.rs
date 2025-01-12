use crate::datastore::Command;
use crate::enums::AgentState;
use crate::error::GoatNsError;
use crate::HEADER_BYTES;
use std::str::from_utf8;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, trace};

pub fn vec_find(item: u8, search: &[u8]) -> Option<usize> {
    for (index, curr_byte) in search.iter().enumerate() {
        if &(item) == curr_byte {
            return Some(index);
        }
    }
    None
}

/// does the conversion from "example.com" to "7example3com" BUT DOES NOT DO THE TRAILING NULL BECAUSE REASONS
fn seven_dot_three_conversion(name: &[u8]) -> Vec<u8> {
    trace!("7.3 conversion for {name:?} {:?}", from_utf8(name));
    let mut result: Vec<u8> = vec![];

    // TODO: reimplement this with slices and stuff
    let mut next_dot: usize = match vec_find(46, name) {
        Some(value) => value,
        None => {
            // if there's no dots, then just push a length on the front and include the data. then bail
            result.push(name.len() as u8);
            result.extend(name);
            return result;
        }
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
                trace!("pushing next_dot {}", next_dot as u8);
            }
        } else {
            // we are processing bytes
            trace!("pushing {}", name_bytes[current_position]);
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

/// If you have a `name` and a `target` and want to see if you can find a chunk of the `target` that the `name` ends with, this is your function!
pub fn find_tail_match(name: &[u8], target: &Vec<u8>) -> usize {
    // #[cfg(debug)]
    // {
    let name_str = format!("{name:?}");
    let target_str = format!("{target:?}");
    let longest = match name_str.len() > target_str.len() {
        true => name_str.len(),
        false => target_str.len(),
    };
    trace!("find_tail_match(\n  name={name_str:>longest$}, \ntarget={target_str:>longest$}\n)",);
    // }
    let mut tail_index: usize = 0;
    for (i, _) in target.iter().enumerate() {
        trace!("Tail index={i}");
        let tail = &target[i..];
        if name.ends_with(tail) {
            trace!("Found a tail at index {i}");
            tail_index = i;
            break;
        } else {
            trace!("Didn't match: name / target \n{name:?}\n{:?}", &target[i..]);
        }
    }
    tail_index
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
    name: &[u8],
    compress_target: Option<u16>,
    compress_reference: Option<&Vec<u8>>,
) -> Result<Vec<u8>, GoatNsError> {
    trace!("################################");
    match from_utf8(name) {
        Ok(nstr) => trace!("name_as_bytes name={nstr:?} compress_target={compress_target:?} compress_reference={compress_reference:?}"),
        Err(_) =>  trace!("failed to utf-8 name name_as_bytes name={name:?} compress_target={compress_target:?} compress_reference={compress_reference:?}"),
    };

    // if we're given a compression target and no reference just compress it and return
    if let (Some(target), None) = (compress_target, compress_reference) {
        trace!("we got a compress target ({target}) but no reference we're just going to compress");
        // we need the first two bits to be 1, to mark it as compressed
        // 4.1.4 RFC1035 - https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.4
        let result: Vec<u8> = (0b1100000000000000 | target).to_be_bytes().into();
        trace!("result of name_as_bytes {result:?}");
        return Ok(result);
    };

    let mut result: Vec<u8> = vec![];
    // if somehow it's a weird bare domain then we don't have to do much it
    if !name.contains(&46) {
        result.push(name.len() as u8);
        result.extend(name);
        result.push(0); // null pad the name
        return Ok(result);
    }
    result = seven_dot_three_conversion(name);

    if compress_target.is_none() {
        trace!("no compression target, adding the trailing null and returning!");
        result.push(0);
        return Ok(result);
    };

    if let Some(ct) = compress_reference {
        trace!("you gave me {ct:?} as a compression reference");

        if name == ct {
            trace!("The thing we're converting is the same as the compression reference!");
            // return a pointer to the target_byte (probably the name in the header)
            if let Some(target) = compress_target {
                let result: u16 = 0b1100000000000000 | target;
                return Ok(result.to_be_bytes().to_vec());
            } else {
                return Err(GoatNsError::InvalidName);
            }
        }
        if name.ends_with(ct) {
            trace!("the name ends with the target! woo!");
            // Ok, we've gotten this far. We need to slice off the "front" of the string and return that.
            result.clone_from(&name.to_vec());
            result.truncate(name.len() - ct.len());
            trace!("The result is trimmed and now {:?}", from_utf8(&result));

            // do the 7.3 conversion
            result = seven_dot_three_conversion(&result);
            trace!("7.3converted: {:?}", from_utf8(&result));

            // then we need to return the pointer to the tail
            if let Some(target) = compress_target {
                let pointer_bytes: u16 = 0b1100000000000000 | target;
                result.extend(pointer_bytes.to_be_bytes());
            } else {
                return Err(GoatNsError::BytePackingError(
                    "No compression target and we totally could have compressed this.".to_string(),
                ));
            }

            trace!("The result is trimmed and now {:?}", result);
        } else {
            // dropped into tail-finding mode where we're looking for a sub-string of the parent to target a compression pointer
            trace!("trying to find a sub-part of {ct:?} in {name:?}");

            let tail_index = find_tail_match(name, ct);
            trace!("tail_index: {tail_index}");
            // if we get to here and the tail_index is 0 then we haven't set it - because we'd have caught the whole thing in the ends_with matcher earlier.
            if tail_index != 0 {
                trace!("Found a tail-match: {tail_index}");
                // slice the tail off the name
                let mut name_copy = name.to_vec();
                // the amount of the tail that matched to the name, ie abc, bc = 2, aab, bbb = 1
                let matched_length = ct.len() - tail_index;
                name_copy.truncate(name.len() - matched_length);
                trace!("sliced name down to {name_copy:?}");
                // put the pointer on there
                result = seven_dot_three_conversion(&name_copy);
                trace!("converted result to {result:?}");
                let pointer: u16 = 0b1100000000000000 | (HEADER_BYTES + tail_index + 1) as u16;
                result.extend(pointer.to_be_bytes());
            }
        }
    }
    trace!("Final result {result:?}");
    Ok(result)
}

// lazy_static!{
//     static ref GOATNS_VERSION: DNSCharString = DNSCharString::from(format!("GoatNS {}", env!("CARGO_PKG_VERSION")).as_str());
// }

// lazy_static!{
//     static ref VERSION_RESPONSE: Vec<InternalResourceRecord> = vec![InternalResourceRecord::TXT {
//         class: RecordClass::Chaos,
//         ttl: 1,
//         txtdata: GOATNS_VERSION.to_owned(),
//     }];
// }

// pub fn reply_version(id: &u16, question: &Option<crate::Question>) -> Result<Reply, String> {
//     let mut reply = reply_builder(id.to_owned(), Rcode::NoError)?;
//     reply.question = question.to_owned();
//     reply.answers = VERSION_RESPONSE.clone();
//     reply.header.ancount = 1;
//     debug!("Version: {reply:?}");
//     debug!("Goatns version: {:?}", GOATNS_VERSION.to_owned());
//     Ok(reply)
// }

/// dumps the bytes out as if you were using some kind of fancy packet-dumper
pub fn hexdump(bytes: &[u8]) -> Result<(), GoatNsError> {
    for byte in bytes.chunks(2) {
        let byte0_alpha = match byte[0].is_ascii_alphanumeric() {
            true => from_utf8(byte[0..1].into())?,
            false => " ",
        };
        match byte.len() {
            2 => {
                let byte1_alpha = match byte[1].is_ascii_alphanumeric() {
                    true => from_utf8(byte[1..2].into())?,
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
    Ok(())
}

/// turn a degrees/minutes/seconds format into unsigned 32-bit integer matching the format
/// required for a DNS LOC record
///
/// when positive = true, you're North or West
pub fn dms_to_u32(deg: u8, min: u8, sec: f32, positive: bool) -> u32 {
    let secsfrac = sec % 1f32;

    let dms_multiplied: u32 = (((((deg as u32 * 60) + min as u32) * 60) + sec as u32) * 1000)
        + (secsfrac * 1000.0) as u32;

    match positive {
        true => 2u32.pow(31) + dms_multiplied,
        false => 2u32.pow(31) - dms_multiplied,
    }
}

/// converts size/precision X * 10**Y(cm) to 0xXY
/// This code is ported from the C code in RFC1876 (Appendix A, precsize_aton)
#[allow(dead_code)]
pub fn loc_size_to_u8(input: f32) -> u8 {
    let mut mantissa: u8;

    let cmval = input * 100.0;

    let mut exponent = 0;
    for i in 0..10 {
        if (cmval as f64) < (10u64.pow(i + 1) as f64) {
            exponent = i;
            // eprintln!("CMVAL: {cmval} Exponent #{i} {}", (poweroften[i+1] as u64));
            break;
        }
    }
    // eprintln!("{:?}", ((cmval as f64) / (poweroften[exponent] as f64) ).ceil());
    mantissa = ((cmval as f64) / (10u64.pow(exponent) as f64)).ceil() as u8;
    if mantissa > 9u8 {
        mantissa = 9u8;
    }
    // eprintln!("mantissa: {mantissa}, exponent: {exponent}");
    // turn it into the magic ugly numbers
    let retval: u8 = (mantissa << 4) | (exponent as u8);
    retval
}

/// Get all the widgets for agent signalling
pub fn start_channels() -> (
    broadcast::Sender<AgentState>,
    mpsc::Sender<Command>,
    mpsc::Receiver<Command>,
) {
    let (agent_tx, _) = broadcast::channel(32);
    let datastore_sender: mpsc::Sender<Command>;
    let datastore_receiver: mpsc::Receiver<Command>;
    (datastore_sender, datastore_receiver) = mpsc::channel(crate::MAX_IN_FLIGHT);
    (agent_tx, datastore_sender, datastore_receiver)
}

/// Compares the TLD to the list of valid TLDs - usually from `allowed_tlds` in [crate::config::ConfigFile]
///```
/// use goatns::utils::check_valid_tld;
///
/// let valid_tlds = vec![];
/// let zone_name = "hello.example.goat";
/// assert_eq!(check_valid_tld(&zone_name, &valid_tlds), true);
///
/// let valid_tlds = vec!["goat".to_string()];
/// let zone_name = "hello.example.goat";
/// assert_eq!(check_valid_tld(&zone_name, &valid_tlds), true);
///
/// let valid_tlds = vec!["cheese".to_string()];
/// let zone_name = "hello.example.goat";
/// assert_eq!(check_valid_tld(&zone_name, &valid_tlds), false);
/// ```
pub fn check_valid_tld(zone_name: &str, allowed_tlds: &[String]) -> bool {
    if allowed_tlds.is_empty() {
        return true;
    }
    for tld in allowed_tlds.iter() {
        if zone_name.ends_with(&format!(".{tld}")) {
            return true;
        }
    }
    false
}
