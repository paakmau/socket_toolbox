use std::result;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error, kind: `{:?}`", std::io::Error::kind(.0))]
    Io(#[from] std::io::Error),

    #[error("the MessageFormat should not be empty")]
    MessageFormatEmpty,

    #[error("invalid IP address syntax, `{invalid_addr}`")]
    AddrParse { invalid_addr: String },

    #[error("there is no such client connected `{addr}`")]
    NoSuchClient { addr: String },

    #[error("the client not connected to a server")]
    NotConnected,

    #[error("the index of length is out of bound, index of item: `{item_idx}`, index of length: `{len_idx}`")]
    LenIdxOutOfBound { item_idx: usize, len_idx: usize },

    #[error("the item specified by index `{len_idx}` is not a length, index of item: `{item_idx}`")]
    NotALen { item_idx: usize, len_idx: usize },

    #[error("the length for this kind of item is too large, max len: `{max_len}`, index of item: `{item_idx}`, actual len: `{len}`")]
    LenTooLarge { max_len: usize, item_idx: usize, len: usize},

    #[error("the length of value is out of bound, len specified by format: `{specified_len}`, index of item: `{item_idx}`, len of item: `{item_len}`")]
    ValueLenOutOfBound { specified_len: usize, item_idx: usize, item_len: usize },

    #[error("no more bytes can be read")]
    Eof,

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
