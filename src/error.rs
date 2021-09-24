use std::{num::ParseIntError, result};

use hex::FromHexError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error, kind: `{:?}`", std::io::Error::kind(.0))]
    Io(#[from] std::io::Error),

    #[error("the MessageFormat should not be empty")]
    MessageFormatEmpty,

    #[error("invalid IP address syntax, `{invalid_addr}`")]
    AddrParse { invalid_addr: String },

    #[error("`{s}` couldn't be parsed to a integer, index of item: `{item_idx}`, details: {e}")]
    IntegerParse {
        s: String,
        item_idx: usize,
        e: ParseIntError,
    },

    #[error("`{s}` couldn't be parsed to bytes, index of item: `{item_idx}`, details: {e}")]
    BytesParse {
        s: String,
        item_idx: usize,
        e: FromHexError,
    },

    #[error("there is no such client connected `{addr}`")]
    NoSuchClient { addr: String },

    #[error("the client not connected to a server")]
    NotConnected,

    #[error("the index of length should be smaller than the index of item, index of item: `{item_idx}`, index of length: `{len_idx}`")]
    LenIdxTooLarge { item_idx: usize, len_idx: usize },

    #[error(
        "the item specified by index `{len_idx}` is not a length, index of item: `{item_idx}`"
    )]
    NotALen { item_idx: usize, len_idx: usize },

    #[error("the length for this kind of item is too small, min len: `{min_len}`, index of item: `{item_idx}`, actual len: `{len}`")]
    LenTooSmall {
        min_len: usize,
        item_idx: usize,
        len: usize,
    },

    #[error("the length for this kind of item is too large, max len: `{max_len}`, index of item: `{item_idx}`, actual len: `{len}`")]
    LenTooLarge {
        max_len: usize,
        item_idx: usize,
        len: usize,
    },

    #[error("the length of value is out of bound, len specified by format: `{specified_len}`, index of item: `{item_idx}`, len of item: `{len}`")]
    ValueLenOutOfBound {
        specified_len: usize,
        item_idx: usize,
        len: usize,
    },

    #[error("no more bytes can be read")]
    EndOfStream,

    #[error("socket need to be stopped")]
    Stopped,

    #[error("the bytes can not be converted to a utf8 string, index of item: `{item_idx}`")]
    FromUtf8 {
        item_idx: usize,
        #[source]
        e: std::string::FromUtf8Error,
    },
}

pub type Result<T> = result::Result<T, Error>;
