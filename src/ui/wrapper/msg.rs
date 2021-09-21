use std::{num::ParseIntError, str::FromStr};

use hex::FromHexError;

use crate::{
    error::Error,
    msg::{ItemFormat, ItemValue},
};

#[derive(Debug, Clone, PartialEq, strum_macros::ToString, strum_macros::EnumIter)]
pub enum ItemKindWrapper {
    Len,
    Uint,
    Int,
    FixedString,
    VarString,
    FixedBytes,
    VarBytes,
}

impl ItemKindWrapper {
    pub fn from_item_format(fmt: &ItemFormatWrapper) -> Self {
        match fmt {
            ItemFormatWrapper::Len { .. } => Self::Len,
            ItemFormatWrapper::Uint { .. } => Self::Uint,
            ItemFormatWrapper::Int { .. } => Self::Int,
            ItemFormatWrapper::FixedString { .. } => Self::FixedString,
            ItemFormatWrapper::VarString { .. } => Self::VarString,
            ItemFormatWrapper::FixedBytes { .. } => Self::FixedBytes,
            ItemFormatWrapper::VarBytes { .. } => Self::VarBytes,
        }
    }

    pub fn default_item_format(&self) -> ItemFormatWrapper {
        match self {
            Self::Len => ItemFormatWrapper::Len { len: 1.to_string() },
            Self::Uint => ItemFormatWrapper::Uint { len: 1.to_string() },
            Self::Int => ItemFormatWrapper::Int { len: 1.to_string() },
            Self::FixedString => ItemFormatWrapper::FixedString { len: 1.to_string() },
            Self::VarString => ItemFormatWrapper::VarString {
                len_idx: 0.to_string(),
            },
            Self::FixedBytes => ItemFormatWrapper::FixedBytes { len: 1.to_string() },
            Self::VarBytes => ItemFormatWrapper::VarBytes {
                len_idx: 0.to_string(),
            },
        }
    }

    pub fn default_item_value(&self) -> ItemValueWrapper {
        match self {
            Self::Len => ItemValueWrapper::Len(0),
            Self::Uint => ItemValueWrapper::Uint(0.to_string()),
            Self::Int => ItemValueWrapper::Int(0.to_string()),
            Self::FixedString => ItemValueWrapper::String(Default::default()),
            Self::VarString => ItemValueWrapper::String(Default::default()),
            Self::FixedBytes => ItemValueWrapper::Bytes(Default::default()),
            Self::VarBytes => ItemValueWrapper::Bytes(Default::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemValueWrapper {
    Len(u64),
    Uint(String),
    Int(String),
    String(String),
    Bytes(String),
}

pub enum ParseError {
    Integer { s: String, e: ParseIntError },
    Bytes { s: String, e: FromHexError },
}

impl ParseError {
    pub fn global_error(&self, item_idx: usize) -> Error {
        match self {
            ParseError::Integer { s, e } => Error::IntegerParse {
                s: s.clone(),
                item_idx,
                e: e.clone(),
            },
            ParseError::Bytes { s, e } => Error::BytesParse {
                s: s.clone(),
                item_idx,
                e: *e,
            },
        }
    }
}

pub type ParseResult<T> = Result<T, ParseError>;

fn parse_integer<T>(s: &str) -> ParseResult<T>
where
    T: FromStr<Err = ParseIntError>,
{
    s.parse::<T>().map_err(|e| ParseError::Integer {
        s: s.to_string(),
        e,
    })
}

impl ItemValueWrapper {
    pub fn parse(&self) -> ParseResult<ItemValue> {
        match self {
            Self::Len(v) => Ok(ItemValue::Len(*v)),
            Self::Uint(s) => parse_integer::<u64>(s).map(ItemValue::Uint),
            Self::Int(s) => parse_integer::<i64>(s).map(ItemValue::Int),
            Self::String(s) => Ok(ItemValue::String(s.clone())),
            Self::Bytes(s) => hex::decode(s)
                .map(ItemValue::Bytes)
                .map_err(|e| ParseError::Bytes { s: s.clone(), e }),
        }
    }
}

impl From<&ItemValue> for ItemValueWrapper {
    fn from(value: &ItemValue) -> Self {
        match value {
            ItemValue::Len(v) => Self::Len(*v),
            ItemValue::Uint(v) => Self::Uint(v.to_string()),
            ItemValue::Int(v) => Self::Int(v.to_string()),
            ItemValue::String(s) => Self::String(s.clone()),
            ItemValue::Bytes(bytes) => Self::Bytes(hex::encode(bytes)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemFormatWrapper {
    Len { len: String },
    Uint { len: String },
    Int { len: String },
    FixedString { len: String },
    VarString { len_idx: String },
    FixedBytes { len: String },
    VarBytes { len_idx: String },
}

impl ItemFormatWrapper {
    pub fn parse(&self) -> ParseResult<ItemFormat> {
        match self {
            Self::Len { len } => parse_integer::<usize>(len).map(|len| ItemFormat::Len { len }),
            Self::Uint { len } => parse_integer::<usize>(len).map(|len| ItemFormat::Uint { len }),
            Self::Int { len } => parse_integer::<usize>(len).map(|len| ItemFormat::Int { len }),
            Self::FixedString { len } => {
                parse_integer::<usize>(len).map(|len| ItemFormat::FixedString { len })
            }
            Self::VarString { len_idx } => {
                parse_integer::<usize>(len_idx).map(|len_idx| ItemFormat::VarString { len_idx })
            }
            Self::FixedBytes { len } => {
                parse_integer::<usize>(len).map(|len| ItemFormat::FixedBytes { len })
            }
            Self::VarBytes { len_idx } => {
                parse_integer::<usize>(len_idx).map(|len_idx| ItemFormat::VarBytes { len_idx })
            }
        }
    }
}
