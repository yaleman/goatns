//! Zone file parsing, based on [RFC1035 Master Files](https://datatracker.ietf.org/doc/html/rfc1035#autoid-48).

/*
valid lines

<blank>[<comment>]
$ORIGIN <domain-name> [<comment>]
$INCLUDE <file-name> [<domain-name>] [<comment>]
$TTL <u32> [<comment>]

... the other bits
*/

use log::{debug, error, info};
use regex::{Captures, Regex};

use crate::enums::{RecordClass, RecordType};

#[derive(Debug)]
#[allow(dead_code)]
enum LineType {
    Soa {
        class: String,
        host: String,
        rrname: Option<String>,
        serial: Option<u32>,
        refresh: Option<u32>,
        retry: Option<u32>,
        expire: Option<u32>,
        minimum: Option<u32>,
        ttl: Option<u32>,
    },

    PartialRecord {
        host: Option<String>,
        class: Option<String>,
        rtype: Option<String>,
        preference: Option<u16>,
        rdata: Option<String>,
        ttl: Option<u32>,
    },
    Unknown(String),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum LexerState {
    Collecting(String),
    PartialInclude {
        filename: String,
        domain_name: Option<String>,
        comment: Option<String>,
    },
    // PartialComment(String),
    Idle,
    PartialSoaRecord {
        host: String,
        class: String,
        rrname: Option<String>,
        serial: Option<u32>,
        refresh: Option<u32>,
        retry: Option<u32>,
        expire: Option<u32>,
        minimum: Option<u32>,
        ttl: Option<u32>,
    },
    PartialRecord {
        host: String,
        class: Option<String>,
        rtype: Option<String>,
        preference: Option<u16>,
        rdata: Option<String>,
        ttl: Option<u32>,
    },
    Unknown(String),
    OriginSOA {
        class: String,
        host: Option<String>,
        rrname: Option<String>,
        serial: Option<u32>,
        refresh: Option<u32>,
        retry: Option<u32>,
        expire: Option<u32>,
        minimum: Option<u32>,
        ttl: Option<u32>,
    },
}

#[derive(Debug)]
struct ZoneInclude {
    #[allow(dead_code)]
    filename: String,
    #[allow(dead_code)]
    domain_name: Option<String>,
    #[allow(dead_code)]
    comment: Option<String>,
}

#[derive(Default, Debug)]
struct ParsedZoneFile {
    /// which line the comment was on, and what it was
    #[allow(dead_code)]
    comments: Vec<(usize, String)>,
    #[allow(dead_code)]
    soarecord: Option<LineType>,
    #[allow(dead_code)]
    records: Vec<LineType>,
    origin: Option<String>,
    #[allow(dead_code)]
    includes: Vec<ZoneInclude>,
    lines: usize,
    ttl: Option<u32>,
    // for multi=line records so we can get the last one
    #[allow(dead_code)]
    last_used_host: Option<String>,
}

fn get_read_len_from_caps(caps: &Captures) -> usize {
    caps.iter()
        .filter_map(|c| c.map(|c| c.end()))
        .max()
        .unwrap()
}

#[allow(dead_code)]
fn parse_file(contents: &str) -> Result<ParsedZoneFile, String> {
    // let mut lex = ZoneFileToken::lexer(contents);
    let max_loops = contents.len() / 5;

    let mut contents = contents.to_string();
    let mut state = LexerState::Idle;
    let mut zone: ParsedZoneFile = ParsedZoneFile::default();

    // stupid regexes for parsing stupid things
    // let class_type_value_str: &str = r"(?<class>\w+)\s+(?<rtype>\w+)\s+(?<value>[a-zA-Z0-9-\.]+)";
    // let class_type_priority_value_str: &str =
    //     r"(?<class>[A-Z]+)\s+(?<rtype>[A-Z]+)\s+(?<priority>[\d]+)\s+(?P<value>[a-zA-Z0-9\.\-]+)";
    // let class_type_priority_value = Regex::new(class_type_priority_value_str).unwrap();

    // let host_class_type_value =
    //     Regex::new(&([r"^\s*(?<host>[\w\d]+)\s+", class_type_value_str].concat())).unwrap();
    // let class_type_value = Regex::new(class_type_value_str).unwrap();
    let mut loops = 0;

    let regex_ttl = Regex::new(r"^\$TTL\s+(?P<ttl>\d+)").unwrap();
    let regex_origin = Regex::new(r"^\$ORIGIN\s+(?P<domain>\S+)").unwrap();
    let regex_include =
        Regex::new(r"^\$INCLUDE\s+(?P<filename>\S+)\s*(?P<domain>\S+)(?P<comment>;[^\n]*)?")
            .unwrap();
    let regex_comment: Regex = Regex::new(r"^;(?P<comment>[^\n]*)").unwrap();

    debug!("original length: {}", contents.len());

    loop {
        let cstr = contents.replace('\t', " ");
        contents = cstr.trim_start().to_string();
        let mut read_len = 1;
        debug!("state: {:?}", state);

        let r_host = r"(?<host>[a-zA-Z0-9\.\_-]+)";
        let r_rname = r"(?P<rname>[a-zA-Z0-9\.\_-]+\.[a-zA-Z0-9\.\_-]+)";
        let r_class = r#"(?P<class>[A-Z]+)"#;
        let r_type = r#"(?P<rtype>[A-Z]+)"#;
        let r_ttl = r"(?P<ttl>\d+)";
        let r_data = r#"(?P<rdata>("[^"]+"|\S+))"#;

        let soa_matcher = Regex::new(&format!(
            r#"^(?P<domain>[\@a-zA-Z0-9\.\_-]+)\s+((?P<ttl>\d*)\s+|){}\s+SOA\s+{}\s+{}"#,
            r_class, r_host, r_rname
        ))
        .unwrap();

        let host_class_type_rdata = Regex::new(&format!(
            r#"^{}\s+{}\s+{}\s+{}"#,
            r_host, r_class, r_type, r_data,
        ))
        .unwrap();

        // class type rdata
        // host_ttl_class_type_rdata // cloudflare
        let host_ttl_class_type_rdata = Regex::new(&format!(
            r#"^{}\s+{}\s+{}\s+{}\s+{}"#,
            r_host, r_ttl, r_class, r_type, r_data
        ))
        .unwrap();

        if contents.starts_with('(') {
            // if let LexerState::PartialSoaRecord { .. } = state {
            //     // keep building the soa record
            //     debug!("in the soa record")
            // } else {
            //     panic!("Open brackets on a non-soa line?")
            // }
            // // todo!("match start of brackets");
        } else if contents.starts_with(')') {
            match state.clone() {
                LexerState::PartialSoaRecord {
                    host,
                    class,
                    rrname,
                    serial,
                    refresh,
                    retry,
                    expire,
                    minimum,
                    ttl,
                } => {
                    // keep building the soa record
                    debug!("in the soa record");
                    zone.soarecord = Some(LineType::Soa {
                        host,
                        class,
                        rrname,
                        serial,
                        refresh,
                        retry,
                        expire,
                        minimum,
                        ttl,
                    });
                    zone.ttl = ttl;
                }
                LexerState::PartialRecord {
                    host,
                    class,
                    rtype,
                    preference,
                    rdata,
                    ttl,
                } => zone.records.push(LineType::PartialRecord {
                    host: Some(host),
                    class,
                    rtype,
                    preference,
                    rdata,
                    ttl,
                }),
                _ => {
                    debug!("end of bracket!");
                }
            }
            state = LexerState::Idle;
        } else if let Some(caps) = regex_origin.captures(&contents) {
            info!("ORIGIN LINE: {:?}", caps);
            // let caps = regex_ttl.captures(&contents).unwrap();
            zone.origin = caps.name("domain").map(|d| d.as_str().to_string());

            // if let Some(comment) = caps.name("comment") {
            //     zone.comments
            //         .push((zone.lines, comment.as_str().to_string()));
            // }
            read_len = get_read_len_from_caps(&caps);
        } else if let Some(caps) = regex_ttl.captures(&contents) {
            info!("TTL LINE: {:?}", caps);
            // let caps = regex_ttl.captures(&contents).unwrap();
            zone.ttl = caps
                .name("ttl")
                .map(|ttl| ttl.as_str().parse::<u32>().expect("Failed to parse TTL!"));
            // if let Some(comment) = caps.name("comment") {
            //     zone.comments
            //         .push((zone.lines, comment.as_str().to_string()));
            //     read_len = caps.get(caps.len() - 2).unwrap().end();
            // } else {
            //     read_len = caps.get(caps.len() - 2).unwrap().end();
            // }
            read_len = get_read_len_from_caps(&caps);
        } else if let Some(caps) = regex_include.captures(&contents) {
            info!("INCLUDE LINE: {:?}", caps);

            zone.includes.push(ZoneInclude {
                filename: caps
                    .name("filename")
                    .expect("Couldn't get filename from $INCLUDE")
                    .as_str()
                    .to_string(),
                domain_name: caps.name("domain").map(|d| d.as_str().to_string()),
                comment: caps.name("comment").map(|c| c.as_str().to_string()),
            });
            read_len = get_read_len_from_caps(&caps);
        } else if let Some(caps) = regex_comment.captures(&contents) {
            debug!("comment!");
            zone.comments.push((
                zone.lines,
                caps.name("comment").unwrap().as_str().to_string(),
            ));
            read_len = get_read_len_from_caps(&caps);
        } else if contents.starts_with('@') {
            match zone.origin {
                Some(_) => zone.last_used_host = zone.origin.clone(),
                None => return Err("@ entry without setting origin first!".to_string()),
            }
            // zone.last_used_host = zone.origin.clone();
        } else if let Some(caps) = soa_matcher.captures(&contents) {
            info!("SOA Matched: {:#?}", caps);
            let host = match caps.name("domain") {
                Some(d) => {
                    let res = d.as_str().to_string();
                    zone.origin = Some(res.clone());
                    zone.last_used_host = Some(res.clone());
                    res
                }
                None => zone.last_used_host.clone().unwrap(),
            };

            state = LexerState::PartialSoaRecord {
                host,
                class: caps.name("class").unwrap().as_str().to_string(),
                rrname: caps.name("rrname").map(|c| c.as_str().to_string()),
                serial: None,
                refresh: None,
                retry: None,
                expire: None,
                minimum: None,
                ttl: None,
            };
            read_len = get_read_len_from_caps(&caps);
            // todo!()
        } else {
            let matchers = vec![host_class_type_rdata, host_ttl_class_type_rdata];

            let mut caps: Option<Captures<'_>> = None;
            for matcher in matchers {
                if let Some(captures) = matcher.captures(&contents) {
                    debug!("{:?} matched ", matcher);
                    caps = Some(captures);
                    state = LexerState::Idle;

                    break;
                }
            }
            if let Some(caps) = caps {
                info!("caps: {:#?}", caps);
                let host = match caps.name("host") {
                    Some(d) => {
                        if zone.origin.is_none() {
                            zone.origin = Some(d.as_str().to_string());
                        }
                        d.as_str().to_string()
                    }
                    None => zone.last_used_host.clone().unwrap(),
                };

                let ttl = match caps.name("ttl") {
                    Some(ttl_cap) => Some(ttl_cap.as_str().parse::<u32>().unwrap()),
                    None => zone.ttl,
                };

                let new_record = LineType::PartialRecord {
                    host: Some(host),
                    class: caps.name("class").map(|c| c.as_str().to_string()),
                    rtype: caps.name("rtype").map(|v| v.as_str().to_string()),
                    preference: caps
                        .name("preference")
                        .map(|v| v.as_str().parse::<u16>().unwrap()),
                    rdata: caps.name("rdata").map(|v| v.as_str().to_string()),
                    ttl,
                };

                read_len = get_read_len_from_caps(&caps);
                debug!("Adding new record: {:?}", new_record);
                zone.records.push(new_record);
                state = LexerState::Idle;
                // todo!("we got sum gud caps");
            } else {
                // get the next word
                let next_term = match contents.split(' ').next() {
                    None => break,
                    Some(val) => val.trim(),
                };
                debug!("next term: {} state: {:?}", next_term, state);

                let partial_state = if RecordClass::try_from(next_term).is_ok() {
                    let host = match zone.last_used_host.clone() {
                        None => match zone.origin.clone() {
                            None => return Err("No last used host or origin field?".to_string()),
                            Some(origin) => origin,
                        },
                        Some(zone_luh) => zone_luh,
                    };

                    LexerState::PartialRecord {
                        host,
                        class: Some(next_term.to_string()),
                        rtype: None,
                        preference: None,
                        rdata: None,
                        ttl: None,
                    }
                } else if RecordType::try_from(next_term).is_ok() {
                    LexerState::PartialRecord {
                        host: zone.last_used_host.clone().unwrap_or(
                            zone.origin
                                .clone()
                                .expect("Didn't have last_used_host or origin!"),
                        ),
                        class: None,
                        rtype: Some(next_term.to_string()),
                        preference: None,
                        rdata: None,
                        ttl: None,
                    }
                } else {
                    LexerState::Unknown(next_term.to_string())
                };

                let mut curr_state = state.clone();
                read_len = next_term.len() + 1;
                state = match &mut curr_state {
                    LexerState::Collecting(val) => {
                        read_len= val.len();
                        LexerState::Collecting(format!("{} {}", val, next_term))
                    }
                    LexerState::PartialInclude {
                        // filename,
                        // domain_name,
                        // comment,
                        ..
                    } => todo!(),
                    LexerState::Idle => {
                        partial_state
                    },
                    LexerState::PartialSoaRecord {
                        host,
                        class,
                        rrname,
                        serial,
                        refresh,
                        retry,
                        expire,
                        minimum,
                        ttl,
                    } => {
                        if serial.is_none() {
                            *serial = Some(next_term.parse::<u32>().unwrap());
                        } else if refresh.is_none() {
                            *refresh = Some(next_term.parse::<u32>().unwrap());
                        } else if retry.is_none() {
                            *retry = Some(next_term.parse::<u32>().unwrap());
                        } else if expire.is_none() {
                            *expire = Some(next_term.parse::<u32>().unwrap());
                        } else if minimum.is_none() {
                            dbg!(&next_term);
                            *minimum = Some(next_term.parse::<u32>().unwrap());
                        } else if ttl.is_none() {
                            *ttl = Some(next_term.parse::<u32>().unwrap());
                        } else {
                            panic!("too many terms in soa record! {}", next_term);
                        }
                        LexerState::PartialSoaRecord {
                            host: host.to_string(), class: class.to_string(),
                            rrname: rrname.as_ref().map(|r| r.to_owned()),
                            serial: serial.map(|r| r.to_owned()),
                            refresh: refresh.map(|r| r.to_owned()),
                            retry:retry.map(|r| r.to_owned()),
                            expire:expire.map(|r| r.to_owned()),
                            minimum:minimum.map(|r| r.to_owned()),
                            ttl:ttl.map(|r| r.to_owned())
                        }
                    },
                    LexerState::PartialRecord {
                        host: _,
                        class,
                        rtype,
                        preference,
                        rdata,
                        ttl

                    } => {
                        if let LexerState::PartialRecord { host,class:  p_class,rtype:  p_rtype,preference:  p_preference,rdata:  p_rdata, ttl:  p_ttl } = partial_state {
                            let new_class = match p_class {
                                Some(val) => Some(val),
                                None => class.to_owned()
                            };
                            let new_rtype = match p_rtype {
                                Some(val) => Some(val),
                                None => rtype.to_owned()
                            };
                            let new_preference = match p_preference {
                                Some(val) => Some(val),
                                None => preference.to_owned()
                            };
                            let new_rdata = match p_rdata {
                                Some(val) => Some(val),
                                None => rdata.to_owned()
                            };
                            let new_ttl = match p_ttl {
                                Some(val) => Some(val),
                                None => ttl.to_owned()
                            };

                            LexerState::PartialRecord{
                                host,
                                class: new_class,
                                rtype: new_rtype,
                                preference: new_preference,
                                rdata: new_rdata,
                                ttl: new_ttl,
                            }

                        } else {
                            panic!()
                        }
                    }
                    LexerState::Unknown(_) => todo!(),
                    LexerState::OriginSOA {
                        // class,
                        // host,
                        // rrname,
                        // serial,
                        // refresh,
                        // retry,
                        // expire,
                        // minimum,
                        // ttl,
                        ..
                    } => todo!(),
                }
            }
            /*
            <domain-name><rr> [<comment>]
            <blank><rr> [<comment>]
            <rr> contents take one of the following forms:
            - [<TTL>] [<class>] <type> <RDATA>
            - [<class>] [<TTL>] <type> <RDATA>
            */
        }

        // if read_len > 1 {
        //     debug!("Splitting at {read_len}");
        // }

        // debug!("old length: {}", contents.len());
        if read_len == contents.len() {
            debug!("done!");
            break;
        }
        if read_len < contents.len() {
            let (chunk, buf) = contents.split_at(read_len);
            if chunk.contains('\n') {
                zone.lines += 1;
            }
            contents = buf.to_string();
        } else {
            loops = max_loops;
        }

        debug!("current state: {:?}", state);
        debug!("current line: {:?}", contents.split('\n').next());

        if contents.is_empty() {
            break;
        }
        if loops > max_loops {
            error!(
                "Looped too many times, bailing! - content length = {}",
                contents.len()
            );
            return Err("oh no!".to_string());
        } else {
            loops += 1;
        }
    }
    debug!("{:#?}", zone);

    // debug!("origin: {:?}", zone.origin);
    // debug!("ttl: {:?}", zone.ttl);
    // debug!("includes: {:?}", zone.includes);
    // debug!("records: {:?}", zone.records);
    // info!("Comments: ");
    // debug!(
    //     "{}",
    //     zone.comments
    //         .iter()
    //         .map(|c| format!("{c:?}"))
    //         .collect::<Vec<String>>()
    //         .join("\n")
    // );
    // info!("Records:");
    // zone.records.iter().for_each(|r| debug!("{r:?}"));
    Ok(zone)
}

#[test]
fn test_parse_example_file() {
    if flexi_logger::Logger::try_with_str("debug")
        .unwrap()
        .start()
        .is_err()
    {
        println!("Oh no, no logging for you!")
    };

    // example from https://access.redhat.com/documentation/en-us/red_hat_enterprise_linux/4/html/reference_guide/s2-bind-zone-examples
    let example_file = r#"$ORIGIN example.com.
    $TTL 86400
    @	IN	SOA	dns1.example.com.	hostmaster.example.com. (
                2001062501 ; serial
                21600      ; refresh after 6 hours
                3600       ; retry after 1 hour
                604800     ; expire after 1 week
                86400 )    ; minimum TTL of 1 day


        IN	NS	dns1.example.com.
        IN	NS	dns2.example.com.


        IN	MX	10	mail.example.com.
        IN	MX	20	mail2.example.com.


    dns1	IN	A	10.0.1.1
    dns2	IN	A	10.0.1.2


    server1	IN	A	10.0.1.5
    server2	IN	A	10.0.1.6


    ftp	IN	A	10.0.1.3
        IN	A	10.0.1.4

    mail	IN	CNAME	server1
    mail2	IN	CNAME	server2


    www	IN	CNAME	server1"#;
    // let lex = ZoneFileToken::lexer(&example_file);
    let res: ParsedZoneFile = parse_file(&example_file).unwrap();

    if let LineType::Soa {
        class,
        host: _,
        rrname: _,
        serial: _,
        refresh: _,
        retry: _,
        expire: _,
        minimum,
        ttl: _,
    } = res.soarecord.unwrap()
    {
        assert_eq!(class, "IN");
        assert_eq!(minimum, Some(86400));
    } else {
        panic!("didn't get an SOA record!");
    };
}

#[test]
fn test_parse_yaleman_file() {
    if flexi_logger::Logger::try_with_str("debug")
        .unwrap()
        .start()
        .is_err()
    {
        println!("well, no logging for you!");
    };

    let example_file = r#";;
    ;; Domain:     yaleman.org.
    ;; Exported:   2023-11-21 13:59:08
    ;;
    ;; This file is intended for use for informational and archival
    ;; purposes ONLY and MUST be edited before use on a production
    ;; DNS server.  In particular, you must:
    ;;   -- update the SOA record with the correct authoritative name server
    ;;   -- update the SOA record with the contact e-mail address information
    ;;   -- update the NS record(s) with the authoritative name servers for this domain.
    ;;
    ;; For further information, please consult the BIND documentation
    ;; located on the following website:
    ;;
    ;; http://www.isc.org/
    ;;
    ;; And RFC 1035:
    ;;
    ;; http://www.ietf.org/rfc/rfc1035.txt
    ;;
    ;; Please note that we do NOT offer technical support for any use
    ;; of this zone data, the BIND name server, or any other third-party
    ;; DNS software.
    ;;
    ;; Use at your own risk.
    ;; SOA Record
    yaleman.org	3600	IN	SOA	hera.ns.cloudflare.com. dns.cloudflare.com. 2045224174 10000 2400 604800 3600

    ;; NS Records
    yaleman.org.	86400	IN	NS	hera.ns.cloudflare.com.
    yaleman.org.	86400	IN	NS	jobs.ns.cloudflare.com.

    ;; A Records
    apache.housenet.yaleman.org.	1	IN	A	10.0.0.21
    apc7921.housenet.yaleman.org.	1	IN	A	10.0.0.35
    auth.housenet.yaleman.org.	1	IN	A	10.0.0.11
    azerbaijan.yaleman.org.	1	IN	A	167.179.181.230
    bogbrother.housenet.yaleman.org.	1	IN	A	10.0.0.75
    brother.azerbaijan.yaleman.org.	1	IN	A	10.1.0.145
    cisco.housenet.yaleman.org.	1	IN	A	10.0.0.8
    cupboard.azerbaijan.yaleman.org.	1	IN	A	10.1.0.21
    dshield.housenet.yaleman.org.	1	IN	A	10.0.0.36
    gateway.housenet.yaleman.org.	1	IN	A	10.0.0.1
    hallway.azerbaijan.yaleman.org.	1	IN	A	10.1.0.20
    hass.housenet.yaleman.org.	1	IN	A	10.0.0.24
    helios1.housenet.yaleman.org.	1	IN	A	10.0.0.31
    hikvision1.housenet.yaleman.org.	1	IN	A	10.0.0.50
    hive.housenet.yaleman.org.	1	IN	A	10.0.0.12
    househorn.azerbaijan.yaleman.org.	1	IN	A	10.1.0.12
    housenet.yaleman.org.	1	IN	A	180.150.105.135
    idiotbox.azerbaijan.yaleman.org.	1	IN	A	10.1.0.6
    k3s1.housenet.yaleman.org.	1	IN	A	10.0.0.94
    k3s2.housenet.yaleman.org.	1	IN	A	10.0.0.95
    k8s.housenet.yaleman.org.	1	IN	A	10.0.0.91
    k8s.housenet.yaleman.org.	1	IN	A	10.0.0.92
    k8s.housenet.yaleman.org.	1	IN	A	10.0.0.94
    kanidm2.housenet.yaleman.org.	1	IN	A	10.0.0.15
    kanidm3.housenet.yaleman.org.	1	IN	A	10.0.0.23
    m1.housenet.yaleman.org.	1	IN	A	127.0.0.1
    minio-v4.housenet.yaleman.org.	1	IN	A	10.0.0.31
    mqtt.housenet.yaleman.org.	1	IN	A	10.0.0.24
    nagios.housenet.yaleman.org.	1	IN	A	10.0.0.19
    nextcloud.housenet.yaleman.org.	1	IN	A	10.0.0.32
    pfsense.housenet.yaleman.org.	1	IN	A	10.0.0.1
    plex.housenet.yaleman.org.	1	IN	A	10.0.0.40
    proxy.housenet.yaleman.org.	1	IN	A	10.0.0.27
    pve1-ipv4.housenet.yaleman.org.	1	IN	A	10.0.0.10
    pve2-ipv4.housenet.yaleman.org.	1	IN	A	10.0.0.14
    pxe.housenet.yaleman.org.	1	IN	A	10.0.0.26
    raspberrypi917a.housenet.yaleman.org.	1	IN	A	10.0.0.92
    raspberrypia1a1.housenet.yaleman.org.	1	IN	A	10.0.0.91
    raspberrypicc72.housenet.yaleman.org.	1	IN	A	10.0.0.93
    raspberrypief91.housenet.yaleman.org.	1	IN	A	10.0.0.90
    raspi3.housenet.yaleman.org.	1	IN	A	10.0.5.30
    raspiclear.housenet.yaleman.org.	1	IN	A	10.0.5.113
    raspi-z2w.housenet.yaleman.org.	1	IN	A	10.0.0.97
    reolink.housenet.yaleman.org.	1	IN	A	10.0.40.10
    rymera.yaleman.org.	1	IN	A	122.148.240.51
    ryzenshine.housenet.yaleman.org.	1	IN	A	10.0.0.33
    sore.housenet.yaleman.org.	1	IN	A	10.0.0.120
    splunk-index2.housenet.yaleman.org.	1	IN	A	10.0.0.18
    splunk-index.housenet.yaleman.org.	1	IN	A	10.0.0.18
    squid.housenet.yaleman.org.	1	IN	A	10.0.0.27
    switchy.housenet.yaleman.org.	1	IN	A	10.0.0.9
    syslog.housenet.yaleman.org.	1	IN	A	10.0.0.18
    unifi.housenet.yaleman.org.	1	IN	A	10.0.0.85
    wireguard.housenet.yaleman.org.	1	IN	A	10.0.0.16
    wordpress.yaleman.org.	1	IN	A	104.236.137.122
    zubat.housenet.yaleman.org.	1	IN	A	10.0.0.77

    ;; AAAA Records
    ansible.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:c41b:c104:a007:fabc
    hass.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:78d0:36ff:fea8:9276
    helios1.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:6662:66ff:fed0:8fa
    hive.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:88e:79ff:feea:9b1c
    idiotbox.azerbaijan.yaleman.org.	1	IN	AAAA	2403:580a:15:0:3ed9:2bff:fe02:910d
    k3s1.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:5c51:3bff:fe04:c77d
    kanidm2.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:24ad:bfff:fe87:d39d
    kanidm3.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:542c:5ff:fe5e:ab16
    loungeap.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:b6fb:e4ff:fe49:bf0f
    microserver.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:2a92:4aff:fe30:5afb
    monitoring.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:f006:7ff:feff:8a10
    nonebook.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:426c:8fff:fe3f:ef17
    officeap.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:b6fb:e4ff:fe49:c8a7
    picluster.housenet.yaleman.org.	1	IN	AAAA	2403:5800:9200:2600:dea6:32ff:fe0d:ef91
    picluster.housenet.yaleman.org.	1	IN	AAAA	2403:5800:9200:2600:dea6:32ff:febf:917a
    plex.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:9c19:13ff:fee7:8242
    proxy.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:fc8c:d1ff:fe6d:1149
    pve1.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:2efd:a1ff:fe59:a6b1
    pve2.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:9e5c:8eff:fec2:cea2
    raspberrypi917a.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:dea6:32ff:febf:917a
    raspberrypia1a1.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:dea6:32ff:febb:a1a1
    raspberrypicc72.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:da3a:ddff:fe27:cc72
    raspberrypief91.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:dea6:32ff:fe0d:ef91
    raspi3.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:f:ba27:ebff:fef8:523
    raspiclear.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:14:ba27:ebff:fe9b:a5a
    ryzenshine.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:3e7c:3fff:fef0:f1f
    scaregistry.housenet.yaleman.org.	1	IN	AAAA	2001:44b8:2123:4c00:ed2d:f019:2834:ee80
    splunk-index2.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:e8a3:2eff:fe5a:b33f
    splunkv6.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:e8a3:2eff:fe5a:b33f
    squid.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:fc8c:d1ff:fe6d:1149
    ubuntufw.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:843a:3dff:fec5:9fe0
    unifi.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:8462:49ff:fe0b:ba99
    wireguard.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:eeba:9039:c07a:47b1
    wordpress.yaleman.org.	1	IN	AAAA	2604:a880:1:20::1d13:6001
    zubat.housenet.yaleman.org.	1	IN	AAAA	2403:580a:2d:0:109f:26aa:3147:6ad7

    ;; CAA Records
    yaleman.org.	1	IN	CAA	0 issue "amazon.com"
    yaleman.org.	1	IN	CAA	0 issue "letsencrypt.org"

    ;; CNAME Records
    _19b2a95314c2d2d55e4f405f20b2ab3b.wiki.yaleman.org.	300	IN	CNAME	_dc7d34a936251a34137a329bc2b40bd9.bsgbmzkfwj.acm-validations.aws.
    api.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    backupserver.housenet.yaleman.org.	300	IN	CNAME	helios1.housenet.yaleman.org.
    cert-hello-world.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    console-minio.housenet.yaleman.org.	300	IN	CNAME	helios1.housenet.yaleman.org.
    fileserver.housenet.yaleman.org.	300	IN	CNAME	helios1.housenet.yaleman.org.
    fm1._domainkey.yaleman.org.	300	IN	CNAME	fm1.yaleman.org.dkim.fmhosted.com.
    fm2._domainkey.yaleman.org.	300	IN	CNAME	fm2.yaleman.org.dkim.fmhosted.com.
    fm3._domainkey.yaleman.org.	300	IN	CNAME	fm3.yaleman.org.dkim.fmhosted.com.
    freshrss.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    freshrss.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    github-linter.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    goatns.housenet.yaleman.org.	60	IN	CNAME	k8s.housenet.yaleman.org.
    goatns.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    grafana.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    hass.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    headlamp.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    headlamp.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    hec.public.housenet.yaleman.org.	300	IN	CNAME	housenet.yaleman.org.
    homeassistant.housenet.yaleman.org.	300	IN	CNAME	hass.housenet.yaleman.org.
    homebridge.housenet.yaleman.org.	300	IN	CNAME	hass.housenet.yaleman.org.
    homepage.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    homepage.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    internal.kanidm.yaleman.org.	300	IN	CNAME	kanidm2.housenet.yaleman.org.
    kanidm.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    left.test-kanidm.yaleman.org.	300	IN	CNAME	kanidm3.housenet.yaleman.org.
    memes.yaleman.org.	1	IN	CNAME	c160046f-f79d-4f63-985b-834846a56bd2.cfargotunnel.com.
    metube.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    minio.housenet.yaleman.org.	300	IN	CNAME	helios1.housenet.yaleman.org.
    minio.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    mqtt.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    nagios.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    ntp.housenet.yaleman.org.	300	IN	CNAME	pfsense.housenet.yaleman.org.
    otlp.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    pamsplainer.yaleman.org.	300	IN	CNAME	yaleman.github.io.
    portdb.yaleman.org.	300	IN	CNAME	yaleman.github.io.
    qbittorrent-microservice.housenet.yaleman.org.	300	IN	CNAME	raspberrypief91.housenet.yaleman.org.
    right.test-kanidm.yaleman.org.	300	IN	CNAME	kanidm3.housenet.yaleman.org.
    saml.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    sca-canon.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    sca.yaleman.org.	1	IN	CNAME	web1.ricetek.net.
    splunk-deployment.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    splunk-easm-worker.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    splunk-nonebook.housenet.yaleman.org.	300	IN	CNAME	nonebook.housenet.yaleman.org.
    splunk.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    sprintf.yaleman.org.	1	IN	CNAME	c160046f-f79d-4f63-985b-834846a56bd2.cfargotunnel.com.
    supersecretbucketfullofhackingtools_hi_team.housenet.yaleman.org.	300	IN	CNAME	helios1.housenet.yaleman.org.
    test-kanidm.yaleman.org.	300	IN	CNAME	kanidm3.housenet.yaleman.org.
    test-oauth2.yaleman.org.	1	IN	CNAME	5852ef56-4ccb-4206-ac4a-0b6e2b43071e.cfargotunnel.com.
    traefik.housenet.yaleman.org.	300	IN	CNAME	k8s.housenet.yaleman.org.
    unifi.yaleman.org.	1	IN	CNAME	20736771-64a6-4a81-b57e-206fbfeb7805.cfargotunnel.com.
    wiki.yaleman.org.	300	IN	CNAME	d2cw5o2h6ess7v.cloudfront.net.
    wireguard.public.housenet.yaleman.org.	300	IN	CNAME	housenet.yaleman.org.
    wpad.housenet.yaleman.org.	300	IN	CNAME	static.housenet.yaleman.org.
    www.yaleman.org.	1	IN	CNAME	yaleman.org.
    yaleman.org.	300	IN	CNAME	yaleman.github.io.

    ;; LOC Records
    pizza.yaleman.org.	69	IN	LOC	01 02 3.000 N 01 02 3.000 E 10m 10m 10m 10m
    yaleman.org.	1	IN	LOC	01 02 3.000 N 01 02 3.000 E 10m 10m 10m 10m

    ;; MX Records
    housenet.yaleman.org.	1	IN	MX	10 housenet.yaleman.org.
    yaleman.org.	1	IN	MX	20 in2-smtp.messagingengine.com.
    yaleman.org.	1	IN	MX	10 in1-smtp.messagingengine.com.

    ;; SRV Records
    _kanidm._tcp.housenet.yaleman.org.	1	IN	SRV	0 200 443 kanidm.yaleman.org.
    _kanidm._tcp.yaleman.org.	1	IN	SRV	0 100 443 kanidm.yaleman.org.
    _ldap._tcp.housenet.yaleman.org.	1	IN	SRV	0 100 636 internal.kanidm.yaleman.org.
    _mqtt._tcp.housenet.yaleman.org.	1	IN	SRV	0 100 1883 mqtt.housenet.yaleman.org.
    _ntp._tcp.housenet.yaleman.org.	1	IN	SRV	0 100 123 ntp.housenet.yaleman.org.
    _ntp._udp.housenet.yaleman.org.	1	IN	SRV	0 100 123 ntp.housenet.yaleman.org.
    _proxy._tcp.housenet.yaleman.org.	1	IN	SRV	0 100 3128 proxy.housenet.yaleman.org.

    ;; TXT Records
    _dmarc.housenet.yaleman.org.	1	IN	TXT	"v=DMARC1; p=quarantine; rua=mailto:dmarc@mvpmonitor.com; fo=1; pct=100"
    _dmarc.yaleman.org.	1	IN	TXT	"v=DMARC1; p=none; rua=mailto:yaleman+dmarc@ricetek.net"
    housenet.yaleman.org.	1	IN	TXT	"v=spf1 include:spf.messagingengine.com mx a ip4:106.187.45.100/32 ip6:2400:8900::f03c:91ff:fedf:a845/64 ~all"
    _kerberos.housenet.yaleman.org.	1	IN	TXT	"HOUSENET.YALEMAN.ORG"
    krs._domainkey.registry.sca.yaleman.org.	1	IN	TXT	"k=rsa;p=MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDEIkTRA2vtfL+b9qXWo/hhnuJfEoLS7ofbK2vJYoghveNX3rH2vTMON+Ycy4Z5kzp2DTc+6H20e5ZUl5bbnyMMle7cD7sjmQx0JgHYUwAn2rpo3ArDsGwPSHLdxUzE9ax52j/YRUVm5AYj2SavqQqddBMWUKY6jFY4HuLDwXOs3wIDAQAB"
    krs._domainkey.registry.sca.yaleman.org.	300	IN	TXT	"k=rsa"
    _network.housenet.yaleman.org.	1	IN	TXT	"2403:580a:2d::"
    registry.sca.yaleman.org.	1	IN	TXT	"v=spf1 include:mailgun.org ~all"
    yaleman.org.	1	IN	TXT	"keybase-site-verification=cq_4sfjtnnuUaRHhga3vZZro7tZpwkVrcJUXzAmJSA0"
    yaleman.org.	1	IN	TXT	"google-site-verification=yVbO2SyKI2ssRYE2AV_uRMx8gXlNt8WutFHN2RnT7rM"
    yaleman.org.	1	IN	TXT	"v=spf1 include:spf.messagingengine.com mx a ip4:106.187.45.100/32 ip6:2400:8900::f03c:91ff:fedf:a845/64 ~all"
    "#;
    let res = parse_file(example_file);

    dbg!(&res);

    assert!(res.is_ok());
}

#[test]
fn test_busted_files() {
    if flexi_logger::Logger::try_with_str("debug")
        .unwrap()
        .start()
        .is_err()
    {
        println!("well, no logging for you!");
    };

    let example_file = r#"@ hello world;;   "#;
    assert!(parse_file(example_file).is_err());

    let example_file = r#") hello world;;   "#;
    assert!(parse_file(example_file).is_err());

    // this should trigger the "oh no too many fields in the SOA record" bit
    let example_file = r#"example.com 3600	IN	SOA	hera.ns.cloudflare.com. dns.cloudflare.com. (
        2045224174
        10000
        2400
        604800
        3600
        hello world
     )  "#;
    let res = parse_file(example_file);
    dbg!(&res);
    assert!(res.is_err());
}
