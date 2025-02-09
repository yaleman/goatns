use concread::cowcell::asynch::CowCellReadTxn;
use packed_struct::prelude::*;
use std::io::Error;
use std::net::SocketAddr;
use std::str::from_utf8;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::io::{self, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, field, info, instrument, trace, warn};

use crate::config::ConfigFile;
use crate::datastore::Command;
use crate::enums::{Agent, AgentState, PacketType, Rcode, RecordClass, RecordType};
use crate::error::GoatNsError;
use crate::reply::{reply_any, reply_builder, reply_nxdomain, Reply};
use crate::resourcerecord::{DNSCharString, InternalResourceRecord};
use crate::zones::ZoneRecord;
use crate::{Header, OpCode, Question, HEADER_BYTES, REPLY_TIMEOUT_MS, UDP_BUFFER_SIZE};

pub(crate) enum ChaosResult {
    Refused(Reply),
    Shutdown(Reply),
}

/// this handles a shutdown CHAOS request
async fn check_for_shutdown(r: &Reply, allowed_shutdown: bool) -> Result<ChaosResult, GoatNsError> {
    // when you get a CHAOS from localhost with "shutdown" break dat loop
    if let Some(q) = &r.question {
        if q.qclass == RecordClass::Chaos {
            let qname = from_utf8(&q.qname).inspect_err(|e| {
                error!(
                    "Failed to parse qname from {:?}, this shouldn't be able to happen! {e:?}",
                    q.qname
                );
            })?;
            // Just don't do this on UDP, because we can't really tell who it's coming from.
            if qname == "shutdown" {
                // when we get a request, we update the response to say if we're going to do it or not
                match allowed_shutdown {
                    true => {
                        info!("Got CHAOS shutdown, shutting down");
                        let mut chaos_reply = r.clone();
                        chaos_reply.answers.push(CHAOS_OK.clone());
                        return Ok(ChaosResult::Shutdown(chaos_reply));
                    }
                    false => {
                        // get lost!  🤣
                        warn!("Got CHAOS shutdown, ignoring!");
                        let mut chaos_reply = r.clone();
                        chaos_reply.answers.push(CHAOS_NO.clone());
                        chaos_reply.header.rcode = Rcode::Refused;
                        return Ok(ChaosResult::Refused(chaos_reply));
                    }
                };
            }
        }
    };

    let mut chaos_reply = r.clone();
    chaos_reply.answers.push(CHAOS_NO.clone());
    chaos_reply.header.rcode = Rcode::Refused;
    Ok(ChaosResult::Refused(chaos_reply))
}

// this handles a version CHAOS request
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
    config: CowCellReadTxn<ConfigFile>,
    datastore_sender: mpsc::Sender<crate::datastore::Command>,
    _agent_tx: broadcast::Sender<AgentState>,
) -> io::Result<()> {
    let udp_sock = match UdpSocket::bind(config.dns_listener_address().map_err(|_err| {
        GoatNsError::StartupError("Failed to get DNS listener address on startup!".to_string())
    })?)
    .await
    {
        Ok(value) => {
            info!("Started UDP listener on {}:{}", config.address, config.port);
            value
        }
        Err(error) => {
            error!("Failed to start UDP listener: {:?}", error);
            return Ok(());
        }
    };

    // TODO: this needs to be bigger to handle edns0-negotiated queries
    let mut udp_buffer = [0; UDP_BUFFER_SIZE];

    loop {
        let (len, addr) = match udp_sock.recv_from(&mut udp_buffer).await {
            Ok(value) => value,
            Err(error) => {
                error!("Error accepting connection via UDP: {:?}", error);
                continue;
            }
        };

        debug!("{:?} bytes received from {:?}", len, addr);

        let udp_result = match timeout(
            Duration::from_millis(REPLY_TIMEOUT_MS),
            parse_query(
                datastore_sender.clone(),
                len,
                &udp_buffer,
                config.capture_packets,
                QueryProtocol::Udp,
            ),
        )
        .await
        {
            Ok(reply) => reply,
            Err(_) => {
                error!("Did not receive response from parse_query within 10 ms");
                continue;
            }
        };

        match udp_result {
            Ok(mut r) => {
                debug!("Ok Result: {:?}", r);

                let reply_bytes: Vec<u8> = match r.as_bytes().await {
                    Ok(value) => {
                        // Check if it's too long and set truncate flag if so, it's safe to unwrap since we've already gone
                        if value.len() > UDP_BUFFER_SIZE {
                            r = r.check_set_truncated().await;
                            r.as_bytes_udp().await?
                        } else {
                            value
                        }
                    }
                    Err(error) => {
                        error!("Failed to parse reply {:?} into bytes: {:?}", r, error);
                        continue;
                    }
                };

                trace!("reply_bytes: {:?}", reply_bytes);
                let len = match udp_sock.send_to(&reply_bytes as &[u8], addr).await {
                    Ok(value) => value,
                    Err(err) => {
                        error!("Failed to send data back to {:?}: {:?}", addr, err);
                        return Ok(());
                    }
                };
                // let len = sock.send_to(r.answer.as_bytes(), addr).await?;
                trace!("{:?} bytes sent", len);
            }
            Err(error) => error!("Error: {}", error),
        }
    }
}

#[instrument(level = "info", skip_all)]
pub async fn tcp_conn_handler(
    stream: &mut TcpStream,
    addr: SocketAddr,
    datastore_sender: mpsc::Sender<Command>,
    agent_tx: broadcast::Sender<AgentState>,
    capture_packets: bool,
    allowed_shutdown: bool,
) -> io::Result<()> {
    let (mut reader, writer) = stream.split();
    let msg_length: usize = reader.read_u16().await?.into();
    debug!("msg_length={msg_length}");
    let mut buf: Vec<u8> = vec![];

    while buf.len() < msg_length {
        let len = match reader.read_buf(&mut buf).await {
            Ok(size) => size,
            Err(error) => {
                error!("Failed to read from TCP Stream: {:?}", error);
                return Ok(());
            }
        };
        if len > 0 {
            debug!("Read {:?} bytes from TCP stream", len);
        }
    }
    // TODO: why are we hexdumping this?
    if let Err(err) = crate::utils::hexdump(&buf) {
        error!("Failed to hexdump buffer: {:?}", err);
    };
    // the first two bytes of a tcp query is the message length
    // ref <https://www.rfc-editor.org/rfc/rfc7766#section-8>

    // check the message is long enough
    if buf.len() < msg_length {
        warn!(
            "Message length too short {}, wanted {}",
            buf.len(),
            msg_length + 2
        );
    } else {
        info!("TCP Message length ftw!");
    }

    // skip the TCP length header because rad
    let buf = &buf[0..msg_length];
    let result = match timeout(
        Duration::from_millis(REPLY_TIMEOUT_MS),
        parse_query(
            datastore_sender.clone(),
            msg_length,
            buf,
            capture_packets,
            QueryProtocol::Tcp,
        ),
    )
    .await
    {
        Ok(reply) => reply,
        Err(_) => {
            error!("Did not receive response from parse_query within {REPLY_TIMEOUT_MS} ms");
            return Ok(());
        }
    };

    match result {
        Ok(r) => {
            debug!("TCP Result: {r:?}");

            // when you get a CHAOS from the allow-list with "shutdown" it's quitting time
            let r = match check_for_shutdown(&r, allowed_shutdown).await {
                // no change here
                Err(err) => {
                    error!("Failed to check for shutdown: {:?}", err);
                    return Ok(());
                }
                Ok(reply) => match reply {
                    ChaosResult::Refused(response) => response,
                    ChaosResult::Shutdown(response) => {
                        if let Err(error) = agent_tx.send(AgentState::Stopped {
                            agent: Agent::TCPServer,
                        }) {
                            eprintln!("Failed to send UDPServer shutdown message: {error:?}");
                        };
                        if let Err(error) = datastore_sender.send(Command::Shutdown).await {
                            eprintln!("Failed to send shutdown command to datastore.. {error:?}");
                        };
                        response
                    }
                },
            };

            let reply_bytes: Vec<u8> = match r.as_bytes().await {
                Ok(value) => value,
                Err(error) => {
                    error!("Failed to parse reply {:?} into bytes: {:?}", r, error);
                    return Ok(());
                }
            };

            trace!("reply_bytes: {:?}", reply_bytes);

            let reply_bytes = &reply_bytes as &[u8];
            // send the outgoing message length
            let response_length: u16 = reply_bytes.len() as u16;
            let len = match writer.try_write(&response_length.to_be_bytes()) {
                Ok(value) => value,
                Err(err) => {
                    error!("Failed to send data back to {:?}: {:?}", addr, err);
                    return Ok(());
                }
            };
            trace!("{:?} bytes sent", len);

            // send the data
            let len = match writer.try_write(reply_bytes) {
                Ok(value) => value,
                Err(err) => {
                    error!("Failed to send data back to {:?}: {:?}", addr, err);
                    return Ok(());
                }
            };
            trace!("{:?} bytes sent", len);
        }
        Err(error) => error!("Error: {}", error),
    }
    Ok(())
}

/// main handler for the TCP side of things
///
/// Ref <https://www.rfc-editor.org/rfc/rfc7766>
pub async fn tcp_server(
    config: CowCellReadTxn<ConfigFile>,
    tx: mpsc::Sender<crate::datastore::Command>,
    agent_tx: broadcast::Sender<AgentState>,
    // mut agent_rx: broadcast::Receiver<AgentState>,
) -> io::Result<()> {
    let mut agent_rx = agent_tx.subscribe();
    let tcpserver = match TcpListener::bind(config.dns_listener_address().map_err(|_err| {
        GoatNsError::StartupError("Failed to get DNS listener address on startup!".to_string())
    })?)
    .await
    {
        Ok(value) => {
            info!(
                "Started TCP listener on {}",
                config
                    .dns_listener_address()
                    .map_err(|_err| GoatNsError::StartupError(
                        "Failed to get DNS listener address on startup!".to_string()
                    ))?
            );
            value
        }
        Err(error) => {
            error!("Failed to start TCP Server: {:?}", error);
            return Ok(());
        }
    };

    let tcp_client_timeout = config.tcp_client_timeout;
    let shutdown_ip_address_list = config.ip_allow_lists.shutdown.to_vec();
    let capture_packets = config.capture_packets;
    loop {
        let (mut stream, addr) = match tcpserver.accept().await {
            Ok(value) => value,
            Err(err) => {
                error!("Couldn't get data from TcpStream: {:?}", err);
                continue;
            }
        };

        let allowed_shutdown = shutdown_ip_address_list.contains(&addr.ip());
        debug!("TCP connection from {:?}", addr);
        let loop_tx = tx.clone();
        let loop_agent_tx = agent_tx.clone();
        tokio::spawn(async move {
            if timeout(
                Duration::from_secs(tcp_client_timeout),
                tcp_conn_handler(
                    &mut stream,
                    addr,
                    loop_tx,
                    loop_agent_tx,
                    capture_packets,
                    allowed_shutdown,
                ),
            )
            .await
            .is_err()
            {
                warn!(
                    "TCP Connection from {addr:?} terminated after {} seconds.",
                    tcp_client_timeout
                );
            }
        })
        .await?;

        if let Ok(agent_state) = agent_rx.try_recv() {
            info!("Got agent state: {:?}", agent_state);
        };
    }
}

pub(crate) enum QueryProtocol {
    Udp,
    Tcp,
    DoH,
}

impl std::fmt::Display for QueryProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryProtocol::Udp => write!(f, "UDP"),
            QueryProtocol::Tcp => write!(f, "TCP"),
            QueryProtocol::DoH => write!(f, "DoH"),
        }
    }
}

/// Parses the rest of the packets once we have stripped the header off.
#[instrument(level = "info", skip_all, fields(protocol=protocol.to_string()))]
pub async fn parse_query(
    datastore: tokio::sync::mpsc::Sender<crate::datastore::Command>,
    len: usize,
    buf: &[u8],
    capture_packets: bool,
    protocol: QueryProtocol,
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
    trace!("Buffer length: {}", len);
    trace!("Parsed header: {:?}", header);
    get_result(header, len, buf, datastore).await
}

static CHAOS_OK: LazyLock<InternalResourceRecord> = LazyLock::new(|| InternalResourceRecord::TXT {
    txtdata: DNSCharString::from("OK"),
    ttl: 0,
    class: RecordClass::Chaos,
});
static CHAOS_NO: LazyLock<InternalResourceRecord> = LazyLock::new(|| InternalResourceRecord::TXT {
    txtdata: DNSCharString::from("NO"),
    ttl: 0,
    class: RecordClass::Chaos,
});

/// The generic handler for the packets once they've been pulled out of their protocol handlers. TCP has a slightly different stream format to UDP, y'know?
#[instrument(level="info", skip_all, fields(qname=field::Empty, qtype=field::Empty))]
async fn get_result(
    header: Header,
    len: usize,
    buf: &[u8],
    datastore: mpsc::Sender<crate::datastore::Command>,
) -> Result<Reply, String> {
    trace!("called get_result(header={header}, len={len})");

    // if we get something other than a query, yeah nah.
    if header.opcode != OpCode::Query {
        return Err(format!("Invalid OPCODE, got {:?}", header.opcode));
    };

    let question = match Question::from_packets(&buf[HEADER_BYTES..len]) {
        Ok(value) => {
            trace!("Parsed question: {:?}", value);

            value
        }
        Err(error) => {
            debug!("Failed to parse question: {} id={}", error, header.id);
            return reply_builder(header.id, Rcode::ServFail);
        }
    };

    // record the details of the query
    let span = tracing::Span::current();
    if !span.is_disabled() {
        let qname_string = from_utf8(&question.qname).unwrap_or("<unable to parse>");
        span.record("qname", qname_string);
        span.record("qtype", question.qtype.to_string());
    }

    // yeet them when we get a request we can't handle
    if !question.qtype.supported() {
        debug!(
            "Unsupported request: {} {:?}, returning NotImplemented",
            from_utf8(&question.qname).unwrap_or("<unable to parse>"),
            question.qtype,
        );
        return reply_builder(header.id, Rcode::NotImplemented);
    }

    // Check for CHAOS commands
    if question.qclass == RecordClass::Chaos && &question.normalized_name()? == "shutdown" {
        debug!("Got CHAOS shutdown!");
        return Ok(Reply {
            header,
            question: Some(question),
            answers: vec![],
            authorities: vec![],
            additional: vec![],
        });
    }

    if let RecordType::ANY {} = question.qtype {
        // TODO this should check to see if we have a zone record, but that requires walking down the qname record recursively, which is its own thing. We just YOLO a HINFO back for any request now.
        return reply_any(header.id, &question);
    };

    // build the request to the datastore to make the query
    let (tx_oneshot, rx_oneshot) = oneshot::channel();
    let ds_req: Command = Command::GetRecord {
        name: question.qname.clone(),
        rrtype: question.qtype,
        rclass: question.qclass,
        resp: tx_oneshot,
    };

    // here we talk to the datastore to pull the result
    match datastore.send(ds_req).await {
        Ok(_) => trace!("Sent a request to the datastore!"),
        // TODO: handle errors sending to the DS properly
        Err(error) => error!("Error sending to datastore: {:?}", error),
    };

    let record: ZoneRecord = match rx_oneshot.await {
        Ok(value) => match value {
            Some(zr) => {
                debug!("DS Response: {}", zr);
                zr
            }
            None => {
                debug!("No response from datastore");
                return reply_nxdomain(header.id);
            }
        },
        Err(error) => {
            error!("Failed to get response from datastore: {:?}", error);
            return reply_builder(header.id, Rcode::ServFail);
        }
    };

    let nscount = record
        .typerecords
        .iter()
        .filter(|r| r.is_type(RecordType::NS))
        .count() as u16;

    // this is our reply - static until that bit's done
    Ok(Reply {
        header: Header {
            id: header.id,
            qr: PacketType::Answer,
            opcode: header.opcode,
            authoritative: true,
            truncated: false, // TODO: work out if it's truncated (ie, UDP)
            recursion_desired: header.recursion_desired,
            recursion_available: false, // TODO: work this out
            z: false,
            // TODO: decide how the ad flag should be set
            ad: false,
            cd: false, // TODO: figure this out -  CD (checking disabled) bit in the query. This requests the server to not perform DNSSEC validation of responses.
            rcode: Rcode::NoError,
            qdcount: 1,
            ancount: record.typerecords.len() as u16,
            nscount,
            arcount: 0,
        },
        question: Some(question),
        answers: record.typerecords,
        authorities: vec![], // TODO: we're authoritative, we should respond with our records!
        additional: vec![],
    })
}

#[derive(Debug)]
pub struct Servers {
    pub datastore: Option<JoinHandle<Result<(), String>>>,
    pub udpserver: Option<JoinHandle<Result<(), Error>>>,
    pub tcpserver: Option<JoinHandle<Result<(), Error>>>,
    pub apiserver: Option<JoinHandle<Result<(), Error>>>,
    pub agent_tx: broadcast::Sender<AgentState>,
}

impl Default for Servers {
    fn default() -> Self {
        let (agent_tx, _) = broadcast::channel(10000);
        Self {
            datastore: None,
            udpserver: None,
            tcpserver: None,
            apiserver: None,
            agent_tx,
        }
    }
}

impl Servers {
    pub fn build(agent_tx: broadcast::Sender<AgentState>) -> Self {
        Self {
            agent_tx,
            ..Default::default()
        }
    }
    pub fn with_apiserver(self, apiserver: JoinHandle<Result<(), Error>>) -> Self {
        Self {
            apiserver: Some(apiserver),
            ..self
        }
    }
    pub fn with_datastore(self, datastore: JoinHandle<Result<(), String>>) -> Self {
        Self {
            datastore: Some(datastore),
            ..self
        }
    }
    pub fn with_tcpserver(self, tcpserver: JoinHandle<Result<(), Error>>) -> Self {
        Self {
            tcpserver: Some(tcpserver),
            ..self
        }
    }
    pub fn with_udpserver(self, udpserver: JoinHandle<Result<(), Error>>) -> Self {
        Self {
            udpserver: Some(udpserver),
            ..self
        }
    }

    fn send_shutdown(&self, agent: Agent) {
        info!("{agent:?} shut down");
        if let Err(error) = self.agent_tx.send(AgentState::Stopped { agent }) {
            eprintln!("Failed to send agent shutdown message: {error:?}");
        };
    }

    pub fn all_finished(&self) -> bool {
        let mut results = vec![];
        if let Some(server) = &self.apiserver {
            if server.is_finished() {
                println!("Sending API Shutdown");
                self.send_shutdown(Agent::API);
            }
            results.push(server.is_finished())
        }
        if let Some(server) = &self.datastore {
            if server.is_finished() {
                println!("Sending Datastore Shutdown");
                self.send_shutdown(Agent::Datastore);
            }
            results.push(server.is_finished())
        }
        if let Some(server) = &self.tcpserver {
            if server.is_finished() {
                println!("Sending TCP Server Shutdown");
                self.send_shutdown(Agent::TCPServer);
            }
            results.push(server.is_finished())
        }
        if let Some(server) = &self.udpserver {
            if server.is_finished() {
                println!("Sending UDP Server Shutdown");
                self.send_shutdown(Agent::UDPServer);
            }
            results.push(server.is_finished())
        }
        results.iter().any(|&r| r)
    }
}
