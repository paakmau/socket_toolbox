use std::{
    io::{self, Read, Write},
    mem::{size_of, size_of_val},
    net::TcpStream,
    result,
    thread::sleep,
    time::Duration,
};

use bytes::{Buf, BufMut};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub enum DataValue {
    Len(u64),
    Uint(u64),
    Int(i64),
    String(String),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct Message {
    values: Vec<DataValue>,
}

impl Message {
    pub fn new(values: Vec<DataValue>) -> Self {
        Self { values }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataFormat {
    Len { len: usize, data_idx: usize },
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
    fn get_global_error(&self, data_idx: usize) -> Error {
        match self {
            &Self::LenIdxOutOfBound { len_idx } => Error::LenIdxOutOfBound { data_idx, len_idx },
            &Self::NotALen { len_idx } => Error::NotALen { data_idx, len_idx },
        }
    }
}

type LenResult = result::Result<usize, LenError>;

enum ReadError {
    Io(std::io::Error),
    Eof,
    FromUtf8(std::string::FromUtf8Error),
}

type ReadResult = result::Result<DataValue, ReadError>;

impl DataFormat {
    fn len(&self, values: &Vec<DataValue>) -> LenResult {
        match self {
            Self::Len { len, data_idx: _ } => Ok(*len),
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
            Self::Len {
                len: _,
                data_idx: _,
            } => Ok(DataValue::Len(buf.get_uint(len))),
            Self::Uint { len: _ } => Ok(DataValue::Uint(buf.get_uint(len))),
            Self::Int { len: _ } => Ok(DataValue::Int(buf.get_int(len))),
            Self::FixedString { len: _ } | Self::VarString { len_idx: _ } => {
                let mut str_buf = vec![0u8; len];
                buf.read_exact(&mut str_buf).unwrap();
                match String::from_utf8(str_buf) {
                    Ok(s) => Ok(DataValue::String(s)),
                    Err(e) => Err(ReadError::FromUtf8(e)),
                }
            }
            Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ } => {
                let mut bytes_buf = vec![0u8; len];
                buf.read_exact(&mut bytes_buf).unwrap();
                Ok(DataValue::Bytes(bytes_buf))
            }
        }
    }

    fn write_to_buf(&self, value: &DataValue, buf: &mut &mut [u8]) {
        match (self, value) {
            (Self::Len { len, data_idx: _ }, DataValue::Len(v)) => buf.put_uint(*v, *len),
            (Self::Uint { len }, DataValue::Uint(v)) => buf.put_uint(*v, *len),
            (Self::Int { len }, DataValue::Int(v)) => buf.put_int(*v, *len),
            (
                Self::FixedString { len: _ } | Self::VarString { len_idx: _ },
                DataValue::String(char_buf),
            ) => buf.put(char_buf.as_bytes()),
            (
                Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ },
                DataValue::Bytes(bytes_buf),
            ) => buf.put(bytes_buf.as_slice()),
            _ => panic!(),
        }
    }

    fn read_from_tcp_stream(&self, len: usize, stream: &mut TcpStream) -> ReadResult {
        let mut integer_buf = [0u8; size_of::<u64>()];
        let mut integer_slice = &mut integer_buf[size_of::<u64>() - len..];

        let mut bytes_buf = vec![0u8; len];

        while let Err(e) = match self {
            Self::Len {
                len: _,
                data_idx: _,
            }
            | Self::Uint { len: _ }
            | Self::Int { len: _ } => stream.read_exact(&mut integer_slice),
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
                    sleep(Duration::from_millis(500))
                }
                _ => return Err(ReadError::Io(e)),
            };
        }

        match self {
            Self::Len {
                len: _,
                data_idx: _,
            } => Ok(DataValue::Len(u64::from_be_bytes(integer_buf))),
            Self::Uint { len: _ } => Ok(DataValue::Uint(u64::from_be_bytes(integer_buf))),
            Self::Int { len: _ } => {
                let mut v = i64::from_be_bytes(integer_buf);

                let offset = (size_of::<u64>() - len) * u8::BITS as usize;
                v = v << offset >> offset;
                Ok(DataValue::Int(v))
            }
            Self::FixedString { len: _ } | Self::VarString { len_idx: _ } => {
                match String::from_utf8(bytes_buf) {
                    Ok(s) => Ok(DataValue::String(s)),
                    Err(e) => Err(ReadError::FromUtf8(e)),
                }
            }
            Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ } => {
                Ok(DataValue::Bytes(bytes_buf))
            }
        }
    }

    fn write_to_tcp_stream(&self, value: &DataValue, stream: &mut TcpStream) -> Result<()> {
        match (self, value) {
            (Self::Len { len, data_idx: _ }, DataValue::Len(v))
            | (Self::Uint { len }, DataValue::Uint(v)) => {
                stream.write_all(&v.to_be_bytes()[size_of_val(v) - *len..])
            }
            (Self::Int { len }, DataValue::Int(v)) => {
                stream.write_all(&v.to_be_bytes()[size_of_val(v) - *len..])
            }
            (
                Self::FixedString { len: _ } | Self::VarString { len_idx: _ },
                DataValue::String(buf),
            ) => stream.write_all(buf.as_bytes()),
            (
                Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ },
                DataValue::Bytes(buf),
            ) => stream.write_all(buf.as_slice()),
            _ => panic!(),
        }
        .map_err(|e| Error::Io(e))?;
        Ok(())
    }

    fn len_by_idx(len_idx: usize, values: &Vec<DataValue>) -> LenResult {
        if let Some(value) = values.get(len_idx) {
            match value {
                DataValue::Len(v) => Ok(*v as usize),
                _ => Err(LenError::NotALen { len_idx }),
            }
        } else {
            Err(LenError::LenIdxOutOfBound { len_idx })
        }
    }
}

#[derive(Clone)]
pub struct MessageFormat {
    data_fmts: Vec<DataFormat>,
}

impl MessageFormat {
    pub fn new(data_fmts: Vec<DataFormat>) -> Self {
        MessageFormat { data_fmts }
    }

    pub fn is_empty(&self) -> bool {
        self.data_fmts.is_empty()
    }

    pub fn decode(&self, buf: &Vec<u8>) -> Result<Message> {
        let mut values = Vec::<DataValue>::with_capacity(self.data_fmts.len());
        let mut slice = buf.as_slice();
        for (idx, data_fmt) in self.data_fmts.iter().enumerate() {
            let len = data_fmt.len(&values).map_err(|e| e.get_global_error(idx))?;
            values.push(
                data_fmt
                    .read_from_buf(len, &mut slice)
                    .map_err(|e| match e {
                        ReadError::Eof => Error::LenOutOfBound { data_idx: idx, len },
                        ReadError::FromUtf8(e) => Error::FromUtf8 { data_idx: idx, e },
                        _ => panic!(),
                    })?,
            );
        }

        Ok(Message { values })
    }

    pub fn encode(&self, msg: &Message) -> Result<Vec<u8>> {
        let mut buf = Vec::<u8>::default();
        let mut len = 0;
        for (idx, (data_fmt, value)) in self.data_fmts.iter().zip(msg.values.iter()).enumerate() {
            let kind_len = data_fmt
                .len(&msg.values)
                .map_err(|e| e.get_global_error(idx))?;
            buf.resize(len + kind_len, 0);
            let mut slice = &mut buf[len..len + kind_len];
            data_fmt.write_to_buf(value, &mut slice);
            len += kind_len;
        }

        Ok(buf)
    }

    pub fn read_from(&self, stream: &mut TcpStream) -> Result<Message> {
        let mut values = Vec::<DataValue>::with_capacity(self.data_fmts.len());
        for (idx, data_fmt) in self.data_fmts.iter().enumerate() {
            let len = data_fmt.len(&values).map_err(|e| e.get_global_error(idx))?;
            match data_fmt.read_from_tcp_stream(len, stream) {
                Ok(v) => values.push(v),
                Err(ReadError::Io(e)) => return Err(Error::Io(e)),
                Err(ReadError::Eof) => return Err(Error::Eof),
                Err(ReadError::FromUtf8(e)) => return Err(Error::FromUtf8 { data_idx: idx, e }),
            }
        }
        Ok(Message { values })
    }

    pub fn write_to(&self, msg: &Message, stream: &mut TcpStream) -> Result<()> {
        for (data_fmt, value) in self.data_fmts.iter().zip(msg.values.iter()) {
            data_fmt.write_to_tcp_stream(value, stream)?;
        }
        Ok(())
    }
}
