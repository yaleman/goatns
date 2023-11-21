//! Zone file parsing, based on [RFC1035 Master Files](https://datatracker.ietf.org/doc/html/rfc1035#autoid-48).

/*
valid lines

<blank>[<comment>]
$ORIGIN <domain-name> [<comment>]
$INCLUDE <file-name> [<domain-name>] [<comment>]
<domain-name><rr> [<comment>]
<blank><rr> [<comment>]
*/

/*

<rr> contents take one of the following forms:
- [<TTL>] [<class>] <type> <RDATA>
- [<class>] [<TTL>] <type> <RDATA>
*/

// use crate::enums::{RecordClass, RecordType};
use log::{debug, error, info};
use logos::{Logos, Source};
use regex::Regex;

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\n]+")] // Ignore this regex pattern between tokens
enum ZoneFileToken {
    // // Or regular expressions.
    #[regex(r#";[^\n\f]+[\n\f]+"#, |lex| lex.slice().trim().to_owned())]
    Comment(String),

    #[regex(r#"\([\s\n]+"#)]
    OpenParen,

    #[regex(r#"\)[\s\n]+"#)]
    CloseParen,

    #[regex(r#"\$INCLUDE [a-zA-Z0-9_\.]+"#, |lex| lex.slice().split(' ').nth(1).unwrap().to_owned())]
    Include(String),

    #[regex(r#"\$ORIGIN\s+[a-zA-Z0-9-_\.]+"#, |lex| lex.slice().split(' ').nth(1).unwrap().to_owned())]
    Origin(String),

    /// RFC 2308 (section 4)
    #[regex(r#"\$TTL\s*\d+"#, |lex| lex.slice().split(' ').nth(1).unwrap().parse::<u32>().unwrap())]
    Ttl(u32),

    #[regex(r#"[@a-z0-9A-Z-_\.]+\s+IN\s+SOA\s+[a-z0-9A-Z-_\.]+\s+[a-z0-9A-Z-_\.]+"#, |lex| lex.slice().to_owned(), priority=10)]
    SoaRecord(String),

    #[regex(r#"\w+[a-zA-Z0-9_\.]+"# , |lex| lex.slice().to_owned())]
    Text(String),

    #[regex(r#"[A-Z]+\s*[A-Z]+\s+[\d]+\s+[a-zA-Z0-9\.\-]+"#, |lex| lex.slice().to_owned())]
    ClassTypePriorityValue(String),

    #[regex(r#"[A-Z]+\s*[A-Z]+\s+[a-zA-Z0-9\.\-]+"#, |lex| lex.slice().to_owned())]
    ClassTypeValue(String),

    #[regex(r#"[\w\d\.]+\s+[A-Z]+\s+[A-Z]+\s+[a-zA-Z0-9-\.]+"#, |lex| lex.slice().to_owned())]
    HostClassTypeValue(String),

    #[token("\n")]
    NewLine,

    #[regex(r#"\d+\s*"#, |lex| lex.slice().trim().to_owned())]
    Number(String),

    #[token("@")]
    OriginMarker,
}
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
    },

    PartialRecord {
        host: String,
        class: String,
        rtype: String,
        preference: Option<u16>,
        value: String,
    },
    // ClassTypeValue {
    //     class: String,
    //     rtype: String,
    //     value: String,
    // },
    // Origin {
    //     domain_name: String,
    //     comment: Option<String>,
    // },
    // Include {
    //     filename: String,
    //     domain_name: Option<String>,
    //     comment: Option<String>,
    // },
    // Comment(String),
    // DnRr {
    //     domain_name: String,
    //     rr: RrEntry,
    //     comment: Option<String>,
    // },
    // Unknown(String),
    // InvalidLine(String),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum LexerState {
    // PartialOrigin {
    //     domain_name: Option<String>,
    //     comment: Option<String>,
    // },
    // PartialOriginWithDomain {
    //     domain_name: String,
    //     comment: Option<String>,
    // },
    PartialInclude {
        filename: String,
        domain_name: Option<String>,
        comment: Option<String>,
    },
    PartialComment(String),
    // PartialRrEntry {
    //     domain_name: Option<String>,
    //     entry_type: RrEntry,
    //     ttl: Option<u32>,
    //     class: Option<RecordClass>,
    //     rtype: Option<RecordType>,
    //     rdata: Option<String>,
    //     comment: Option<String>,
    // },
    Idle,
    Unknown(String),
    InvalidLine(String),
    OriginMarker,
    OriginSOA {
        class: String,
        host: Option<String>,
        rrname: Option<String>,
        serial: Option<u32>,
        refresh: Option<u32>,
        retry: Option<u32>,
        expire: Option<u32>,
        minimum: Option<u32>,
    },
    OpenParen(Box<LexerState>),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum RrEntry {
    TtlFirst,
    ClassFirst,
}

#[derive(Default, Debug)]
struct ParsedZoneFile {
    /// which line the comment was on, and what it was
    comments: Vec<(usize, String)>,
    #[allow(dead_code)]
    soarecord: Option<LineType>,
    records: Vec<LineType>,
    origin: Option<String>,
    includes: Vec<String>,
    lines: usize,
    ttl: Option<u32>,
    // for multi=line records so we can get the last one
    last_used_host: Option<String>,
}

#[allow(dead_code)]
fn parse_file(contents: &str) -> Result<ParsedZoneFile, String> {
    let mut lex = ZoneFileToken::lexer(contents);
    let mut state = LexerState::Idle;

    let mut zone: ParsedZoneFile = ParsedZoneFile::default();

    // stupid regexes for parsing stupid things
    let class_type_value_str: &str = r"(?<class>\w+)\s+(?<rtype>\w+)\s+(?<value>[a-zA-Z0-9-\.]+)";
    let class_type_priority_value_str: &str =
        r"(?<class>[A-Z]+)\s+(?<rtype>[A-Z]+)\s+(?<priority>[\d]+)\s+(?P<value>[a-zA-Z0-9\.\-]+)";
    let class_type_priority_value = Regex::new(class_type_priority_value_str).unwrap();

    let host_class_type_value =
        Regex::new(&([r"^\s*(?<host>[\w\d]+)\s+", class_type_value_str].concat())).unwrap();
    let class_type_value = Regex::new(class_type_value_str).unwrap();

    // let mut loops = 0;
    while let Some(token) = lex.next() {
        if token.is_err() {
            let slice = contents.slice(lex.span());
            error!("*** Got an error at {:?} {:?}", lex.span(), slice);
            break;
        }
        let current_token = token.unwrap();
        debug!("current token: {:?}", &current_token);
        state = match current_token {
            ZoneFileToken::SoaRecord(value) => {
                debug!("soa record: {:?}", &value);
                let soa_getter = Regex::new(
                    r"(?<domain>\S+)\s+(?<class>[A-Z]+)\s+SOA\s+(?<host>\S+)\s+(?<rrname>\S+)",
                )
                .expect("soa_getter regex compilation failed!");

                let res = soa_getter.captures(&value).unwrap();

                let domain = res.name("domain").unwrap().as_str().to_string();
                let class = res.name("class").unwrap().as_str().to_string();
                let host = res.name("host").unwrap().as_str().to_string();
                let rrname = res
                    .name("rrname")
                    .unwrap()
                    .as_str()
                    .replace('\\', "")
                    .to_string();

                if domain != "@" {
                    zone.origin = Some(domain);
                }

                LexerState::OriginSOA {
                    class,
                    host: Some(host),
                    rrname: Some(rrname),
                    serial: None,
                    refresh: None,
                    retry: None,
                    expire: None,
                    minimum: None,
                }
            }
            ZoneFileToken::Number(value) => text_state(&mut zone, value, state)?,
            ZoneFileToken::ClassTypePriorityValue(value) => {
                let res = class_type_priority_value.captures(&value).unwrap();
                debug!("ctpv: {:?}", &res);
                let class = res.name("class").unwrap().as_str().to_string();
                let rtype = res.name("rtype").unwrap().as_str().to_string();
                let preference = res.name("priority").unwrap().as_str().to_string();
                let preference: u16 = preference
                    .parse()
                    .expect("Failed to parse preference field!");
                let value = res.name("value").unwrap().as_str().to_string();
                zone.records.push(LineType::PartialRecord {
                    host: zone.last_used_host.clone().unwrap(),
                    class,
                    rtype,
                    preference: Some(preference),
                    value,
                });
                LexerState::Idle
            }
            ZoneFileToken::ClassTypeValue(value) => {
                let res = class_type_value.captures(&value).unwrap();
                debug!("ctv: {:?}", &res);
                let class = res.name("class").unwrap().as_str().to_string();
                let rtype = res.name("rtype").unwrap().as_str().to_string();
                let value = res.name("value").unwrap().as_str().to_string();

                if &rtype == "SOA" {
                    LexerState::OriginSOA {
                        class,
                        host: Some(value),
                        rrname: None,
                        serial: None,
                        refresh: None,
                        retry: None,
                        expire: None,
                        minimum: None,
                    }
                } else {
                    zone.records.push(LineType::PartialRecord {
                        host: zone.last_used_host.clone().unwrap(),
                        class,
                        rtype,
                        preference: None,
                        value,
                    });
                    LexerState::Idle
                }
            }
            ZoneFileToken::HostClassTypeValue(value) => {
                let res = host_class_type_value.captures(&value).unwrap();
                debug!("ctv: {:?}", &res);
                let host = res.name("host").unwrap().as_str().to_string();

                zone.last_used_host = Some(host.clone());
                let class = res.name("class").unwrap().as_str().to_string();
                let rtype = res.name("rtype").unwrap().as_str().to_string();
                let value = res.name("value").unwrap().as_str().to_string();

                if &rtype == "SOA" {
                    todo!("dctv soa");
                    // LexerState::OriginSOA {
                    //     class,
                    //     host: Some(value),
                    //     rrname: None,
                    //     serial: None,
                    //     refresh: None,
                    //     retry: None,
                    //     expire: None,
                    //     minimum: None,
                    // }
                } else {
                    zone.records.push(LineType::PartialRecord {
                        host,
                        class,
                        rtype,
                        preference: None,
                        value,
                    });
                    LexerState::Idle
                }
            }

            ZoneFileToken::OriginMarker => {
                // TODO: reset the last_used_host here?
                zone.last_used_host = zone.origin.clone();
                LexerState::OriginMarker
            }
            ZoneFileToken::Ttl(ttl) => {
                zone.ttl = Some(ttl);
                LexerState::Idle
            }
            ZoneFileToken::NewLine =>                 newline_state(&mut zone, state)?

                , //state = match state {},
            ZoneFileToken::Comment(comment) => {
                debug!("Adding comment: {:?}", comment);
                zone.comments.push((zone.lines, comment));
                // return to the previous state
                state
            }
            ZoneFileToken::OpenParen => LexerState::OpenParen(Box::new(state)),
            ZoneFileToken::CloseParen => closedparen_state(&mut zone, state),
            ZoneFileToken::Include(filename) => LexerState::PartialInclude {
                filename,
                domain_name: None,
                comment: None,
            },

            ZoneFileToken::Origin(origin) => {
                zone.origin = Some(origin.clone());
                zone.last_used_host = Some(origin);
                debug!("setting origin/luh: {:?}", zone.origin);
                LexerState::Idle
            }
            ZoneFileToken::Text(value) => text_state(&mut zone, value, state).map_err(|err| {
                let slice = contents.slice(lex.span());
                format!("Failed at {:?}: '{:?}' {}", lex.span(), slice, err)
            })?,
        };
        debug!("State: {:?}", state);
    }
    debug!("final state: {:?}", state);

    debug!("origin: {:?}", zone.origin);
    debug!("ttl: {:?}", zone.ttl);
    debug!("includes: {:?}", zone.includes);
    // debug!("records: {:?}", zone.records);
    info!("Comments: ");
    debug!(
        "{}",
        zone.comments
            .iter()
            .map(|c| format!("{c:?}"))
            .collect::<Vec<String>>()
            .join("\n")
    );
    info!("Records:");
    zone.records.iter().for_each(|r| debug!("{r:?}"));
    Ok(zone)
}

#[test]
fn test_parse_file() {
    flexi_logger::Logger::try_with_str("debug")
        .unwrap()
        .start()
        .unwrap();

    let example_file = "$ORIGIN example.com.
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


    ftp	IN	A	10.0.1.3 ; TODO: remove this one day
        IN	A	10.0.1.4

    mail	IN	CNAME	server1
    mail2	IN	CNAME	server2


    www	IN	CNAME	server1"
        .to_string();
    // let lex = ZoneFileToken::lexer(&example_file);
    let res = parse_file(&example_file)
        .map_err(|err| {
            error!("Oh no: {err}");
        })
        .unwrap();

    if let LineType::Soa {
        class,
        host: _,
        rrname: _,
        serial: _,
        refresh: _,
        retry: _,
        expire: _,
        minimum,
    } = res.soarecord.unwrap()
    {
        assert_eq!(class, "IN");
        assert_eq!(minimum, Some(86400));
    } else {
        panic!("didn't get an SOA record!");
    };
}

/// we got a newline, what are we going to do with it?
fn newline_state(zone: &mut ParsedZoneFile, state: LexerState) -> Result<LexerState, String> {
    zone.lines += 1;

    match state {
        LexerState::OpenParen(current_state) => match *current_state {
            LexerState::OriginSOA { .. } => Ok(*current_state),
            _ => panic!("Uh, how'd you get an openparen on a non-origin line?"),
        },
        LexerState::OriginSOA { .. } | LexerState::OriginMarker => {
            Err("Got a newline after an unfinished origin marker, seriously?".to_string())
        }
        LexerState::PartialInclude {
            filename: _,
            domain_name: _,
            comment: _,
        } => todo!(),
        LexerState::PartialComment(comment) => {
            zone.comments.push((zone.lines, comment));
            Ok(LexerState::Idle)
        }
        LexerState::Idle => Ok(state),
        LexerState::Unknown(_) => todo!(),
        LexerState::InvalidLine(_) => todo!(),
    }
}

fn closedparen_state(zone: &mut ParsedZoneFile, state: LexerState) -> LexerState {
    match state {
        LexerState::PartialInclude {
            filename: _,
            domain_name: _,
            comment: _,
        } => todo!(),
        LexerState::PartialComment(_) => todo!(),
        // LexerState::PartialRrEntry {
        //     domain_name: _,
        //     entry_type: _,
        //     ttl: _,
        //     class: _,
        //     rtype: _,
        //     rdata: _,
        //     comment: _,
        // } => todo!(),
        LexerState::Idle => todo!(),
        LexerState::Unknown(_) => todo!(),
        LexerState::InvalidLine(_) => todo!(),
        LexerState::OriginMarker => todo!(),
        LexerState::OriginSOA {
            class,
            host,
            rrname,
            serial,
            refresh,
            retry,
            expire,
            minimum,
        } => {
            let rec = LineType::Soa {
                class,
                host: host.expect("Failed to get host for SOA record?"),
                rrname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
            };
            debug!("adding soa {:?}", rec);

            zone.soarecord = Some(rec);
            LexerState::Idle
        }
        LexerState::OpenParen(current_state) => match *current_state {
            // LexerState::PartialOrigin { domain_name: _, comment: _ } => todo!(),
            // LexerState::PartialOriginWithDomain { domain_name: _, comment: _ } => todo!(),
            LexerState::OriginSOA { .. } => *current_state,
            _ => panic!("WTF? {:?}", current_state)
            // LexerState::PartialInclude { filename, domain_name, comment } => todo!(),
            // LexerState::PartialComment(_) => todo!(),
            // LexerState::PartialRrEntry { domain_name, entry_type, ttl, class, rtype, rdata, comment } => todo!(),
            // LexerState::Idle => todo!(),
            // LexerState::Unknown(_) => todo!(),
            // LexerState::InvalidLine(_) => todo!(),
            // LexerState::OriginMarker => todo!(),
            // LexerState::OriginWithClass { class } => todo!(),
            // LexerState::OpenParen(_) => todo!(),
        },
    }
}

fn text_state(
    _zone: &mut ParsedZoneFile,
    value: String,
    state: LexerState,
) -> Result<LexerState, String> {
    match state.clone() {
        LexerState::OpenParen(boxed_state) => text_state(_zone, value, *boxed_state.clone()),
        LexerState::OriginSOA {
            class,
            host,
            rrname,
            serial,
            refresh,
            retry,
            expire,
            minimum,
        } => {
            debug!("lexerstate originsoa with value: {:?}", value);
            if value == *"SOA" {
                Ok(state)
            } else if host.is_none() {
                Ok(LexerState::OriginSOA {
                    class,
                    host: Some(value),
                    rrname,
                    serial,
                    refresh,
                    retry,
                    expire,
                    minimum,
                })
            } else if rrname.is_none() {
                let value = value.replace('\\', "").to_string();
                Ok(LexerState::OriginSOA {
                    class,
                    host,
                    rrname: Some(value),
                    serial: None,
                    refresh: None,
                    retry: None,
                    expire: None,
                    minimum: None,
                })
            } else if serial.is_none() {
                let serial = match value.parse::<u32>() {
                    Ok(val) => val,
                    Err(err) => {
                        return Err(format!(
                            "Failed to parse serial number from '{value}': {err:?}"
                        ))
                    }
                };
                Ok(LexerState::OriginSOA {
                    class,
                    host,
                    rrname,
                    serial: Some(serial),
                    refresh: None,
                    retry: None,
                    expire: None,
                    minimum: None,
                })
            } else if refresh.is_none() {
                Ok(LexerState::OriginSOA {
                    class,
                    host,
                    rrname,
                    serial,
                    refresh: Some(value.parse::<u32>().expect("Failed to parse refresh")),
                    retry: None,
                    expire: None,
                    minimum: None,
                })
            } else if retry.is_none() {
                Ok(LexerState::OriginSOA {
                    class,
                    host,
                    rrname,
                    serial,
                    refresh,
                    retry: Some(value.parse::<u32>().expect("Failed to parse retry")),
                    expire: None,
                    minimum: None,
                })
            } else if expire.is_none() {
                Ok(LexerState::OriginSOA {
                    class,
                    host,
                    rrname,
                    serial,
                    refresh,
                    retry,
                    expire: Some(value.parse::<u32>().expect("Failed to parse expire")),
                    minimum: None,
                })
            } else if minimum.is_none() {
                debug!("adding {} to minimum", value);
                Ok(LexerState::OriginSOA {
                    class,
                    host,
                    rrname,
                    serial,
                    refresh,
                    retry,
                    expire,
                    minimum: Some(value.parse::<u32>().expect("Failed to parse minimum")),
                })
            } else {
                panic!(
                    "Got a text value '{}' after the SOA record was finished!",
                    value
                );
            }
        }

        LexerState::OriginMarker => {
            // TODO: this could be a bunch of things?
            Ok(LexerState::OriginSOA {
                class: value,
                host: None,
                rrname: None,
                serial: None,
                refresh: None,
                retry: None,
                expire: None,
                minimum: None,
            })
        }
        // LexerState::PartialOriginWithDomain { .. } => {
        //     panic!("Already got the domain, shouldn't have another string!");
        // }
        // LexerState::PartialOrigin { .. } => Ok(LexerState::PartialOriginWithDomain {
        //     domain_name: value,
        //     comment: None,
        // }),
        LexerState::PartialInclude {
            filename: _,
            domain_name: _,
            comment: _,
        } => todo!(),
        LexerState::PartialComment(comment) => {
            Ok(LexerState::PartialComment(format!("{} {}", comment, value)))
        }
        // LexerState::PartialRrEntry {
        //     domain_name: _,
        //     entry_type: _,
        //     ttl: _,
        //     class: _,
        //     rtype: _,
        //     rdata: _,
        //     comment: _,
        // } => todo!(),
        LexerState::Idle | LexerState::Unknown(_) | LexerState::InvalidLine(_) => {
            Err(format!("Uh, what've we got here? '{state:?}"))
        }
    }
}
