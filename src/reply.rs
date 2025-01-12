use crate::enums::{PacketType, Rcode};
use crate::error::GoatNsError;
use crate::resourcerecord::{DNSCharString, InternalResourceRecord};
use crate::{Header, Question};
use crate::{ResourceRecord, UDP_BUFFER_SIZE};
use packed_struct::prelude::*;
use tracing::error;

#[derive(Debug, Clone)]
pub struct Reply {
    pub header: Header,
    pub question: Option<Question>,
    pub answers: Vec<InternalResourceRecord>,
    pub authorities: Vec<ResourceRecord>,
    pub additional: Vec<ResourceRecord>,
}

impl Reply {
    /// This is used to turn into a series of bytes to yeet back to the client, needs to take a mutable self because the answers record length goes into the header
    pub async fn as_bytes(&self) -> Result<Vec<u8>, GoatNsError> {
        let mut retval: Vec<u8> = vec![];

        // so we can set the headers
        let mut final_reply = self.clone();
        final_reply.header.ancount = final_reply.answers.len() as u16;
        // use the packed_struct to build the bytes
        let reply_header = final_reply.header.pack()?;
        retval.extend(reply_header);

        // need to add the question in here
        if let Some(question) = &final_reply.question {
            retval.extend(question.try_to_bytes()?);

            for answer in &final_reply.answers {
                let ttl: &u32 = match answer {
                    InternalResourceRecord::A { ttl, .. } => ttl,
                    InternalResourceRecord::AAAA { ttl, .. } => ttl,
                    InternalResourceRecord::AXFR { ttl, .. } => ttl,
                    InternalResourceRecord::CAA { ttl, .. } => ttl,
                    InternalResourceRecord::CNAME { ttl, .. } => ttl,
                    InternalResourceRecord::HINFO { ttl, .. } => ttl,
                    InternalResourceRecord::InvalidType => &1u32,
                    InternalResourceRecord::LOC { ttl, .. } => ttl,
                    InternalResourceRecord::MX { ttl, .. } => ttl,
                    InternalResourceRecord::NAPTR { ttl, .. } => ttl,
                    InternalResourceRecord::NS { ttl, .. } => ttl,
                    InternalResourceRecord::PTR { ttl, .. } => ttl,
                    InternalResourceRecord::SOA { minimum, .. } => minimum,
                    InternalResourceRecord::TXT { ttl, .. } => ttl,
                    InternalResourceRecord::URI { ttl, .. } => ttl,
                };

                let answer_record = ResourceRecord {
                    name: question.qname.clone(),
                    record_type: answer.to_owned().into(),
                    class: question.qclass,
                    ttl: *ttl,
                    rdata: answer.as_bytes(&question.qname)?,
                };
                let reply_bytes: Vec<u8> = answer_record.try_into()?;
                retval.extend(reply_bytes);
            }
        }

        for authority in &final_reply.authorities {
            error!(
                "Should be handling authority rr's in reply: {:?}",
                authority
            );
        }

        for additional in &final_reply.additional {
            error!(
                "Should be handling additional rr's in reply: {:?}",
                additional
            );
        }

        Ok(retval)
    }

    /// because sometimes you need to trunc that junk
    pub async fn as_bytes_udp(&self) -> Result<Vec<u8>, GoatNsError> {
        let mut result = self.as_bytes().await?;
        if result.len() > UDP_BUFFER_SIZE {
            result.truncate(UDP_BUFFER_SIZE);
        };
        Ok(result)
    }

    /// checks to see if it's over the max length set in [UDP_BUFFER_SIZE] and set the truncated flag if it is
    pub async fn check_set_truncated(&self) -> Reply {
        if let Ok(ret_bytes) = self.as_bytes().await {
            if ret_bytes.len() > UDP_BUFFER_SIZE {
                let mut header = self.header.clone();
                header.truncated = true;
                return Self {
                    header,
                    ..self.clone()
                };
            }
        }
        self.clone()
    }
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

/// Build a NXDOMAIN response
pub fn reply_nxdomain(id: u16) -> Result<Reply, String> {
    // RFC 2308  - 2.1 Name Error - <https://www.rfc-editor.org/rfc/rfc2308#section-2.1>
    reply_builder(id, Rcode::NameError)
}

/// Reply to an ANY request with a HINFO "RFC8482" "" response
pub fn reply_any(id: u16, question: &Question) -> Result<Reply, String> {
    Ok(Reply {
        header: Header {
            id,
            qr: PacketType::Answer,
            rcode: Rcode::NoError,
            authoritative: true,
            qdcount: 1,
            ancount: 1,
            ..Header::default()
        },
        question: Some(question.clone()),
        answers: vec![InternalResourceRecord::HINFO {
            cpu: Some(DNSCharString::from("RFC8482")),
            os: Some(DNSCharString::from("")),
            ttl: 3789,
            rclass: question.qclass,
        }],
        authorities: vec![],
        additional: vec![],
    })
}
