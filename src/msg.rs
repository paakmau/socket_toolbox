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

    pub fn values(&self) -> &Vec<DataValue> {
        &self.values
    }
}

#[derive(Debug, Clone)]
pub enum DataFormat {
    Uint { len: usize },
    Int { len: usize },
    FixedString { len: usize },
    VarString { len_idx: usize },
    FixedBytes { len: usize },
    VarBytes { len_idx: usize },
}

impl DataFormat {
    fn len(&self, values: &Vec<DataValue>) -> Result<usize, ()> {
        match self {
            Self::Uint { len } => Ok(*len),
            Self::Int { len } => Ok(*len),
            Self::FixedString { len } => Ok(*len),
            Self::VarString { len_idx } => Self::len_by_idx(*len_idx, values),
            Self::FixedBytes { len } => Ok(*len),
            Self::VarBytes { len_idx } => Self::len_by_idx(*len_idx, values),
        }
    }

    fn read_from_buf(&self, values: &Vec<DataValue>, buf: &mut &[u8]) -> Result<DataValue, ()> {
        let len = self.len(values)?;
        match self {
            Self::Uint { len: _ } => Ok(DataValue::Uint(buf.get_uint(len))),
            Self::Int { len: _ } => Ok(DataValue::Int(buf.get_int(len))),
            Self::FixedString { len: _ } | Self::VarString { len_idx: _ } => {
                let mut str_buf = vec![0u8; len];
                buf.read_exact(&mut str_buf).map_err(|_| ())?;
                Ok(DataValue::String(
                    String::from_utf8(str_buf).map_err(|_| ())?,
                ))
            }
            Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ } => {
                let mut bytes_buf = vec![0u8; len];
                buf.read_exact(&mut bytes_buf).map_err(|_| ())?;
                Ok(DataValue::Bytes(bytes_buf))
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
            (
                Self::FixedString { len: _ } | Self::VarString { len_idx: _ },
                DataValue::String(char_buf),
            ) => {
                buf.put(char_buf.as_bytes());
            }
            (
                Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ },
                DataValue::Bytes(bytes_buf),
            ) => {
                buf.put(bytes_buf.as_slice());
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
            Self::FixedString { len: _ } | Self::VarString { len_idx: _ } => {
                let mut buf = vec![0u8; len];
                stream.read_exact(&mut buf).map_err(|_| ())?;
                Ok(DataValue::String(String::from_utf8(buf).map_err(|_| ())?))
            }
            Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ } => {
                let mut buf = vec![0u8; len];
                stream.read_exact(&mut buf).map_err(|_| ())?;
                Ok(DataValue::Bytes(buf))
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
            (
                Self::FixedString { len: _ } | Self::VarString { len_idx: _ },
                DataValue::String(buf),
            ) => stream.write_all(buf.as_bytes()).map_err(|_| ())?,
            (
                Self::FixedBytes { len: _ } | Self::VarBytes { len_idx: _ },
                DataValue::Bytes(buf),
            ) => stream.write_all(buf.as_slice()).map_err(|_| ())?,
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
    data_fmts: Vec<DataFormat>,
}

impl MessageFormat {
    pub fn new(data_fmts: Vec<DataFormat>) -> Self {
        MessageFormat { data_fmts }
    }

    pub fn data_fmts(&self) -> &Vec<DataFormat> {
        &self.data_fmts
    }

    pub fn len(&self) -> usize {
        self.data_fmts.len()
    }

    pub fn decode(&self, buf: &Vec<u8>) -> Result<Message, ()> {
        let mut values = Vec::<DataValue>::with_capacity(self.data_fmts.len());
        let mut slice = buf.as_slice();
        for data_fmt in &self.data_fmts {
            values.push(data_fmt.read_from_buf(&values, &mut slice)?);
        }

        Ok(Message { values })
    }

    pub fn encode(&self, msg: &Message) -> Result<Vec<u8>, ()> {
        let mut buf = Vec::<u8>::default();
        let mut len = 0;
        for (data_fmt, value) in self.data_fmts.iter().zip(msg.values.iter()) {
            let kind_len = data_fmt.len(&msg.values)?;
            buf.resize(len + kind_len, 0);
            let mut slice = &mut buf[len..len + kind_len];
            data_fmt.write_to_buf(value, &mut slice)?;
            len += kind_len;
        }

        Ok(buf)
    }

    pub fn read_from(&self, stream: &mut TcpStream) -> Result<Message, ()> {
        let mut values = Vec::<DataValue>::with_capacity(self.data_fmts.len());
        for data_fmt in &self.data_fmts {
            values.push(
                data_fmt
                    .read_from_tcp_stream(&values, stream)
                    .map_err(|_| ())?,
            );
        }
        Ok(Message { values })
    }

    pub fn write_to(&self, msg: &Message, stream: &mut TcpStream) -> Result<(), ()> {
        for (data_fmt, value) in self.data_fmts.iter().zip(msg.values.iter()) {
            data_fmt
                .write_to_tcp_stream(value, stream)
                .map_err(|_| ())?;
        }
        Ok(())
    }
}
