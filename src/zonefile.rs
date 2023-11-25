//! Zone file parsing, based on [RFC1035 Master Files](https://datatracker.ietf.org/doc/html/rfc1035#autoid-48).

/*
valid lines

<blank>[<comment>]
$ORIGIN <domain-name> [<comment>]
$INCLUDE <file-name> [<domain-name>] [<comment>]
$TTL <u32> [<comment>]

... the other bits
*/

use log::{debug, error};
use regex::{Captures, Regex};

use crate::enums::{RecordClass, RecordType};
use crate::zones::FileZoneRecord;

#[derive(Debug)]
pub(crate) struct PartialRecord {
    host: Option<String>,
    class: Option<String>,
    rtype: Option<String>,
    preference: Option<u16>,
    rdata: Option<String>,
    ttl: Option<u32>,
}

#[derive(Debug, Clone)]
enum LexerState {
    Idle,
    PartialSoaRecord {
        host: String,
        class: String,
        rname: Option<String>,
        serial: Option<u32>,
        refresh: Option<u32>,
        retry: Option<u32>,
        expire: Option<u32>,
        minimum: Option<u32>,
        ttl: Option<u32>,
        in_brackets: bool,
    },
    PartialRecord {
        host: String,
        class: Option<String>,
        rtype: Option<String>,
        preference: Option<u16>,
        rdata: Option<String>,
        ttl: Option<u32>,
        in_quotes: bool,
    },
    Unknown(String),
}

#[derive(Debug, Clone)]
pub(crate) struct ZoneInclude {
    #[allow(dead_code)]
    filename: String,
    #[allow(dead_code)]
    domain_name: Option<String>,
    #[allow(dead_code)]
    comment: Option<String>,
}

#[derive(Default, Debug)]
pub(crate) struct ParsedZoneFile {
    /// which line the comment was on, and what it was
    pub comments: Vec<(usize, String)>,
    pub soarecord: Option<FileZoneRecord>,
    pub records: Vec<PartialRecord>,
    pub origin: Option<String>,
    pub includes: Vec<ZoneInclude>,
    pub lines: usize,
    /// ttl of the zone
    pub ttl: Option<u32>,
    /// for multi=line records so we can get the last one
    pub last_used_host: Option<String>,
}

fn get_read_len_from_caps(caps: &Captures) -> usize {
    caps.iter()
        .filter_map(|c| c.map(|c| c.end()))
        .max()
        .unwrap()
}

const R_HOST: &str = r"(?<host>[a-zA-Z0-9\.\_-]+)";
const R_RNAME: &str = r"(?P<rname>[a-zA-Z0-9\.\_-]+\.[a-zA-Z0-9\.\_-]+)";
const R_CLASS: &str = r#"(?P<class>[A-Z]+)"#;
const R_TYPE: &str = r#"(?P<rtype>[A-Z]+)"#;
const R_TTL: &str = r"(?P<ttl>\d+)";
const R_DATA: &str = r#"(?P<rdata>("[^"]+"|\S+))"#;

lazy_static! {
    static ref REGEX_TTL: Regex = Regex::new(r"^\$TTL\s+(?P<ttl>\d+)").unwrap();
    static ref REGEX_ORIGIN: Regex = Regex::new(r"^\$ORIGIN\s+(?P<domain>\S+)").unwrap();
    static ref REGEX_INCLUDE: Regex =
        Regex::new(r"^\$INCLUDE\s+(?P<filename>\S+)\s*(?P<domain>\S+)(?P<comment>;[^\n]*)?")
            .unwrap();
    static ref REGEX_COMMENT: Regex = Regex::new(r"^;(?P<comment>[^\n]*)").unwrap();
    static ref REGEX_SOA_MATCHER: Regex = Regex::new(&format!(
        r#"^(?P<domain>[\@a-zA-Z0-9\.\_-]+)\s+((?P<ttl>\d*)\s+|){}\s+SOA\s+{}\s+{}"#,
        R_CLASS, R_HOST, R_RNAME
    ))
    .unwrap();
    static ref HOST_TTL_CLASS_TYPE_RDATA: Regex = Regex::new(&format!(
        r#"^{}\s+{}\s+{}\s+{}\s+{}"#,
        R_HOST, R_TTL, R_CLASS, R_TYPE, R_DATA
    ))
    .unwrap();
    static ref HOST_CLASS_TYPE_RDATA: Regex = Regex::new(&format!(
        r#"^{}\s+{}\s+{}\s+{}"#,
        R_HOST, R_CLASS, R_TYPE, R_DATA,
    ))
    .unwrap();
}

#[derive(Debug)]
pub enum ParserError {
    IncompleteSoa,
    // NoSoa,
    NoOrigin,
    MissingFields(String),
    // NoTtl,
    Regex,
    // RegexNameMissing,
    UnHandledState(String),
    TooManyLoops { content_length: usize, loops: usize },
    FailedConversion(String),
}

impl ToString for ParserError {
    fn to_string(&self) -> String {
        match self {
            ParserError::IncompleteSoa => "Incomplete SOA record".to_string(),
            ParserError::NoOrigin => "No origin found".to_string(),
            ParserError::Regex => "Regex match error".to_string(),
            ParserError::MissingFields(dump) => {
                format!("Missing fields while parsing record: {dump}")
            }
            ParserError::UnHandledState(msg) => msg.clone(),
            ParserError::FailedConversion(msg) => msg.clone(),
            ParserError::TooManyLoops {
                content_length,
                loops,
            } => format!(
                "Too many loops - content_length: {}, loops: {}",
                content_length, loops
            ),
            // ParserError::NoSoa => "No SOA record found".to_string(),
            // ParserError::NoTtl => "No TTL found".to_string(),
            // ParserError::RegexNameMissing => "Regex match name not found".to_string(),
        }
    }
}

#[allow(dead_code)]
/// A hilariously overcomplicated thing
pub(crate) fn parse_file(contents: &str) -> Result<ParsedZoneFile, ParserError> {
    // let mut lex = ZoneFileToken::lexer(contents);
    let max_loops = contents.len() * 10;

    let mut contents = contents.to_string();
    let mut state = LexerState::Idle;
    let mut zone: ParsedZoneFile = ParsedZoneFile::default();

    let mut loops = 0;

    debug!("original length: {}", contents.len());
    // strip out the extra nonsense
    let cstr = contents.replace('\t', " ");
    contents = cstr.trim_start().to_string();

    let matchers = [
        HOST_CLASS_TYPE_RDATA.clone(),
        HOST_TTL_CLASS_TYPE_RDATA.clone(),
    ];

    loop {
        let mut read_len = 1;
        debug!("******************************");
        debug!("loop start");
        debug!("******************************");
        debug!("state: {:?}", state);

        if contents.starts_with(';') {
            // we're in a comment, capture until the next line break
            let data = match REGEX_COMMENT.captures(&contents) {
                None => return Err(ParserError::Regex),
                Some(val) => val,
            };

            let comment = match data.name("comment") {
                Some(val) => val,
                None => return Err(ParserError::Regex),
            };
            zone.comments.push((
                zone.lines,
                #[allow(clippy::expect_used)]
                comment.as_str().to_string(),
            ));
            read_len = get_read_len_from_caps(&data);
        } else if contents.starts_with('(') {
            debug!("Found a bracket...");
            if let LexerState::PartialSoaRecord {
                host,
                class,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
                ttl,
                in_brackets: _,
            } = state
            {
                state = LexerState::PartialSoaRecord {
                    host,
                    class,
                    rname,
                    serial,
                    refresh,
                    retry,
                    expire,
                    minimum,
                    ttl,
                    in_brackets: true,
                };
                read_len = 1;
            } else {
                panic!("Open brackets on a non-soa line?")
            }
            // todo!("match start of brackets");
        } else if contents.starts_with(')') {
            debug!("Found a closing bracket...");
            read_len = 1;
            match state.clone() {
                LexerState::PartialSoaRecord { .. } => {
                    let soarecord: FileZoneRecord = (state, &zone).try_into().map_err(|err| {
                        ParserError::FailedConversion(format!(
                            "Failed to parse SOA record: {:?}",
                            err
                        ))
                    })?;

                    zone.ttl = Some(soarecord.ttl.clone());
                    zone.soarecord = Some(soarecord);
                }
                LexerState::PartialRecord {
                    host,
                    class,
                    rtype,
                    preference,
                    rdata,
                    ttl,
                    in_quotes: _,
                } => zone.records.push(PartialRecord {
                    host: Some(host),
                    class,
                    rtype,
                    preference,
                    rdata,
                    ttl,
                }),
                _ => {
                    return Err(ParserError::UnHandledState(format!(
                        "end of bracket, unhandled state: {:?}",
                        state
                    )));
                }
            }
            state = LexerState::Idle;
        } else if let Some(caps) = REGEX_ORIGIN.captures(&contents) {
            debug!("ORIGIN LINE: {:?}", caps);
            zone.origin = caps.name("domain").map(|d| d.as_str().to_string());
            read_len = get_read_len_from_caps(&caps);
        } else if let Some(caps) = REGEX_TTL.captures(&contents) {
            debug!("TTL LINE: {:?}", caps);
            zone.ttl = caps
                .name("ttl")
                .map(|ttl| ttl.as_str().parse::<u32>().expect("Failed to parse TTL!"));
            read_len = get_read_len_from_caps(&caps);
        } else if let Some(caps) = REGEX_INCLUDE.captures(&contents) {
            #[cfg(debug_assertions)]
            debug!("INCLUDE LINE: {:?}", caps);
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
        // } else if let Some(caps) = regex_comment.captures(&contents) {
        //     #[cfg(debug_assertions)]
        //     debug!("Comment!");
        //     let comment = match caps.name("comment") {
        //         Some(val) => val,
        //         None => {
        //             return Err("Failed to pull a comment, when we'd just matched it?".to_string())
        //         }
        //     };
        //     zone.comments
        //         .push((zone.lines, comment.as_str().to_string()));
        //     read_len = get_read_len_from_caps(&caps);
        } else if let Some(caps) = REGEX_SOA_MATCHER.captures(&contents) {
            #[cfg(debug_assertions)]
            debug!("SOA Matched: {:#?}", caps);
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
                rname: caps.name("rname").map(|c| c.as_str().to_string()),
                serial: None,
                refresh: None,
                retry: None,
                expire: None,
                minimum: None,
                ttl: None,
                in_brackets: false,
            };
            debug!("AFTER SOA MATCH {:?}", state);
            read_len = get_read_len_from_caps(&caps);
            // todo!()
        } else if contents.starts_with('@') {
            match zone.origin {
                Some(_) => zone.last_used_host = zone.origin.clone(),
                None => return Err(ParserError::NoOrigin),
            }
            // zone.last_used_host = zone.origin.clone();
        } else {
            let mut caps: Option<Captures<'_>> = None;
            for matcher in matchers.iter() {
                if let Some(captures) = matcher.captures(&contents) {
                    #[cfg(debug_assertions)]
                    debug!("{:?} matched ", matcher);
                    caps = Some(captures);
                    state = LexerState::Idle;

                    break;
                }
            }
            if let Some(caps) = caps {
                #[cfg(debug_assertions)]
                debug!("caps: {:#?}", caps);
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

                let new_record = PartialRecord {
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
                            None => return Err(ParserError::NoOrigin),
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
                        in_quotes: false,
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
                        in_quotes: false,
                    }
                } else {
                    LexerState::Unknown(next_term.to_string())
                };

                let mut curr_state = state.clone();
                read_len = next_term.len() + 1;
                state = match &mut curr_state {
                    LexerState::Idle => partial_state,
                    LexerState::PartialSoaRecord {
                        host,
                        class,
                        rname: rrname,
                        serial,
                        refresh,
                        retry,
                        expire,
                        minimum,
                        ttl,
                        in_brackets,
                    } => {
                        if serial.is_none() {
                            *serial = next_term
                                .parse::<u32>()
                                .map_err(|_| "failed to parse the serial".to_string())
                                .ok();
                        } else if refresh.is_none() {
                            *refresh = next_term
                                .parse::<u32>()
                                .map_err(|_| "failed to parse refresh".to_string())
                                .ok();
                        } else if retry.is_none() {
                            *retry = next_term
                                .parse::<u32>()
                                .map_err(|_| "failed to parse retry".to_string())
                                .ok();
                        } else if expire.is_none() {
                            *expire = next_term
                                .parse::<u32>()
                                .map_err(|_| "failed to parse expire".to_string())
                                .ok();
                        } else if minimum.is_none() {
                            dbg!(&next_term);
                            *minimum = next_term
                                .parse::<u32>()
                                .map_err(|_| "failed to parse minimum".to_string())
                                .ok();
                        } else if ttl.is_none() {
                            *ttl = next_term
                                .parse::<u32>()
                                .map_err(|err| {
                                    format!("failed to parse {} as u32: {:?}", next_term, err)
                                })
                                .ok();
                        } else {
                            panic!("too many terms in soa record! {}", next_term);
                        }
                        LexerState::PartialSoaRecord {
                            host: host.to_string(),
                            class: class.to_string(),
                            rname: rrname.as_ref().map(|r| r.to_owned()),
                            serial: serial.map(|r| r.to_owned()),
                            refresh: refresh.map(|r| r.to_owned()),
                            retry: retry.map(|r| r.to_owned()),
                            expire: expire.map(|r| r.to_owned()),
                            minimum: minimum.map(|r| r.to_owned()),
                            ttl: ttl.map(|r| r.to_owned()),
                            in_brackets: in_brackets.to_owned(),
                        }
                    }
                    LexerState::PartialRecord {
                        host: _,
                        class,
                        rtype,
                        preference,
                        rdata,
                        ttl,
                        in_quotes,
                    } => {
                        if let LexerState::PartialRecord {
                            host,
                            class: p_class,
                            rtype: p_rtype,
                            preference: p_preference,
                            rdata: p_rdata,
                            ttl: p_ttl,
                            in_quotes: _,
                        } = partial_state
                        {
                            let new_class = match p_class {
                                Some(val) => Some(val),
                                None => class.to_owned(),
                            };
                            let new_rtype = match p_rtype {
                                Some(val) => Some(val),
                                None => rtype.to_owned(),
                            };
                            let new_preference = match p_preference {
                                Some(val) => Some(val),
                                None => preference.to_owned(),
                            };
                            let new_rdata = match p_rdata {
                                Some(val) => Some(val),
                                None => rdata.to_owned(),
                            };
                            let new_ttl = match p_ttl {
                                Some(val) => Some(val),
                                None => ttl.to_owned(),
                            };

                            LexerState::PartialRecord {
                                host,
                                class: new_class,
                                rtype: new_rtype,
                                preference: new_preference,
                                rdata: new_rdata,
                                ttl: new_ttl,
                                in_quotes: in_quotes.to_owned(),
                            }
                        } else {
                            panic!()
                        }
                    }
                    LexerState::Unknown(_) => todo!(),
                }
            }
        }

        if read_len == contents.len() {
            debug!("done!");
            break;
        }
        if read_len < contents.len() {
            let (chunk, buf) = contents.split_at(read_len);
            if chunk.contains('\n') {
                zone.lines += 1;
            }
            contents = buf.trim().to_string();
        } else {
            loops = max_loops;
        }

        debug!("current state: {:?}", state);
        debug!("current line: {:?}", contents.split('\n').next());

        if contents.trim().is_empty() {
            break;
        }
        if loops > max_loops {
            let errmsg = format!(
                "Looped too many times, bailing! - content length = {} loops = {}",
                contents.len(),
                loops
            );
            error!("{}", &errmsg);
            return Err(ParserError::TooManyLoops {
                content_length: contents.len(),
                loops,
            });
        } else {
            loops += 1;
        }
    }

    // so we got to the end of parsing, what's state's the lexer in?
    match state.clone() {
        LexerState::Idle => debug!("Idle, OK"),
        LexerState::PartialSoaRecord {
            host: _,
            class: _,
            rname: _,
            serial,
            refresh,
            retry,
            expire,
            minimum,
            ttl: _,
            in_brackets: _,
        } => {
            if serial.is_none()
                | refresh.is_none()
                | retry.is_none()
                | expire.is_none()
                | minimum.is_none()
            {
                return Err(ParserError::IncompleteSoa);
            }

            let soarecord: FileZoneRecord = (state, &zone)
                .try_into()
                .map_err(|err| ParserError::FailedConversion(err))?;

            zone.ttl = Some(soarecord.ttl.clone());
            zone.soarecord = Some(soarecord);

            // zone.soarecord = Some(LineType::Soa {
            //     host,
            //     class,
            //     rname: rrname,
            //     serial,
            //     refresh,
            //     retry,
            //     expire,
            //     minimum,
            //     ttl,
            // });
        }
        LexerState::PartialRecord {
            host,
            class,
            rtype,
            preference,
            rdata,
            ttl,
            in_quotes: _,
        } => {
            if class.is_none()
                | rtype.is_none()
                | preference.is_none()
                | rdata.is_none()
                | ttl.is_none()
            {
                return Err(ParserError::MissingFields(format!("{:?}", state)));
            }
            zone.records.push(PartialRecord {
                host: Some(host),
                class,
                rtype,
                preference,
                rdata,
                ttl,
            })
        }
        LexerState::Unknown(_) => todo!(),
    }

    Ok(zone)
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum ZoneFileLine {
    Soa(FileZoneRecord),
    Record(FileZoneRecord),
    Comment(String),
    Include(ZoneInclude),
    Origin,
    Ttl(u32),
}

impl TryFrom<ParsedZoneFile> for Vec<FileZoneRecord> {
    type Error = String;
    fn try_from(zone: ParsedZoneFile) -> Result<Self, String> {
        let mut res = Vec::new();
        let origin = if let Some(zone_origin) = zone.origin {
            zone_origin
        } else {
            zone.soarecord.clone().expect("No SOA record?").name
        };
        let zone_ttl = if let Some(ttl) = zone.ttl {
            ttl
        } else {
            zone.soarecord.expect("No SOA record?").ttl
        };

        res.push(FileZoneRecord {
            id: None,
            zoneid: None,
            name: origin,
            rrtype: RecordType::SOA.to_string(),
            class: RecordClass::Internet,
            ttl: zone_ttl,
            rdata: "".to_string(),
        });

        for record in zone.records {
            let fzr: FileZoneRecord = record
                .try_into()
                .map_err(|err| format!("Failed to parse record: {:?}", err))?;
            res.push(fzr);
        }

        Ok(res)
    }
}

impl TryFrom<(LexerState, &ParsedZoneFile)> for FileZoneRecord {
    type Error = String;
    fn try_from(input: (LexerState, &ParsedZoneFile)) -> Result<Self, Self::Error> {
        let (value, zone) = input;

        if let LexerState::PartialSoaRecord {
            host: _,
            class,
            rname: _,
            serial: _,
            refresh: _,
            retry: _,
            expire: _,
            minimum: _,
            ttl,
            in_brackets: _,
        } = value
        {
            // keep building the soa record
            debug!("in the soa record");

            let name = match zone.origin.clone() {
                Some(val) => val,
                None => return Err("went to write the soa record and had no origin!".to_string()),
            };

            let ttl = match ttl {
                Some(val) => val,
                None => match zone.ttl {
                    Some(val) => val,
                    None => match zone.soarecord.clone() {
                        Some(soa) => soa.ttl,
                        None => return Err("No TTL found!".to_string()),
                    },
                },
            };
            let class = RecordClass::try_from(class.as_str()).map_err(|err| err.to_string())?;

            Ok(FileZoneRecord {
                id: None,
                zoneid: None,
                name,
                rrtype: RecordType::SOA.to_string(),
                class,
                ttl,
                rdata: "".to_string(),
            })
        } else {
            Err("Not a partial SOA record!".to_string())
        }
    }
}

impl TryFrom<PartialRecord> for FileZoneRecord {
    type Error = String;

    fn try_from(value: PartialRecord) -> Result<Self, Self::Error> {
        let class = match value.class.clone() {
            Some(class) => RecordClass::try_from(class.as_str()).map_err(|err| err.to_string())?,
            None => return Err("No class record?".to_string()),
        };
        let name = match value.host.clone() {
            Some(val) => val,
            None => return Err("No host record?".to_string()),
        };

        let rrtype = match value.rtype {
            Some(val) => val,
            None => return Err("No rtype record?".to_string()),
        };
        let ttl: u32 = match value.ttl {
            Some(val) => val,
            None => return Err("No ttl value?".to_string()),
        };
        let rdata = match value.rdata {
            Some(val) => match value.preference {
                Some(pref) => format!("{} {}", pref, val),
                None => val,
            },
            None => return Err("No rdata value?".to_string()),
        };

        Ok(FileZoneRecord {
            id: None,
            zoneid: None,
            name,
            rrtype,
            class,
            ttl,
            rdata,
        })
    }
}
