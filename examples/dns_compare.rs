//! Yeah, don't use this with anything but ipv4 and UDP for now.
//!
//! This is a simple packet sniffer that listens for DNS packets on a specified port.
//!

use clap::*;
use goatns::Question;
use packed_struct::prelude::*;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    process::exit,
};

const UDP_HEADER_LENGTH: usize = 12;

#[derive(Debug, PackedStruct, PartialEq, Eq, Clone, Hash)]
#[packed_struct(bit_numbering = "msb0")]
struct PacketHeader {
    #[packed_field(bits = "0..4", endian = "msb")]
    version: u8,

    #[packed_field(bits = "4..8", endian = "msb")]
    header_length: u8,
    #[packed_field(byte = "1", endian = "msb")]
    dscp_field: u8,
    #[packed_field(bytes = "2..=3", endian = "msb")]
    length: u16,
    #[packed_field(bytes = "4..=5", endian = "msb")]
    identification: u16,
    #[packed_field(bits = "56..59", endian = "msb")]
    flags: [bool; 3],
    #[packed_field(bytes = "8", endian = "msb")]
    ttl: u8,
    #[packed_field(bytes = "9", endian = "msb")]
    protocol: u8,
    #[packed_field(bytes = "10..=11", endian = "msb")]
    checksum: u16,
}

impl PacketHeader {
    fn header_length(&self) -> usize {
        self.header_length as usize * 4
    }

    fn ip_data<'a>(&self, data: &'a [u8]) -> &'a [u8] {
        &data[12..=self.header_length() - 1]
    }

    fn payload<'a>(&self, data: &'a [u8]) -> &'a [u8] {
        // this differs for TCP when it's the return data?
        if self.protocol == 17 {
            &data[self.header_length() + 8..]
        } else {
            // panic!("Can't handle returning the payload yet!")
            &data[self.header_length()..]
        }
    }
}

#[derive(Debug, PackedStruct, PartialEq, Eq, Clone, Hash)]
#[packed_struct(bit_numbering = "msb0")]
struct Ipv4Header {
    #[packed_field(endian = "msb")]
    source: u32,
    #[packed_field(endian = "msb")]
    dest: u32,
}

impl Ipv4Header {
    pub fn source_addr(&self) -> IpAddr {
        IpAddr::V4(Ipv4Addr::from_bits(self.source))
    }
    pub fn dest_addr(&self) -> IpAddr {
        IpAddr::V4(Ipv4Addr::from_bits(self.dest))
    }
}

#[derive(Debug, PackedStruct, PartialEq, Eq, Clone, Hash)]
#[packed_struct(bit_numbering = "msb0")]
struct Ipv6Header {
    #[packed_field(endian = "msb")]
    source: [u8; 16],
    #[packed_field(endian = "msb")]
    dest: [u8; 16],
}

impl Ipv6Header {
    #[allow(dead_code)]
    pub fn source_addr(&self) -> IpAddr {
        IpAddr::V6(Ipv6Addr::from_bits(u128::from_be_bytes(self.source)))
    }
    #[allow(dead_code)]
    pub fn dest_addr(&self) -> IpAddr {
        IpAddr::V6(Ipv6Addr::from_bits(u128::from_be_bytes(self.dest)))
    }
}

#[derive(Debug, Parser)]
struct Cli {
    port: u16,
}

#[allow(dead_code)]
#[derive(Debug)]
struct IpData {
    source: IpAddr,
    dest: IpAddr,
}

#[derive(Debug, PackedStruct)]
#[packed_struct(bit_numbering = "msb0")]
struct UdpPacket {
    #[packed_field(bytes = "0..=1", endian = "msb")]
    source_port: u16,
    #[packed_field(bytes = "2..=3", endian = "msb")]
    dest_port: u16,
    #[packed_field(bytes = "4..=5", endian = "msb")]
    length: u16,
    #[packed_field(bytes = "6..=7", endian = "msb")]
    checksum: u16,
}

const TCP_PACKET_HEADER: usize = 26;

#[derive(Debug, PackedStruct)]
#[packed_struct(bit_numbering = "msb0")]
struct TcpPacket {
    #[packed_field(bytes = "0..", endian = "msb")]
    source_port: u16,
    #[packed_field(bytes = "4..", endian = "msb")]
    dest_port: u16,
    #[packed_field(bytes = "8..", endian = "msb")]
    sequence_number: u32,
    #[packed_field(bytes = "16..", endian = "msb")]
    ack_number: u32,

    #[packed_field(bits = "160..164", endian = "msb")]
    header_length: u8,
    #[packed_field(bits = "164..", endian = "msb")]
    flags: [bool; 12],
    #[packed_field(bits = "176..", endian = "msb")]
    window: u16,
    #[packed_field(bits = "192..", endian = "msb")]
    checksum: u16,
}

impl TcpPacket {
    fn has_data(&self) -> bool {
        self.flags[2]
    }
}

fn hexdump(data: &[u8]) {
    for chunk in data.chunks(16) {
        for byte in chunk {
            print!("{:02x} ", byte);
        }
        for _ in chunk.len()..16 {
            print!("   ");
        }
        print!("  ");
        for byte in chunk {
            print!(
                "{}",
                if byte.is_ascii_graphic() {
                    *byte as char
                } else {
                    '.'
                }
            );
        }
        println!("");
    }
}

#[tokio::main]
pub async fn main() {
    let cli = Cli::parse();

    // let mut seen_headers = HashSet::new();
    // listen on the device named "any", which is only available on Linux. This is only for
    // demonstration purposes.
    let mut cap = pcap::Capture::from_device("any")
        .expect("Failed to open capture handle on device")
        .immediate_mode(true)
        .open()
        .expect("Failed to open device");

    // filter for DNS packets on the specified port.
    let filter = format!("port {}", cli.port);
    if let Err(err) = cap.filter(&filter, true) {
        eprintln!("Failed to set filter '{}', quitting: {}", filter, err);
        exit(1)
    };
    eprintln!("Watching for packets with the filter: '{}'", filter);

    while let Ok(packet) = cap.next_packet() {
        let header_slice: [u8; UDP_HEADER_LENGTH] = packet.data[0..UDP_HEADER_LENGTH]
            .try_into()
            .expect("slice with incorrect length");

        let header = PacketHeader::unpack(&header_slice).expect("Failed to parse packet header");
        // println!("Header: {:x?}", header);

        if packet.data.len() != header.length as usize {
            eprintln!(
                "Wrong-sized packet: {} != {}",
                packet.data.len(),
                header.length
            );
        }
        // println!("Header length: {}", header.header_length());

        let (ip_data, remaining_packets) = match header.version {
            4 => {
                let mut slice: [u8; 8] = [0; 8];
                slice.copy_from_slice(header.ip_data(packet.data));
                let ipv4packet = Ipv4Header::unpack(&slice).expect("Failed to parse DNS packet");
                (
                    IpData {
                        source: ipv4packet.source_addr(),
                        dest: ipv4packet.dest_addr(),
                    },
                    header.payload(packet.data),
                )
            }
            // 6 => {
            //     let mut slice: [u8; 32] = [0; 32];
            //     slice.copy_from_slice(&header.ip_data(&packet.data));
            //     let ipv6packet = Ipv6Header::unpack(&slice).expect("Failed to parse DNS packet");
            //     IpData {
            //         source: ipv6packet.source_addr(),
            //         dest: ipv6packet.dest_addr(),
            //     }
            // }
            _ => panic!("Unsupported IP version: {}", header.version),
        };

        let (source_port, dest_port) = match header.protocol {
            17 => {
                let mut udp_slice: [u8; 8] = [0; 8];
                udp_slice.copy_from_slice(
                    &packet.data[header.header_length()..header.header_length() + 8],
                );
                let udp_packet = UdpPacket::unpack(&udp_slice).expect("Failed to parse UDP packet");

                // println!("UDP Packet: {:?}", udp_packet);
                (udp_packet.source_port, udp_packet.dest_port)
            }
            6 => {
                let mut tcp_slice: [u8; TCP_PACKET_HEADER] = [0; TCP_PACKET_HEADER];
                tcp_slice.copy_from_slice(
                    &packet.data
                        [header.header_length()..header.header_length() + TCP_PACKET_HEADER],
                );
                let tcp_packet = TcpPacket::unpack(&tcp_slice).expect("Failed to parse TCP packet");
                if !tcp_packet.has_data() {
                    // eprintln!("No data in TCP packet");
                    continue;
                }
                println!("TCP Packet: {:?}", tcp_packet);
                println!("{:x?}", &tcp_slice);

                (tcp_packet.source_port, tcp_packet.dest_port)
            }
            _ => {
                eprintln!("Unsupported protocol: {}", header.protocol);
                continue;
            }
        };

        println!(
            "{}:{} -> {}:{} ({} bytes)",
            ip_data.source,
            source_port,
            ip_data.dest,
            dest_port,
            packet.data.len()
        );

        // println!("remaining bytes of packet {:x?}", remaining_packets);

        let dns_header: &[u8; 12] = remaining_packets[0..12]
            .try_into()
            .expect("slice with incorrect length");
        let dns_header = match goatns::Header::unpack(dns_header) {
            Ok(val) => val,
            Err(err) => {
                eprintln!("Failed to parse DNS header: {:?}", err);
                continue;
            }
        };
        println!("DNS Header: {:?}", dns_header);

        let body = &remaining_packets[12..];

        if let Ok((question, byte_offset)) = Question::from_packets_with_offset(body) {
            println!("Question: {:?}", question);
            println!("Remaining bytes: {}", body.len() - byte_offset);
            if let Ok(name) = question.normalized_name() {
                println!("Question: {:?}", name);
            } else {
                eprintln!("Failed to normalize name");
            }
            println!("Dumping remainder of packet");
            hexdump(&body[byte_offset..]);
        } else {
            eprintln!("Failed to parse question header");
        }
        println!("\nDumping whole packet");
        hexdump(packet.data);
        // TODO: dig around and parse the response betterer

        println!("####################");
    }
}
