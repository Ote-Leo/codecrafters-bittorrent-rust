use std::{
    error::Error,
    fmt::{self, Display},
};

use bytes::BufMut;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandShake {
    /// The length of the protocl string (Will always be 19)
    length: u8,

    /// The String "BitTorrent protocol"
    protocol: [u8; 19],

    /// Eight reserved bytes, which are all set to zero
    reserved: [u8; 8],

    /// SHA1 infohash
    pub info_hash: [u8; 20],

    /// peer id
    pub peer_id: [u8; 20],
}

impl HandShake {
    pub fn new(info_hash: [u8; 20]) -> Self {
        Self {
            info_hash,
            length: 19,
            protocol: *b"BitTorrent protocol",
            reserved: [0; 8],
            peer_id: *b"00112233445566778899",
        }
    }

    pub fn peer_id(self, peer_id: [u8; 20]) -> Self {
        Self { peer_id, ..self }
    }
}

impl From<HandShake> for [u8; 68] {
    fn from(value: HandShake) -> Self {
        let mut buf = Vec::with_capacity(68);

        buf.push(value.length);
        buf.put_slice(&value.protocol);
        buf.put_slice(&value.reserved);
        buf.put_slice(&value.info_hash);
        buf.put_slice(&value.peer_id);

        buf.try_into().unwrap()
    }
}

impl From<HandShake> for Vec<u8> {
    fn from(value: HandShake) -> Self {
        let mut buf = Vec::with_capacity(68);

        buf.push(value.length);
        buf.put_slice(&value.protocol);
        buf.put_slice(&value.reserved);
        buf.put_slice(&value.info_hash);
        buf.put_slice(&value.peer_id);

        buf
    }
}

#[derive(Debug, Clone)]
pub enum ConversionError {
    InvalidLength { length: u8 },
    InvalidProtocol { protocol: Vec<u8> },
}

impl Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ConversionError::*;
        match self {
            InvalidLength { length } => {
                format!("protocol length should only be '19', but found '{length}'").fmt(f)
            }
            InvalidProtocol { protocol } => format!(
                "protocol string should only be 'BitTorrent protocol', but found {}",
                String::from_utf8_lossy(protocol)
            )
            .fmt(f),
        }
    }
}

impl Error for ConversionError {}

impl TryFrom<[u8; 68]> for HandShake {
    type Error = ConversionError;

    fn try_from(value: [u8; 68]) -> Result<Self, Self::Error> {
        use ConversionError::*;

        match value[0] {
            19 => (),
            length => return Err(InvalidLength { length }),
        }

        match &value[1..20] {
            b"BitTorrent protocol" => (),
            protocol => {
                return Err(InvalidProtocol {
                    protocol: protocol.to_vec(),
                })
            }
        }
        let reserved = value[20..28].try_into().unwrap();
        let info_hash = value[28..48].try_into().unwrap();
        let peer_id = value[48..68].try_into().unwrap();

        Ok(Self {
            length: 19,
            protocol: *b"BitTorrent protocol",
            reserved,
            info_hash,
            peer_id,
        })
    }
}
