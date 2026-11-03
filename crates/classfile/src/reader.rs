use crate::{ErrorKind, ParseError};

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    base: usize,
    context: &'static str,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8], base: usize, context: &'static str) -> Self {
        Self {
            bytes,
            position: 0,
            base,
            context,
        }
    }

    pub fn offset(&self) -> usize {
        self.base + self.position
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    pub fn error(&self, kind: ErrorKind) -> ParseError {
        ParseError::new(self.offset(), self.context, kind)
    }
    pub fn invalid(&self, reason: &'static str) -> ParseError {
        self.error(ErrorKind::InvalidStructure(reason))
    }

    pub fn take(&mut self, count: usize) -> Result<&'a [u8], ParseError> {
        if count > self.remaining() {
            return Err(self.error(ErrorKind::UnexpectedEof {
                needed: count,
                remaining: self.remaining(),
            }));
        }
        let start = self.position;
        self.position += count;
        Ok(&self.bytes[start..self.position])
    }

    pub fn u1(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }
    pub fn u2(&mut self) -> Result<u16, ParseError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }
    pub fn u4(&mut self) -> Result<u32, ParseError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    pub fn finish(&self) -> Result<(), ParseError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(self.error(ErrorKind::TrailingBytes(self.remaining())))
        }
    }
}
