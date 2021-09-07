use std::{
    io::{Read, Write},
    mem::{size_of, size_of_val},
    net::TcpStream,
};

use bytes::{Buf, BufMut};

#[derive(Debug, Clone)]
pub enum DataValue {
    Uint(u64),
    Int(i64),
    FixedString(String),
    VarString(String),
}

#[derive(Debug, Clone)]
pub struct Message {
    values: Vec<DataValue>,
}

impl Message {
    pub fn new(values: Vec<DataValue>) -> Self {
        Self { values }
    }

    pub fn values(&self) -> &Vec<DataValue> {
        &self.values
    }
}

#[derive(Debug, Clone)]
pub enum DataKind {
    Uint { len: usize },
    Int { len: usize },
    FixedString { len: usize },
    VarString { len_idx: usize },
}

impl DataKind {
    fn len(&self, values: &Vec<DataValue>) -> Result<usize, ()> {
        match self {
            Self::Uint { len } => Ok(*len),
            Self::Int { len } => Ok(*len),
            Self::FixedString { len } => Ok(*len),
            Self::VarString { len_idx } => Self::len_by_idx(*len_idx, values),
        }
    }

    fn read_from_buf(&self, values: &Vec<DataValue>, buf: &mut &[u8]) -> Result<DataValue, ()> {
        let len = self.len(values)?;
        match self {
            Self::Uint { len: _ } => Ok(DataValue::Uint(buf.get_uint(len))),
            Self::Int { len: _ } => Ok(DataValue::Int(buf.get_int(len))),
            Self::FixedString { len: _ } => {
                let mut str_buf = vec![0u8; len];
                buf.read_exact(&mut str_buf).map_err(|_| ())?;
                Ok(DataValue::FixedString(
                    String::from_utf8(str_buf).map_err(|_| ())?,
                ))
            }
            Self::VarString { len_idx: _ } => {
                let mut str_buf = vec![0u8; len];
                buf.read_exact(&mut str_buf).map_err(|_| ())?;
                Ok(DataValue::VarString(
                    String::from_utf8(str_buf).map_err(|_| ())?,
                ))
            }
        }
    }

    fn write_to_buf(&self, value: &DataValue, buf: &mut &mut [u8]) -> Result<(), ()> {
        match (self, value) {
            (Self::Uint { len }, DataValue::Uint(v)) => {
                buf.put_uint(*v, *len);
            }
            (Self::Int { len }, DataValue::Int(v)) => {
                buf.put_int(*v, *len);
            }
            (Self::FixedString { len: _ }, DataValue::FixedString(char_buf)) => {
                buf.put(char_buf.as_bytes());
            }
            _ => return Err(()),
        }
        Ok(())
    }

    fn read_from_tcp_stream(
        &self,
        values: &Vec<DataValue>,
        stream: &mut TcpStream,
    ) -> Result<DataValue, ()> {
        let len = self.len(values)?;

        let mut bytes = [0u8; size_of::<u64>()];
        match self {
            Self::Uint { len: _ } => {
                let mut buf = &mut bytes[size_of::<u64>() - len..];
                stream.read(&mut buf).map_err(|_| ())?;
                Ok(DataValue::Uint(u64::from_be_bytes(bytes)))
            }
            Self::Int { len: _ } => {
                let mut v;
                let mut buf = &mut bytes[size_of::<u64>() - len..];
                stream.read(&mut buf).map_err(|_| ())?;
                v = i64::from_be_bytes(bytes);

                let offset = (size_of::<u64>() - len) * u8::BITS as usize;
                v = v << offset >> offset;
                Ok(DataValue::Int(v))
            }
            Self::FixedString { len: _ } => {
                let mut buf = vec![0u8; len];
                stream.read_exact(&mut buf).map_err(|_| ())?;
                Ok(DataValue::FixedString(
                    String::from_utf8(buf).map_err(|_| ())?,
                ))
            }
            Self::VarString { len_idx: _ } => {
                let mut buf = vec![0u8; len];
                stream.read_exact(&mut buf).map_err(|_| ())?;
                Ok(DataValue::VarString(
                    String::from_utf8(buf).map_err(|_| ())?,
                ))
            }
        }
    }

    fn write_to_tcp_stream(&self, value: &DataValue, stream: &mut TcpStream) -> Result<(), ()> {
        match (self, value) {
            (Self::Uint { len }, DataValue::Uint(v)) => stream
                .write_all(&v.to_be_bytes()[size_of_val(v) - *len..])
                .map_err(|_| ())?,
            (Self::Int { len }, DataValue::Int(v)) => stream
                .write_all(&v.to_be_bytes()[size_of_val(v) - *len..])
                .map_err(|_| ())?,
            (Self::FixedString { len: _ }, DataValue::FixedString(buf)) => {
                stream.write_all(buf.as_bytes()).map_err(|_| ())?
            }
            (Self::VarString { len_idx: _ }, DataValue::VarString(buf)) => {
                stream.write_all(buf.as_bytes()).map_err(|_| ())?
            }
            _ => panic!(),
        };
        Ok(())
    }

    fn len_by_idx(len_idx: usize, values: &Vec<DataValue>) -> Result<usize, ()> {
        if let Some(value) = values.get(len_idx) {
            match value {
                DataValue::Uint(v) => Ok(*v as usize),
                DataValue::Int(v) => Ok(*v as usize),
                _ => Err(()),
            }
        } else {
            Err(())
        }
    }
}

#[derive(Clone)]
pub struct MessageFormat {
    kinds: Vec<DataKind>,
}

impl MessageFormat {
    pub fn new(kinds: Vec<DataKind>) -> Self {
        MessageFormat { kinds }
    }

    pub fn kinds(&self) -> &Vec<DataKind> {
        &self.kinds
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn decode(&self, buf: &Vec<u8>) -> Result<Message, ()> {
        let mut values = Vec::<DataValue>::with_capacity(self.kinds.len());
        let mut slice = buf.as_slice();
        for kind in &self.kinds {
            values.push(kind.read_from_buf(&values, &mut slice)?);
        }

        Ok(Message { values })
    }

    pub fn encode(&self, msg: &Message) -> Result<Vec<u8>, ()> {
        let mut buf = Vec::<u8>::default();
        let mut len = 0;
        for (kind, value) in self.kinds.iter().zip(msg.values.iter()) {
            let kind_len = kind.len(&msg.values)?;
            buf.resize(len + kind_len, 0);
            let mut slice = &mut buf[len..len + kind_len];
            kind.write_to_buf(value, &mut slice)?;
            len += kind_len;
        }

        Ok(buf)
    }

    pub fn read_from(&self, stream: &mut TcpStream) -> Result<Message, ()> {
        let mut values = Vec::<DataValue>::with_capacity(self.kinds.len());
        for kind in &self.kinds {
            values.push(kind.read_from_tcp_stream(&values, stream).map_err(|_| ())?);
        }
        Ok(Message { values })
    }

    pub fn write_to(&self, msg: &Message, stream: &mut TcpStream) -> Result<(), ()> {
        for (kind, value) in self.kinds.iter().zip(msg.values.iter()) {
            kind.write_to_tcp_stream(value, stream).map_err(|_| ())?;
        }
        Ok(())
    }
}
