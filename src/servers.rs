use log::*;
use std::net::SocketAddr;
use std::str::{from_utf8, FromStr};
use std::time::Duration;
use tokio::io::{self, AsyncReadExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::timeout;
// use packed_struct::PackedStruct;
use crate::config::ConfigFile;
use crate::enums::RecordClass;
use crate::Reply;
use crate::{parse_udp_query, REPLY_TIMEOUT_MS, UDP_BUFFER_SIZE};

lazy_static! {
    static ref LOCALHOST: std::net::IpAddr = std::net::IpAddr::from_str("127.0.0.1").unwrap();
}

async fn check_for_shutdown(r: &Reply, addr: &SocketAddr, config: &ConfigFile) -> Result<(), ()> {
    // when you get a CHAOS from localhost with "shutdown" break dat loop
    if let Some(q) = &r.question {
        if q.qclass == RecordClass::Chaos {
            if let Ok(qname) = from_utf8(&q.qname) {
                // TODO: this needs some kind of password or auth, because UDP is weird. Probably should only support this on TCP. But we don't do TCP properly yet so ... yolo? Or just .. not do this on UDP!
                if (qname == "shutdown") & (config.shutdown_ip_allow_list.contains(&addr.ip())) {
                    info!("Got CHAOS shutdown from {:?}, shutting down", addr.ip());
                    return Ok(());
                } else {
                    log::warn!("Got CHAOS shutdown from {:?}, ignoring!", addr.ip());
                }
            }
        }
    };
    Err(())
}

pub async fn udp_server(
    bind_address: SocketAddr,
    config: ConfigFile,
    datastore: mpsc::Sender<crate::datastore::Command>,
) -> io::Result<()> {
    let udp_sock = match UdpSocket::bind(bind_address).await {
        Ok(value) => {
            info!("Started UDP listener on {}:{}", config.address, config.port);
            value
        }
        Err(error) => {
            error!("Failed to start UDP listener: {:?}", error);
            return Ok(());
        }
    };

    let mut udp_buffer = [0; UDP_BUFFER_SIZE];

    loop {
        let (len, addr) = match udp_sock.recv_from(&mut udp_buffer).await {
            Ok(value) => value,
            Err(error) => panic!("{:?}", error),
        };
        debug!("{:?} bytes received from {:?}", len, addr);

        let udp_result = match timeout(
            Duration::from_millis(REPLY_TIMEOUT_MS),
            parse_udp_query(datastore.clone(), len, udp_buffer, config.capture_packets),
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
                debug!("Result: {:?}", r);

                // yeah not sure this is something we really want on the UDP side, since you can trick the server with UDP things
                // if check_for_shutdown(&r, &addr, &config).await.is_ok() {
                //     return Ok(());
                // };

                let reply_bytes: Vec<u8> = match r.as_bytes() {
                    Ok(value) => {
                        // Check if it's too long and set truncate flag if so, it's safe to unwrap since we've already gone
                        if value.len() > UDP_BUFFER_SIZE {
                            r = r.set_truncated();
                            r.as_bytes().unwrap_or(value)
                        } else {
                            value
                        }
                    }
                    Err(error) => {
                        error!("Failed to parse reply {:?} into bytes: {:?}", r, error);
                        continue;
                    }
                };

                debug!("reply_bytes: {:?}", reply_bytes);
                let len = match udp_sock.send_to(&reply_bytes as &[u8], addr).await {
                    Ok(value) => value,
                    Err(err) => {
                        error!("Failed to send data back to {:?}: {:?}", addr, err);
                        return Ok(());
                    }
                };
                // let len = sock.send_to(r.answer.as_bytes(), addr).await?;
                debug!("{:?} bytes sent", len);
            }
            Err(error) => error!("Error: {}", error),
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
) -> io::Result<()> {
    // TODO: add a configurable idle timeout for the TCP server
    let tcpserver = match TcpListener::bind(bind_address).await {
        Ok(value) => {
            info!("Started TCP listener on {}", bind_address);
            value
        }
        Err(error) => {
            error!("Failed to start TCP Server: {:?}", error);
            return Ok(());
        }
    };

    loop {
        let (mut stream, addr) = match tcpserver.accept().await {
            Ok(value) => value,
            Err(error) => panic!("Couldn't get data from TcpStrream: {:?}", error),
        };
        debug!("TCP connection from {:?}", addr);

        let (mut reader, writer) = stream.split();
        // TODO: this is a hilariously risky unwrap
        let msg_length: usize = reader.read_u16().await.unwrap().into();
        debug!("msg_length={msg_length}");
        // let mut buf: Vec<u8> = Vec::with_capacity(msg_length.into());
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

        crate::utils::hexdump(buf.clone());
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
            crate::parse_tcp_query(tx.clone(), msg_length, buf, config.capture_packets),
        )
        .await
        {
            Ok(reply) => reply,
            Err(_) => {
                error!("Did not receive response from parse_query within {REPLY_TIMEOUT_MS} ms");
                continue;
            }
        };

        match result {
            Ok(mut r) => {
                debug!("TCP Result: {r:?}");

                // when you get a CHAOS from localhost with "shutdown" break dat loop
                if check_for_shutdown(&r, &addr, &config).await.is_ok() {
                    return Ok(());
                }

                let reply_bytes: Vec<u8> = match r.as_bytes() {
                    Ok(value) => value,
                    Err(error) => {
                        error!("Failed to parse reply {:?} into bytes: {:?}", r, error);
                        continue;
                    }
                };
                debug!("reply_bytes: {:?}", reply_bytes);

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
                debug!("{:?} bytes sent", len);

                // send the data
                let len = match writer.try_write(reply_bytes) {
                    Ok(value) => value,
                    Err(err) => {
                        error!("Failed to send data back to {:?}: {:?}", addr, err);
                        return Ok(());
                    }
                };
                debug!("{:?} bytes sent", len);
            }
            Err(error) => error!("Error: {}", error),
        }
    }
}
