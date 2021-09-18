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

#[derive(Debug, Clone)]
pub enum ItemValue {
    Len(u64),
    Uint(u64),
    Int(i64),
    String(String),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone)]
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

enum LenError {
    LenIdxOutOfBound { len_idx: usize },
    NotALen { len_idx: usize },
}

impl LenError {
    fn global_error(&self, item_idx: usize) -> Error {
        match self {
            Self::LenIdxOutOfBound { len_idx } => Error::LenIdxOutOfBound {
                item_idx,
                len_idx: *len_idx,
            },
            Self::NotALen { len_idx } => Error::NotALen {
                item_idx,
                len_idx: *len_idx,
            },
        }
    }
}

type LenResult = result::Result<usize, LenError>;

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
    fn len(&self, values: &[ItemValue]) -> LenResult {
        match self {
            Self::Len { len } => Ok(*len),
            Self::Uint { len } => Ok(*len),
            Self::Int { len } => Ok(*len),
            Self::FixedString { len } => Ok(*len),
            Self::VarString { len_idx } => Self::len_by_idx(*len_idx, values),
            Self::FixedBytes { len } => Ok(*len),
            Self::VarBytes { len_idx } => Self::len_by_idx(*len_idx, values),
        }
    }

    fn read_from_buf(&self, len: usize, buf: &mut &[u8]) -> ReadResult {
        if buf.len() < len {
            return Err(ReadError::Eof);
        }

        match self {
            Self::Len { len: _ } => Ok(ItemValue::Len(buf.get_uint(len))),
            Self::Uint { len: _ } => Ok(ItemValue::Uint(buf.get_uint(len))),
            Self::Int { len: _ } => Ok(ItemValue::Int(buf.get_int(len))),
            Self::FixedString { len: _ } | Self::VarString { len_idx: _ } => {
                let mut str_buf = vec![0u8; len];
                buf.read_exact(&mut str_buf).unwrap();
                match String::from_utf8(str_buf) {
                    Ok(s) => Ok(ItemValue::String(s)),
                    Err(e) => Err(ReadError::FromUtf8(e)),
                }
            }
            Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ } => {
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
            (Self::Len { len: _ }, ItemValue::Len(v))
            | (Self::Uint { len: _ }, ItemValue::Uint(v)) => buf.put_uint(*v, len),
            (Self::Int { len: _ }, ItemValue::Int(v)) => buf.put_int(*v, len),
            (
                Self::FixedString { len: _ } | Self::VarString { len_idx: _ },
                ItemValue::String(char_buf),
            ) => buf.put(char_buf.as_bytes()),
            (
                Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ },
                ItemValue::Bytes(bytes_buf),
            ) => buf.put(bytes_buf.as_slice()),
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
            Self::Len { len: _ } | Self::Uint { len: _ } | Self::Int { len: _ } => {
                stream.read_exact(&mut integer_slice)
            }
            Self::FixedString { len: _ }
            | Self::VarString { len_idx: _ }
            | Self::FixedBytes { len: _ }
            | Self::VarBytes { len_idx: _ } => stream.read_exact(&mut bytes_buf),
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
            Self::Len { len: _ } => Ok(ItemValue::Len(u64::from_be_bytes(integer_buf))),
            Self::Uint { len: _ } => Ok(ItemValue::Uint(u64::from_be_bytes(integer_buf))),
            Self::Int { len: _ } => {
                let mut v = i64::from_be_bytes(integer_buf);

                let offset = (size_of::<u64>() - len) * u8::BITS as usize;
                v = v << offset >> offset;
                Ok(ItemValue::Int(v))
            }
            Self::FixedString { len: _ } | Self::VarString { len_idx: _ } => {
                match String::from_utf8(bytes_buf) {
                    Ok(s) => Ok(ItemValue::String(s)),
                    Err(e) => Err(ReadError::FromUtf8(e)),
                }
            }
            Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ } => {
                Ok(ItemValue::Bytes(bytes_buf))
            }
        }
    }

    fn len_by_idx(len_idx: usize, values: &[ItemValue]) -> LenResult {
        if let Some(value) = values.get(len_idx) {
            match value {
                ItemValue::Len(v) => Ok(*v as usize),
                _ => Err(LenError::NotALen { len_idx }),
            }
        } else {
            Err(LenError::LenIdxOutOfBound { len_idx })
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

    pub fn decode(&self, buf: &[u8]) -> Result<Message> {
        let mut values = Vec::<ItemValue>::with_capacity(self.item_fmts.len());
        let mut slice = buf;
        for (idx, item_fmt) in self.item_fmts.iter().enumerate() {
            let len = item_fmt.len(&values).map_err(|e| e.global_error(idx))?;
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
            let value_len = item_fmt
                .len(&msg.values)
                .map_err(|e| e.global_error(idx))?;
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
            let len = item_fmt.len(&values).map_err(|e| e.global_error(idx))?;
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
