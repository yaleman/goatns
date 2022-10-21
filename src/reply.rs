use crate::resourcerecord::InternalResourceRecord;
use crate::{ResourceRecord, UDP_BUFFER_SIZE};
use crate::{Header, Question};
use log::debug;
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
    pub async fn as_bytes(&mut self) -> Result<Vec<u8>, String> {
        // TODO it feels like a bug to edit the reply as we're outputting it as bytes?
        let mut retval: Vec<u8> = vec![];

        self.header.ancount = self.answers.len() as u16;

        // use the packed_struct to build the bytes
        let reply_header = match self.header.pack() {
            Ok(value) => value,
            // TODO: this should not be a panic
            Err(err) => return Err(format!("Failed to pack reply header bytes: {:?}", err)),
        };
        retval.extend(reply_header);

        // need to add the question in here
        if let Some(question) = &self.question {
            retval.extend(question.to_bytes());

            for answer in &self.answers {
                let ttl: &u32 = match answer {
                    InternalResourceRecord::A { address: _, ttl } => ttl,
                    InternalResourceRecord::NAPTR {
                        ttl,
                        domain: _,
                        order: _,
                        preference: _,
                        flags: _,
                    } => ttl,
                    InternalResourceRecord::NS { nsdname: _, ttl } => ttl,
                    InternalResourceRecord::MD { ttl } => ttl,
                    InternalResourceRecord::MF { ttl } => ttl,
                    InternalResourceRecord::CNAME { cname: _, ttl } => ttl,
                    InternalResourceRecord::SOA {
                        zone: _,
                        mname: _,
                        rname: _,
                        serial: _,
                        refresh: _,
                        retry: _,
                        expire: _,
                        minimum,
                    } => minimum,
                    InternalResourceRecord::MB { ttl } => ttl,
                    InternalResourceRecord::MG { ttl } => ttl,
                    InternalResourceRecord::MR { ttl } => ttl,
                    InternalResourceRecord::NULL { ttl } => ttl,
                    InternalResourceRecord::WKS { ttl } => ttl,
                    InternalResourceRecord::PTR { ptrdname: _, ttl } => ttl,
                    InternalResourceRecord::HINFO { cpu: _, os: _, ttl } => ttl,
                    InternalResourceRecord::MINFO { ttl } => ttl,
                    InternalResourceRecord::MX {
                        preference: _,
                        exchange: _,
                        ttl,
                    } => ttl,
                    InternalResourceRecord::TXT { txtdata: _, ttl } => ttl,
                    InternalResourceRecord::AAAA { address: _, ttl } => ttl,
                    InternalResourceRecord::AXFR { ttl } => ttl,
                    InternalResourceRecord::MAILB { ttl } => ttl,
                    InternalResourceRecord::ALL {} => &1u32,
                    InternalResourceRecord::InvalidType => &1u32,
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

        for authority in &self.authorities {
            debug!("Authority: {:?}", authority);
        }

        for additional in &self.additional {
            debug!("Authority: {:?}", additional);
        }

        Ok(retval)
    }

    /// because sometimes you need to trunc that junk
    pub async fn as_bytes_udp(&mut self) -> Result<Vec<u8>, String> {
        let mut result = self.as_bytes().await?;
        if result.len() > UDP_BUFFER_SIZE {
            result.truncate(UDP_BUFFER_SIZE);
        };
        Ok(result)
    }


    /// checks to see if it's over the max length set in [UDP_BUFFER_SIZE] and set the truncated flag if it is
    pub async fn check_set_truncated(&mut self) -> Self {
        if let Ok(ret_bytes) = self.as_bytes().await {
            if ret_bytes.len() > UDP_BUFFER_SIZE {

                let mut header = self.header.clone();
                header.truncated = true;
                return Self { header, ..self.clone() }
            }
        }
        self.clone().to_owned()
    }

}
