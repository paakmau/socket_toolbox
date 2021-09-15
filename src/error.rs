use std::result;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error, kind: {:?}", std::io::Error::kind(.0))]
    Io(#[from] std::io::Error),

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

    #[error("the length is out of bound, index of data: {data_idx}, len: {len}")]
    LenOutOfBound { data_idx: usize, len: usize },

    #[error("no more bytes can be read")]
    Eof,

    #[error("the bytes can not be converted to a utf8 string, index of data: {data_idx}")]
    FromUtf8 {
        data_idx: usize,
        #[source]
        e: std::string::FromUtf8Error,
    },
}

pub type Result<T> = result::Result<T, Error>;
