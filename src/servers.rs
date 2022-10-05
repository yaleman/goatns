use log::*;
use std::net::SocketAddr;
use std::str::from_utf8;
use std::time::Duration;
use tokio::io::{self, AsyncReadExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::timeout;
// use packed_struct::PackedStruct;
use crate::config::ConfigFile;
use crate::{parse_udp_query, REPLY_TIMEOUT_MS, UDP_BUFFER_SIZE};

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

                let reply_bytes: Vec<u8> = match r.as_bytes() {
                    Ok(value) => value,
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
/// Ref https://www.rfc-editor.org/rfc/rfc7766

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
        let mut buf: Vec<u8> = vec![];
        let len = match reader.read_to_end(&mut buf).await {
            Ok(size) => size,
            Err(error) => {
                error!("Failed to read from TCP Stream: {:?}", error);
                return Ok(());
            }
        };
        debug!("Read {:?} bytes from TCP stream", len);
        for b in buf.clone() {
            debug!("{:?}\t'{}'", &b, from_utf8(&[b]).unwrap_or("."));
        }
        let buf = &buf[0..len];
        let result = match timeout(
            Duration::from_millis(REPLY_TIMEOUT_MS),
            crate::parse_tcp_query(tx.clone(), len, buf, config.capture_packets),
        )
        .await
        {
            Ok(reply) => reply,
            Err(_) => {
                error!("Did not receive response from parse_query within 10 ms");
                continue;
            }
        };

        match result {
            Ok(mut r) => {
                debug!("Result: {:?}", r);
                let reply_bytes: Vec<u8> = match r.as_bytes() {
                    Ok(value) => value,
                    Err(error) => {
                        error!("Failed to parse reply {:?} into bytes: {:?}", r, error);
                        continue;
                    }
                };
                debug!("reply_bytes: {:?}", reply_bytes);
                // let len = match writer.send_to(&reply_bytes as &[u8], addr).await {
                let len = match writer.try_write(&reply_bytes as &[u8]) {
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
