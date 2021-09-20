use std::{
    io::{self},
    mem::{size_of, size_of_val},
    ops::{Deref, DerefMut},
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
pub enum ItemFormat {
    Len { len: usize },
    Uint { len: usize },
    Int { len: usize },
    FixedString { len: usize },
    VarString { len_idx: usize },
    FixedBytes { len: usize },
    VarBytes { len_idx: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageFormat {
    fmts: Vec<ItemFormat>,
}

impl MessageFormat {
    pub fn new(fmts: &[ItemFormat]) -> Result<Self> {
        if fmts.is_empty() {
            return Err(Error::MessageFormatEmpty);
        }

        fmts.iter()
            .enumerate()
            .try_for_each(|(idx, fmt)| Self::validate_fmt(fmt, idx, fmts))?;

        Ok(Self {
            fmts: fmts.to_vec(),
        })
    }

    fn validate_fmt(fmt: &ItemFormat, idx: usize, fmts: &[ItemFormat]) -> Result<()> {
        let min_len = 1;
        let mut max_len = usize::MAX;
        match fmt {
            ItemFormat::Len { .. } | ItemFormat::Uint { .. } => max_len = size_of::<u64>(),
            ItemFormat::Int { .. } => max_len = size_of::<u64>(),
            _ => {}
        }

        match fmt {
            // Validate the length.
            ItemFormat::Len { len }
            | ItemFormat::Uint { len }
            | ItemFormat::Int { len }
            | ItemFormat::FixedString { len }
            | ItemFormat::FixedBytes { len } => {
                if *len < min_len {
                    return Err(Error::LenTooSmall {
                        min_len,
                        item_idx: idx,
                        len: *len,
                    });
                } else if *len > max_len {
                    return Err(Error::LenTooLarge {
                        max_len,
                        item_idx: idx,
                        len: *len,
                    });
                }
            }

            // Validate the index of length.
            ItemFormat::VarString { len_idx } | ItemFormat::VarBytes { len_idx } => {
                if *len_idx > idx {
                    return Err(Error::LenIdxTooLarge {
                        item_idx: idx,
                        len_idx: *len_idx,
                    });
                } else if !matches!(fmts[*len_idx], ItemFormat::Len { .. }) {
                    return Err(Error::NotALen {
                        item_idx: idx,
                        len_idx: *len_idx,
                    });
                }
            }
        }
        Ok(())
    }
}

impl Deref for MessageFormat {
    type Target = Vec<ItemFormat>;

    fn deref(&self) -> &Self::Target {
        &self.fmts
    }
}

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

impl Deref for Message {
    type Target = Vec<ItemValue>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

#[inline]
fn value_len_by_idx(len_idx: usize, values: &[ItemValue]) -> usize {
    if let Some(value) = values.get(len_idx) {
        match value {
            ItemValue::Len(v) => *v as usize,
            _ => panic!(),
        }
    } else {
        panic!()
    }
}

#[inline]
fn value_len(fmt: &ItemFormat, values: &[ItemValue]) -> usize {
    match fmt {
        ItemFormat::Len { len } => *len,
        ItemFormat::Uint { len } => *len,
        ItemFormat::Int { len } => *len,
        ItemFormat::FixedString { len } => *len,
        ItemFormat::VarString { len_idx } => value_len_by_idx(*len_idx, values),
        ItemFormat::FixedBytes { len } => *len,
        ItemFormat::VarBytes { len_idx } => value_len_by_idx(*len_idx, values),
    }
}

trait Read {
    fn read(&mut self, fmt: &ItemFormat, idx: usize, values: &[ItemValue]) -> Result<ItemValue>;
}

impl Read for &[u8] {
    #[inline]
    fn read(&mut self, fmt: &ItemFormat, idx: usize, values: &[ItemValue]) -> Result<ItemValue> {
        let len = value_len(fmt, values);

        if self.len() < len {
            return Err(Error::Eof);
        }

        match fmt {
            ItemFormat::Len { .. } => Ok(ItemValue::Len(self.get_uint(len))),
            ItemFormat::Uint { .. } => Ok(ItemValue::Uint(self.get_uint(len))),
            ItemFormat::Int { .. } => {
                let offset = (size_of::<i64>() - len) * u8::BITS as usize;
                Ok(ItemValue::Int(self.get_int(len) << offset >> offset))
            }

            ItemFormat::FixedString { .. } | ItemFormat::VarString { .. } => {
                let mut str_buf = vec![0u8; len];
                io::Read::read_exact(self, &mut str_buf).unwrap();
                match String::from_utf8(str_buf) {
                    Ok(s) => Ok(ItemValue::String(s)),
                    Err(e) => Err(Error::FromUtf8 { item_idx: idx, e }),
                }
            }

            ItemFormat::FixedBytes { .. } | ItemFormat::VarBytes { .. } => {
                let mut bytes_buf = vec![0u8; len];
                io::Read::read_exact(self, &mut bytes_buf).unwrap();
                Ok(ItemValue::Bytes(bytes_buf))
            }
        }
    }
}

trait Write {
    fn write(
        &mut self,
        fmt: &ItemFormat,
        idx: usize,
        value: &ItemValue,
        values: &[ItemValue],
    ) -> Result<()>;
}

impl Write for &mut [u8] {
    #[inline]
    fn write(
        &mut self,
        fmt: &ItemFormat,
        idx: usize,
        value: &ItemValue,
        values: &[ItemValue],
    ) -> Result<()> {
        let len = value_len(fmt, values);

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
            return Err(Error::LenTooLarge {
                max_len,
                item_idx: idx,
                len,
            });
        }
        if len < min_len {
            return Err(Error::ValueLenOutOfBound {
                specified_len: len,
                item_idx: idx,
                len: min_len,
            });
        }

        // Write value to buf.
        match (fmt, value) {
            (ItemFormat::Len { .. }, ItemValue::Len(v))
            | (ItemFormat::Uint { .. }, ItemValue::Uint(v)) => self.put_uint(*v, len),
            (ItemFormat::Int { .. }, ItemValue::Int(v)) => self.put_int(*v, len),
            (
                ItemFormat::FixedString { .. } | ItemFormat::VarString { .. },
                ItemValue::String(char_buf),
            ) => self.put(char_buf.as_bytes()),
            (
                ItemFormat::FixedBytes { .. } | ItemFormat::VarBytes { .. },
                ItemValue::Bytes(bytes_buf),
            ) => self.put(bytes_buf.as_slice()),
            _ => panic!(),
        }

        Ok(())
    }
}

pub struct MessageDecoder<'a, R: io::Read> {
    fmt: &'a MessageFormat,
    r: R,
}

impl<'a, R: io::Read> MessageDecoder<'a, R> {
    pub fn new(fmt: &'a MessageFormat, r: R) -> Self {
        Self { fmt, r }
    }

    pub fn decode(mut self, stop_flag: Arc<AtomicBool>) -> Result<Message> {
        let mut values = Vec::<ItemValue>::with_capacity(self.fmt.len());
        for (idx, item_fmt) in self.fmt.iter().enumerate() {
            let len = value_len(item_fmt, &values);

            let mut buf = vec![0u8; len];
            let mut cnt = 0usize;
            loop {
                match self.r.read(&mut buf[cnt..len]) {
                    Ok(n) => {
                        cnt += n;
                        if cnt == len {
                            break;
                        }
                        if n == 0 {
                            return Err(Error::Eof);
                        }
                    }

                    Err(e) => {
                        match e.kind() {
                            io::ErrorKind::ConnectionReset => return Err(Error::Eof),
                            io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted => {
                                if stop_flag.load(Ordering::Relaxed) {
                                    return Err(Error::Stopped);
                                }
                                sleep(Duration::from_millis(300))
                            }
                            _ => return Err(Error::Io(e)),
                        };
                    }
                }
            }
            values.push(buf.deref().read(item_fmt, idx, &values)?);
        }

        Ok(Message { values })
    }
}

pub struct MessageEncoder<'a, W: io::Write> {
    fmt: &'a MessageFormat,
    w: W,
}

impl<'a, W: io::Write> MessageEncoder<'a, W> {
    pub fn new(fmt: &'a MessageFormat, w: W) -> Self {
        Self { fmt, w }
    }

    pub fn encode(mut self, msg: &Message) -> Result<()> {
        for (idx, (item_fmt, item_value)) in self.fmt.iter().zip(msg.iter()).enumerate() {
            let len = value_len(item_fmt, msg);
            let mut buf = vec![0u8; len];
            buf.deref_mut().write(item_fmt, idx, item_value, msg)?;
            self.w.write_all(&buf)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use crate::msg::{
        ItemFormat, ItemValue, Message, MessageDecoder, MessageEncoder, MessageFormat,
    };

    #[test]
    fn encode_and_decode_ok() {
        let fmt = MessageFormat::new(&[
            ItemFormat::Len { len: 2 },
            ItemFormat::Uint { len: 2 },
            ItemFormat::Int { len: 1 },
            ItemFormat::FixedString { len: 8 },
            ItemFormat::VarString { len_idx: 0 },
        ])
        .unwrap();

        let msg = Message::new(vec![
            ItemValue::Len(16),
            ItemValue::Uint(2333),
            ItemValue::Int(127),
            ItemValue::String("aaaabbbb".to_string()),
            ItemValue::String("aaaabbbbccccdddd".to_string()),
        ]);

        let mut bytes = Vec::<u8>::default();
        assert!(MessageEncoder::new(&fmt, &mut bytes).encode(&msg).is_ok());

        let decoded_msg = MessageDecoder::new(&fmt, bytes.deref()).decode(Default::default());

        assert!(decoded_msg.is_ok());

        assert_eq!(msg, decoded_msg.unwrap());
    }
}
