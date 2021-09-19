use std::{
    io::{self, Read, Write},
    mem::{size_of, size_of_val},
    net::TcpStream,
    result,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::sleep,
    time::Duration,
};

use bytes::{Buf, BufMut};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum ItemValue {
    Len(u64),
    Uint(u64),
    Int(i64),
    String(String),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    values: Vec<ItemValue>,
}

impl Message {
    pub fn new(values: Vec<ItemValue>) -> Self {
        Self { values }
    }

    pub fn values(&self) -> &Vec<ItemValue> {
        &self.values
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemFormat {
    Len { len: usize },
    Uint { len: usize },
    Int { len: usize },
    FixedString { len: usize },
    VarString { len_idx: usize },
    FixedBytes { len: usize },
    VarBytes { len_idx: usize },
}

enum ReadError {
    Io(std::io::Error),
    Eof,
    Stopped,
    FromUtf8(std::string::FromUtf8Error),
}

type ReadResult = result::Result<ItemValue, ReadError>;

enum WriteError {
    LenTooLarge {
        max_len: usize,
        len: usize,
    },
    ValueLenOutOfBound {
        specified_len: usize,
        item_len: usize,
    },
}

impl WriteError {
    fn global_error(&self, item_idx: usize) -> Error {
        match self {
            Self::LenTooLarge { max_len, len } => Error::LenTooLarge {
                max_len: *max_len,
                item_idx,
                len: *len,
            },
            Self::ValueLenOutOfBound {
                specified_len,
                item_len,
            } => Error::ValueLenOutOfBound {
                specified_len: *specified_len,
                item_idx,
                item_len: *item_len,
            },
        }
    }
}

type WriteResult = result::Result<(), WriteError>;

impl ItemFormat {
    fn err(&self, idx: usize, fmts: &[ItemFormat]) -> Option<Error> {
        let mut max_len = usize::MAX;
        match self {
            Self::Len { .. } | Self::Uint { .. } => max_len = size_of::<u64>(),
            Self::Int { .. } => max_len = size_of::<u64>(),
            _ => {}
        }

        match self {
            // Validate the length.
            Self::Len { len }
            | Self::Uint { len }
            | Self::Int { len }
            | Self::FixedString { len }
            | Self::FixedBytes { len } => {
                if *len > max_len {
                    return Some(Error::LenTooLarge {
                        max_len,
                        item_idx: idx,
                        len: *len,
                    });
                }
            }

            // Validate the index of length.
            Self::VarString { len_idx } | Self::VarBytes { len_idx } => {
                if *len_idx > idx {
                    return Some(Error::LenIdxTooLarge {
                        item_idx: idx,
                        len_idx: *len_idx,
                    });
                } else if let Self::Len { .. } = fmts[*len_idx] {
                } else {
                    return Some(Error::NotALen {
                        item_idx: idx,
                        len_idx: *len_idx,
                    });
                }
            }
        }
        None
    }

    fn len_by_idx(len_idx: usize, values: &[ItemValue]) -> usize {
        if let Some(value) = values.get(len_idx) {
            match value {
                ItemValue::Len(v) => *v as usize,
                _ => panic!(),
            }
        } else {
            panic!()
        }
    }

    fn len(&self, idx: usize, fmts: &[ItemFormat], values: &[ItemValue]) -> Result<usize> {
        if let Some(e) = self.err(idx, fmts) {
            return Err(e);
        }

        Ok(match self {
            Self::Len { len } => *len,
            Self::Uint { len } => *len,
            Self::Int { len } => *len,
            Self::FixedString { len } => *len,
            Self::VarString { len_idx } => Self::len_by_idx(*len_idx, values),
            Self::FixedBytes { len } => *len,
            Self::VarBytes { len_idx } => Self::len_by_idx(*len_idx, values),
        })
    }

    fn read_from_buf(&self, len: usize, buf: &mut &[u8]) -> ReadResult {
        if buf.len() < len {
            return Err(ReadError::Eof);
        }

        match self {
            Self::Len { .. } => Ok(ItemValue::Len(buf.get_uint(len))),
            Self::Uint { .. } => Ok(ItemValue::Uint(buf.get_uint(len))),
            Self::Int { .. } => Ok(ItemValue::Int(buf.get_int(len))),
            Self::FixedString { .. } | Self::VarString { .. } => {
                let mut str_buf = vec![0u8; len];
                buf.read_exact(&mut str_buf).unwrap();
                match String::from_utf8(str_buf) {
                    Ok(s) => Ok(ItemValue::String(s)),
                    Err(e) => Err(ReadError::FromUtf8(e)),
                }
            }
            Self::FixedBytes { .. } | Self::VarBytes { .. } => {
                let mut bytes_buf = vec![0u8; len];
                buf.read_exact(&mut bytes_buf).unwrap();
                Ok(ItemValue::Bytes(bytes_buf))
            }
        }
    }

    fn write_to_buf(&self, len: usize, value: &ItemValue, buf: &mut &mut [u8]) -> WriteResult {
        // Validate the length
        let mut max_len = usize::MAX;
        let mut min_len = 0usize;
        match value {
            ItemValue::Len(v) | ItemValue::Uint(v) => max_len = size_of_val(v),
            ItemValue::Int(v) => max_len = size_of_val(v),

            ItemValue::String(s) => min_len = s.len(),
            ItemValue::Bytes(bytes) => min_len = bytes.len(),
        }

        if len > max_len {
            return Err(WriteError::LenTooLarge { max_len, len });
        }
        if len < min_len {
            return Err(WriteError::ValueLenOutOfBound {
                specified_len: len,
                item_len: min_len,
            });
        }

        // Write value to buf.
        match (self, value) {
            (Self::Len { .. }, ItemValue::Len(v)) | (Self::Uint { .. }, ItemValue::Uint(v)) => {
                buf.put_uint(*v, len)
            }
            (Self::Int { .. }, ItemValue::Int(v)) => buf.put_int(*v, len),
            (Self::FixedString { .. } | Self::VarString { .. }, ItemValue::String(char_buf)) => {
                buf.put(char_buf.as_bytes())
            }
            (Self::FixedBytes { .. } | Self::VarBytes { .. }, ItemValue::Bytes(bytes_buf)) => {
                buf.put(bytes_buf.as_slice())
            }
            _ => panic!(),
        }

        Ok(())
    }

    fn read_from_tcp_stream(
        &self,
        len: usize,
        stream: &mut TcpStream,
        stop_flag: Arc<AtomicBool>,
    ) -> ReadResult {
        let mut integer_buf = [0u8; size_of::<u64>()];
        let mut integer_slice = &mut integer_buf[size_of::<u64>().max(len) - len..];

        let mut bytes_buf = vec![0u8; len];

        while let Err(e) = match self {
            Self::Len { .. } | Self::Uint { .. } | Self::Int { .. } => {
                stream.read_exact(&mut integer_slice)
            }
            Self::FixedString { .. }
            | Self::VarString { .. }
            | Self::FixedBytes { .. }
            | Self::VarBytes { .. } => stream.read_exact(&mut bytes_buf),
        } {
            match e.kind() {
                io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset => {
                    return Err(ReadError::Eof)
                }
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => {
                    if stop_flag.load(Ordering::Relaxed) {
                        return Err(ReadError::Stopped);
                    }
                    sleep(Duration::from_millis(300))
                }
                _ => return Err(ReadError::Io(e)),
            };
        }

        match self {
            Self::Len { .. } => Ok(ItemValue::Len(u64::from_be_bytes(integer_buf))),
            Self::Uint { .. } => Ok(ItemValue::Uint(u64::from_be_bytes(integer_buf))),
            Self::Int { .. } => {
                let mut v = i64::from_be_bytes(integer_buf);

                let offset = (size_of::<u64>() - len) * u8::BITS as usize;
                v = v << offset >> offset;
                Ok(ItemValue::Int(v))
            }
            Self::FixedString { .. } | Self::VarString { .. } => {
                match String::from_utf8(bytes_buf) {
                    Ok(s) => Ok(ItemValue::String(s)),
                    Err(e) => Err(ReadError::FromUtf8(e)),
                }
            }
            Self::FixedBytes { .. } | Self::VarBytes { .. } => Ok(ItemValue::Bytes(bytes_buf)),
        }
    }
}

#[derive(Clone)]
pub struct MessageFormat {
    item_fmts: Vec<ItemFormat>,
}

impl MessageFormat {
    pub fn new(item_fmts: Vec<ItemFormat>) -> Self {
        MessageFormat { item_fmts }
    }

    pub fn is_empty(&self) -> bool {
        self.item_fmts.is_empty()
    }

    pub fn err(&self) -> Option<Error> {
        if self.item_fmts.is_empty() {
            return Some(Error::MessageFormatEmpty);
        }

        for (idx, item_fmt) in self.item_fmts.iter().enumerate() {
            if let Some(e) = item_fmt.err(idx, &self.item_fmts) {
                return Some(e);
            }
        }
        None
    }

    pub fn decode(&self, buf: &[u8]) -> Result<Message> {
        let mut values = Vec::<ItemValue>::with_capacity(self.item_fmts.len());
        let mut slice = buf;
        for (idx, item_fmt) in self.item_fmts.iter().enumerate() {
            let len = item_fmt.len(idx, &self.item_fmts, &values)?;
            values.push(
                item_fmt
                    .read_from_buf(len, &mut slice)
                    .map_err(|e| match e {
                        ReadError::Eof => Error::Eof,
                        ReadError::FromUtf8(e) => Error::FromUtf8 { item_idx: idx, e },
                        _ => panic!(),
                    })?,
            );
        }

        Ok(Message { values })
    }

    pub fn encode(&self, msg: &Message) -> Result<Vec<u8>> {
        let mut buf = Vec::<u8>::default();
        let mut buf_len = 0;
        for (idx, (item_fmt, value)) in self.item_fmts.iter().zip(msg.values.iter()).enumerate() {
            let value_len = item_fmt.len(idx, &self.item_fmts, &msg.values)?;
            buf.resize(buf_len + value_len, 0);
            let mut slice = &mut buf[buf_len..buf_len + value_len];
            item_fmt
                .write_to_buf(value_len, value, &mut slice)
                .map_err(|e| e.global_error(idx))?;
            buf_len += value_len;
        }

        Ok(buf)
    }

    pub fn read_from(&self, stream: &mut TcpStream, stop_flag: Arc<AtomicBool>) -> Result<Message> {
        let mut values = Vec::<ItemValue>::with_capacity(self.item_fmts.len());
        for (idx, item_fmt) in self.item_fmts.iter().enumerate() {
            let len = item_fmt.len(idx, &self.item_fmts, &values)?;
            match item_fmt.read_from_tcp_stream(len, stream, stop_flag.clone()) {
                Ok(v) => values.push(v),
                Err(ReadError::Io(e)) => return Err(Error::Io(e)),
                Err(ReadError::Eof) => return Err(Error::Eof),
                Err(ReadError::FromUtf8(e)) => return Err(Error::FromUtf8 { item_idx: idx, e }),
                Err(ReadError::Stopped) => return Err(Error::Stopped),
            }
        }
        Ok(Message { values })
    }

    pub fn write_to(&self, msg: &Message, stream: &mut TcpStream) -> Result<()> {
        let bytes = self.encode(msg)?;
        stream.write_all(&bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::msg::{ItemFormat, ItemValue, Message, MessageFormat};

    #[test]
    fn encode_and_decode_ok() {
        let fmt = MessageFormat::new(vec![
            ItemFormat::Len { len: 2 },
            ItemFormat::Uint { len: 2 },
            ItemFormat::Int { len: 1 },
            ItemFormat::FixedString { len: 8 },
            ItemFormat::VarString { len_idx: 0 },
        ]);

        let msg = Message::new(vec![
            ItemValue::Len(16),
            ItemValue::Uint(2333),
            ItemValue::Int(127),
            ItemValue::String("aaaabbbb".to_string()),
            ItemValue::String("aaaabbbbccccdddd".to_string()),
        ]);

        let bytes = fmt.encode(&msg);

        assert!(bytes.is_ok());

        let decoded_msg = fmt.decode(bytes.as_ref().unwrap());

        assert!(decoded_msg.is_ok());

        assert_eq!(msg, decoded_msg.unwrap());
    }
}
