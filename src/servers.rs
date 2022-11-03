use packed_struct::prelude::*;
use std::net::SocketAddr;
use std::str::{from_utf8, FromStr};
use std::time::Duration;
use tokio::io::{self, AsyncReadExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::timeout;

use crate::config::ConfigFile;
use crate::datastore::Command;
use crate::enums::{Agent, AgentState, PacketType, Rcode, RecordClass};
use crate::reply::Reply;
use crate::utils::*;
use crate::zones::ZoneRecord;
use crate::{Header, OpCode, Question, HEADER_BYTES, REPLY_TIMEOUT_MS, UDP_BUFFER_SIZE};

lazy_static! {
    static ref LOCALHOST: std::net::IpAddr = std::net::IpAddr::from_str("127.0.0.1").unwrap();
    // static ref VERSION_STRINGS: Vec<String> =
        // vec![String::from("version"), String::from("version.bind"),];
}

/// this handles a shutdown CHAOS request
async fn check_for_shutdown(r: &Reply, addr: &SocketAddr, config: &ConfigFile) -> Result<(), ()> {
    // when you get a CHAOS from localhost with "shutdown" break dat loop
    if let Some(q) = &r.question {
        if q.qclass == RecordClass::Chaos {
            if let Ok(qname) = from_utf8(&q.qname) {
                // Just don't do this on UDP, because we can't really tell who it's coming from.
                if qname == "shutdown" {
                    match config.ip_allow_lists.shutdown.contains(&addr.ip()) {
                        true => {
                            log::info!("Got CHAOS shutdown from {:?}, shutting down", addr.ip());
                            return Ok(());
                        }
                        false => log::warn!("Got CHAOS shutdown from {:?}, ignoring!", addr.ip()),
                    }
                }
            } else {
                log::error!(
                    "Failed to parse qname from {:?}, this shouldn't be able to happen!",
                    q.qname
                );
            }
        }
    };
    Err(())
}

/// this handles a version CHAOS request
// async fn check_for_version(r: &Reply, addr: &SocketAddr, config: &ConfigFile) -> Result<(), ()> {
//     // when you get a CHAOS from localhost with "VERSION" or "VERSION.BIND" we might respond
//     if let Some(q) = &r.question {
//         if q.qclass == RecordClass::Chaos {
//             if let Ok(qname) = from_utf8(&q.qname) {
//                 if VERSION_STRINGS.contains(&qname.to_ascii_lowercase()) & (config.ip_allow_lists.shutdown.contains(&addr.ip())) {
//                     info!("Got CHAOS VERSION from {:?}, responding.", addr.ip());
//                     return Ok(());
//                 } else {
//                     warn!("Got CHAOS VERSION from {:?}, ignoring!", addr.ip());
//                 }
//             } else {
//                 error!("Failed to parse qname from {:?}, this shouldn't be able to happen!", q.qname);
//             }
//         }
//     };
//     Err(())
// }

pub async fn udp_server(
    bind_address: SocketAddr,
    config: ConfigFile,
    datastore: mpsc::Sender<crate::datastore::Command>,
    _agent_tx: broadcast::Sender<AgentState>,
    mut _agent_rx: broadcast::Receiver<AgentState>,
) -> io::Result<()> {
    let udp_sock = match UdpSocket::bind(bind_address).await {
        Ok(value) => {
            log::info!("Started UDP listener on {}:{}", config.address, config.port);
            value
        }
        Err(error) => {
            log::error!("Failed to start UDP listener: {:?}", error);
            return Ok(());
        }
    };

    let mut udp_buffer = [0; UDP_BUFFER_SIZE];

    loop {
        let (len, addr) = match udp_sock.recv_from(&mut udp_buffer).await {
            Ok(value) => value,
            Err(error) => {
                log::error!("Error accepting connection via UDP: {:?}", error);
                continue;
            }
        };

        log::debug!("{:?} bytes received from {:?}", len, addr);

        let udp_result = match timeout(
            Duration::from_millis(REPLY_TIMEOUT_MS),
            parse_query(datastore.clone(), len, &udp_buffer, config.capture_packets),
        )
        .await
        {
            Ok(reply) => reply,
            Err(_) => {
                log::error!("Did not receive response from parse_query within 10 ms");
                continue;
            }
        };

        match udp_result {
            Ok(mut r) => {
                log::debug!("Result: {:?}", r);

                let reply_bytes: Vec<u8> = match r.as_bytes().await {
                    Ok(value) => {
                        // Check if it's too long and set truncate flag if so, it's safe to unwrap since we've already gone
                        if value.len() > UDP_BUFFER_SIZE {
                            let mut response_bytes = value.to_vec();
                            response_bytes.truncate(UDP_BUFFER_SIZE);
                            r = r.check_set_truncated().await;
                            let r = r.as_bytes_udp().await;
                            r.unwrap_or(value)
                        } else {
                            value
                        }
                    }
                    Err(error) => {
                        log::error!("Failed to parse reply {:?} into bytes: {:?}", r, error);
                        continue;
                    }
                };

                log::trace!("reply_bytes: {:?}", reply_bytes);
                let len = match udp_sock.send_to(&reply_bytes as &[u8], addr).await {
                    Ok(value) => value,
                    Err(err) => {
                        log::error!("Failed to send data back to {:?}: {:?}", addr, err);
                        return Ok(());
                    }
                };
                // let len = sock.send_to(r.answer.as_bytes(), addr).await?;
                log::trace!("{:?} bytes sent", len);
            }
            Err(error) => log::error!("Error: {}", error),
        }
    }
}

/// main handler for the TCP side of things
///
/// Ref <https://www.rfc-editor.org/rfc/rfc7766>

pub async fn tcp_server(
    bind_address: SocketAddr,
    config: ConfigFile,
    tx: mpsc::Sender<crate::datastore::Command>,
    agent_tx: broadcast::Sender<AgentState>,
    mut agent_rx: broadcast::Receiver<AgentState>,
) -> io::Result<()> {
    // TODO: add a configurable idle timeout for the TCP server
    let tcpserver = match TcpListener::bind(bind_address).await {
        Ok(value) => {
            log::info!("Started TCP listener on {}", bind_address);
            value
        }
        Err(error) => {
            log::error!("Failed to start TCP Server: {:?}", error);
            return Ok(());
        }
    };

    loop {
        let (mut stream, addr) = match tcpserver.accept().await {
            Ok(value) => value,
            Err(error) => panic!("Couldn't get data from TcpStrream: {:?}", error),
        };
        log::debug!("TCP connection from {:?}", addr);

        let (mut reader, writer) = stream.split();
        // TODO: this is a hilariously risky unwrap
        let msg_length: usize = reader.read_u16().await.unwrap().into();
        log::debug!("msg_length={msg_length}");
        // let mut buf: Vec<u8> = Vec::with_capacity(msg_length.into());
        let mut buf: Vec<u8> = vec![];

        while buf.len() < msg_length {
            let len = match reader.read_buf(&mut buf).await {
                Ok(size) => size,
                Err(error) => {
                    log::error!("Failed to read from TCP Stream: {:?}", error);
                    return Ok(());
                }
            };
            if len > 0 {
                log::debug!("Read {:?} bytes from TCP stream", len);
            }
        }

        crate::utils::hexdump(buf.clone());
        // the first two bytes of a tcp query is the message length
        // ref <https://www.rfc-editor.org/rfc/rfc7766#section-8>

        // check the message is long enough
        if buf.len() < msg_length {
            log::warn!(
                "Message length too short {}, wanted {}",
                buf.len(),
                msg_length + 2
            );
        } else {
            log::info!("TCP Message length ftw!");
        }

        // skip the TCP length header because rad
        let buf = &buf[0..msg_length];
        let result = match timeout(
            Duration::from_millis(REPLY_TIMEOUT_MS),
            parse_query(tx.clone(), msg_length, buf, config.capture_packets),
        )
        .await
        {
            Ok(reply) => reply,
            Err(_) => {
                log::error!(
                    "Did not receive response from parse_query within {REPLY_TIMEOUT_MS} ms"
                );
                continue;
            }
        };

        match result {
            Ok(r) => {
                log::debug!("TCP Result: {r:?}");

                // when you get a CHAOS from the allow-list with "shutdown" it's quitting time
                if check_for_shutdown(&r, &addr, &config).await.is_ok() {
                    if let Err(error) = agent_tx.send(AgentState::Stopped {
                        agent: Agent::TCPServer,
                    }) {
                        eprintln!("Failed to send UDPServer shutdown message: {error:?}");
                    };
                    if let Err(error) = tx.send(Command::Shutdown).await {
                        eprintln!("Failed to send shutdown command to datastore.. {error:?}");
                    };

                    return Ok(());
                }

                let reply_bytes: Vec<u8> = match r.as_bytes().await {
                    Ok(value) => value,
                    Err(error) => {
                        log::error!("Failed to parse reply {:?} into bytes: {:?}", r, error);
                        continue;
                    }
                };

                log::trace!("reply_bytes: {:?}", reply_bytes);

                let reply_bytes = &reply_bytes as &[u8];
                // send the outgoing message length
                let response_length: u16 = reply_bytes.len() as u16;
                let len = match writer.try_write(&response_length.to_be_bytes()) {
                    Ok(value) => value,
                    Err(err) => {
                        log::error!("Failed to send data back to {:?}: {:?}", addr, err);
                        return Ok(());
                    }
                };
                log::trace!("{:?} bytes sent", len);

                // send the data
                let len = match writer.try_write(reply_bytes) {
                    Ok(value) => value,
                    Err(err) => {
                        log::error!("Failed to send data back to {:?}: {:?}", addr, err);
                        return Ok(());
                    }
                };
                log::trace!("{:?} bytes sent", len);
            }
            Err(error) => log::error!("Error: {}", error),
        }
        if let Ok(agent_state) = agent_rx.try_recv() {
            log::info!("Got agent state: {:?}", agent_state);
        };
    }
}

/// Parses the rest of the packets once we have stripped the header off.
pub async fn parse_query(
    datastore: tokio::sync::mpsc::Sender<crate::datastore::Command>,
    len: usize,
    buf: &[u8],
    capture_packets: bool,
) -> Result<Reply, String> {
    if capture_packets {
        crate::packet_dumper::dump_bytes(
            buf[0..len].into(),
            crate::packet_dumper::DumpType::ClientRequest,
        )
        .await;
    }
    // we only want the first 12 bytes for the header
    let mut split_header: [u8; HEADER_BYTES] = [0; HEADER_BYTES];
    split_header.copy_from_slice(&buf[0..HEADER_BYTES]);
    // unpack the header for great justice
    let header = match crate::Header::unpack(&split_header) {
        Ok(value) => value,
        Err(error) => {
            // can't return a servfail if we can't unpack the header, they're probably doing something bad.
            return Err(format!("Failed to parse header: {:?}", error));
        }
    };
    log::trace!("Buffer length: {}", len);
    log::trace!("Parsed header: {:?}", header);
    get_result(header, len, buf, datastore).await
}

/// The generic handler for the packets once they've been pulled out of their protocol handlers. TCP has a slightly different stream format to UDP, y'know?
async fn get_result(
    header: Header,
    len: usize,
    buf: &[u8],
    datastore: mpsc::Sender<crate::datastore::Command>,
) -> Result<Reply, String> {
    log::trace!("called get_result(header={header}, len={len})");

    // if we get something other than a query, yeah nah.
    if header.opcode != OpCode::Query {
        return Err(format!("Invalid OPCODE, got {:?}", header.opcode));
    };

    let question = match Question::from_packets(&buf[HEADER_BYTES..len]).await {
        Ok(value) => {
            log::trace!("Parsed question: {:?}", value);
            value
        }
        Err(error) => {
            log::debug!("Failed to parse question: {} id={}", error, header.id);
            return reply_builder(header.id, Rcode::ServFail);
        }
    };

    // yeet them when we get a request we can't handle
    if !question.qtype.supported() {
        log::debug!(
            "Unsupported request: {} {:?}, returning NotImplemented",
            from_utf8(&question.qname).unwrap_or("<unable to parse>"),
            question.qtype,
        );
        return reply_builder(header.id, Rcode::NotImplemented);
    }

    // Check for CHAOS commands
    #[allow(clippy::collapsible_if)]
    if question.qclass == RecordClass::Chaos {
        if &question.normalized_name().unwrap() == "shutdown" {
            log::debug!("Got CHAOS shutdown!");
            return Ok(Reply {
                header,
                question: Some(question),
                answers: vec![],
                authorities: vec![],
                additional: vec![],
            });
        } /*else if VERSION_STRINGS.contains(&question.normalized_name().unwrap()) {
              trace!("Got CHAOS VERSION");
              return Ok(Reply {
                  header,
                  question: Some(question),
                  answers: vec![],
                  authorities: vec![],
                  additional: vec![],
              });
          }*/
    }

    // build the request to the datastore to make the query

    let (tx_oneshot, rx_oneshot) = oneshot::channel();
    let ds_req: Command = Command::Get {
        name: question.qname.clone(),
        rrtype: question.qtype,
        rclass: question.qclass,
        resp: tx_oneshot,
    };

    // here we talk to the datastore to pull the result
    match datastore.send(ds_req).await {
        Ok(_) => log::trace!("Sent a request to the datastore!"),
        // TODO: handle this properly
        Err(error) => log::error!("Error sending to datastore: {:?}", error),
    };

    let record: ZoneRecord = match rx_oneshot.await {
        Ok(value) => match value {
            Some(zr) => {
                log::debug!("DS Response: {}", zr);
                zr
            }
            None => {
                log::debug!("No response from datastore");
                return reply_nxdomain(header.id);
            }
        },
        Err(error) => {
            log::error!("Failed to get response from datastore: {:?}", error);
            return reply_builder(header.id, Rcode::ServFail);
        }
    };

    // this is our reply - static until that bit's done
    Ok(Reply {
        header: Header {
            id: header.id,
            qr: PacketType::Answer,
            opcode: header.opcode,
            authoritative: false, // TODO: are we authoritative
            truncated: false,     // TODO: work out if it's truncated (ie, UDP)
            recursion_desired: header.recursion_desired,
            recursion_available: header.recursion_desired, // TODO: work this out
            z: false,
            ad: true, // TODO: decide how the ad flag should be set -  "authentic data" - This requests the server to return whether all of the answer and
            // authority sections have all been validated as secure according to the security policy of the server. AD=1 indicates that all
            // records have been validated as secure and the answer is not from a OPT-OUT range. AD=0 indicate that some part of the answer
            // was insecure or not validated. This bit is set by default.
            cd: false, // TODO: figure this out -  CD (checking disabled) bit in the query. This requests the server to not perform DNSSEC validation of responses.
            rcode: Rcode::NoError, // TODO: this could be something to return if we don't die half way through
            qdcount: 1,
            ancount: record.typerecords.len() as u16, // TODO: work out how many we'll return
            nscount: 0,
            arcount: 0,
        },
        question: Some(question),
        answers: record.typerecords,
        authorities: vec![],
        additional: vec![],
    })
}
