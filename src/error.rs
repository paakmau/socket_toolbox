use std::result;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error, kind: {:?}", std::io::Error::kind(.0))]
    Io(#[from] std::io::Error),

    #[error("the MessageFormat should not be empty")]
    MessageFormatEmpty,

    #[error("invalid IP address syntax, `{invalid_addr}`")]
    AddrParse { invalid_addr: String },

    #[error("there is no such client connected `{addr}`")]
    NoSuchClient { addr: String },

    #[error("the client not connected to a server")]
    NotConnected,

    #[error("the index of length is out of bound, index of data: {data_idx}, index of length: {len_idx}")]
    LenIdxOutOfBound { data_idx: usize, len_idx: usize },

    #[error("the data specified by index `{len_idx}` is not a length, index of data: {data_idx}")]
    NotALen { data_idx: usize, len_idx: usize },

    #[error("the length for this kind of data is too large, max len: {max_len}, index of data: {data_idx}, actual len: {len}")]
    LenTooLarge { max_len: usize, data_idx: usize, len: usize},

    #[error("the length of value is out of bound, len specified by format: {specified_len}, index of data: {data_idx}, len of data: {data_len}")]
    ValueLenOutOfBound { specified_len: usize, data_idx: usize, data_len: usize },

    #[error("no more bytes can be read")]
    Eof,

    #[error("socket need to be stopped")]
    Stopped,

    #[error("the bytes can not be converted to a utf8 string, index of data: {data_idx}")]
    FromUtf8 {
        data_idx: usize,
        #[source]
        e: std::string::FromUtf8Error,
    },
}

pub type Result<T> = result::Result<T, Error>;
