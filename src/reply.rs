use crate::ResourceRecord;
use crate::{Header, Question};
use log::debug;
use packed_struct::prelude::*;

#[derive(Debug)]
pub struct Reply {
    pub header: Header,
    pub question: Option<Question>,
    pub answers: Vec<ResourceRecord>,
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
        }

        for answer in &self.answers {
            let reply_bytes: Vec<u8> = answer.into();
            retval.extend(reply_bytes);
        }

        for authority in &self.authorities {
            debug!("Authority: {:?}", authority);
        }

        for additional in &self.additional {
            debug!("Authority: {:?}", additional);
        }

        Ok(retval)
    }

    /// checks to see if it's over the max length set in [UDP_BUFFER_SIZE] and set the truncated flag if it is
    pub fn set_truncated(self) -> Self {
        let mut header = self.header;
        header.truncated = true;
        Self { header, ..self }
    }
}
