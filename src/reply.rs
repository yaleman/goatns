use crate::resourcerecord::InternalResourceRecord;
use crate::{Header, Question};
use crate::{ResourceRecord, UDP_BUFFER_SIZE};
use log::error;
use packed_struct::prelude::*;

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
    pub async fn as_bytes(&self) -> Result<Vec<u8>, String> {
        let mut retval: Vec<u8> = vec![];

        // so we can set the headers
        let mut final_reply = self.clone();
        final_reply.header.ancount = final_reply.answers.len() as u16;
        // use the packed_struct to build the bytes
        let reply_header = match final_reply.header.pack() {
            Ok(value) => value,
            Err(err) => return Err(format!("Failed to pack reply header bytes: {:?}", err)),
        };
        retval.extend(reply_header);

        // need to add the question in here
        if let Some(question) = &final_reply.question {
            retval.extend(question.to_bytes());

            for answer in &final_reply.answers {
                let ttl: &u32 = match answer {
                    InternalResourceRecord::A { ttl, .. } => ttl,
                    InternalResourceRecord::AAAA { ttl, .. } => ttl,
                    InternalResourceRecord::ALL {} => &1u32,
                    InternalResourceRecord::AXFR { ttl, .. } => ttl,
                    InternalResourceRecord::CAA { ttl, .. } => ttl,
                    InternalResourceRecord::CNAME { ttl, .. } => ttl,
                    InternalResourceRecord::HINFO { ttl, .. } => ttl,
                    InternalResourceRecord::InvalidType => &1u32,
                    InternalResourceRecord::LOC { ttl, .. } => ttl,
                    InternalResourceRecord::MAILB { ttl, .. } => ttl,
                    InternalResourceRecord::MB { ttl, .. } => ttl,
                    InternalResourceRecord::MG { ttl, .. } => ttl,
                    InternalResourceRecord::MINFO { ttl, .. } => ttl,
                    InternalResourceRecord::MR { ttl, .. } => ttl,
                    InternalResourceRecord::MX { ttl, .. } => ttl,
                    InternalResourceRecord::NAPTR { ttl, .. } => ttl,
                    InternalResourceRecord::NS { ttl, .. } => ttl,
                    InternalResourceRecord::NULL { ttl, .. } => ttl,
                    InternalResourceRecord::PTR { ttl, .. } => ttl,
                    InternalResourceRecord::SOA { minimum, .. } => minimum,
                    InternalResourceRecord::TXT { ttl, .. } => ttl,
                    InternalResourceRecord::URI { ttl, .. } => ttl,
                    InternalResourceRecord::WKS { ttl, .. } => ttl,
                };

                let answer_record = ResourceRecord {
                    name: question.qname.clone(),
                    record_type: answer.to_owned().into(),
                    class: question.qclass,
                    ttl: *ttl,
                    rdata: answer.as_bytes(&question.qname),
                };
                let reply_bytes: Vec<u8> = answer_record.into();
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
    pub async fn as_bytes_udp(&self) -> Result<Vec<u8>, String> {
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
