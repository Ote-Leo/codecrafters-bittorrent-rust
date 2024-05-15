use std::{
    error::Error,
    fmt::{self, Display},
    io::{Read, Write},
    net::TcpStream,
};

use anyhow::{bail, Context};
use bytes::BufMut;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::torrent::Torrent;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerMessage {
    Choke,
    UnChoke,
    Interested,
    NotInterested,
    Have {
        piece_index: u32,
    },
    Bitfield {
        fields: Vec<u8>,
    },
    Request {
        piece_index: u32,
        offset: u32,
        length: u32,
    },
    Piece {
        piece_index: u32,
        offset: u32,
        piece: Vec<u8>,
    },
    Cancel {
        piece_index: u32,
        offset: u32,
        length: u32,
    },
}

impl From<PeerMessage> for Vec<u8> {
    fn from(value: PeerMessage) -> Self {
        use PeerMessage::*;
        let mut buf = vec![];

        match value {
            Choke => buf.push(0),
            UnChoke => buf.push(1),
            Interested => buf.push(2),
            NotInterested => buf.push(3),
            Have { piece_index } => {
                buf.push(4);
                buf.put_u32(piece_index);
            }
            Bitfield { mut fields } => {
                buf.push(5);
                buf.append(&mut fields);
            }
            Request {
                piece_index,
                offset,
                length,
            } => {
                buf.push(6);
                buf.put_u32(piece_index);
                buf.put_u32(offset);
                buf.put_u32(length);
            }
            Piece {
                piece_index,
                offset,
                mut piece,
            } => {
                buf.push(7);
                buf.put_u32(piece_index);
                buf.put_u32(offset);
                buf.append(&mut piece);
            }
            Cancel {
                piece_index,
                offset,
                length,
            } => {
                buf.push(8);
                buf.put_u32(piece_index);
                buf.put_u32(offset);
                buf.put_u32(length);
            }
        }

        buf
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerMessageError {
    UnknownCode(u8),
}

impl Display for PeerMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use PeerMessageError::*;

        match self {
            UnknownCode(code) => format!("unknown peer message code {code}").fmt(f),
        }
    }
}

impl Error for PeerMessageError {}

impl TryFrom<&[u8]> for PeerMessage {
    type Error = PeerMessageError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        use PeerMessage::*;
        use PeerMessageError::*;

        let mut offset = 0;
        let code = value[offset];
        offset += 1;
        Ok(match code {
            0 => Choke,
            1 => UnChoke,
            2 => Interested,
            3 => NotInterested,
            4 => {
                let piece_index = u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());

                Have { piece_index }
            }
            5 => {
                let fields = value[offset..].to_vec();

                Bitfield { fields }
            }
            6 => {
                let piece_index = u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());
                offset += 4;

                let piece_offset =
                    u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());
                offset += 4;

                let length = u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());

                Request {
                    piece_index,
                    offset: piece_offset,
                    length,
                }
            }
            7 => {
                let piece_index = u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());
                offset += 4;

                let piece_offset =
                    u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());
                offset += 4;

                let piece = value[offset..].to_vec();

                Piece {
                    piece_index,
                    offset: piece_offset,
                    piece,
                }
            }
            8 => {
                let piece_index = u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());
                offset += 4;

                let piece_offset =
                    u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());
                offset += 4;

                let length = u32::from_be_bytes(value[offset..offset + 4].try_into().unwrap());

                Cancel {
                    piece_index,
                    offset: piece_offset,
                    length,
                }
            }
            code => return Err(UnknownCode(code)),
        })
    }
}

pub fn receive_message(stream: &mut TcpStream) -> anyhow::Result<PeerMessage> {
    let mut length_buf = [0; 4];
    stream
        .read_exact(&mut length_buf)
        .context("reading message length")?;
    let length = u32::from_be_bytes(length_buf);
    let mut message = vec![0u8; length as usize];
    stream
        .read_exact(&mut message)
        .context(format!("reading message slice of length {length}"))?;
    Ok(message.as_slice().try_into()?)
}

pub fn send_message(stream: &mut TcpStream, message: PeerMessage) -> anyhow::Result<()> {
    let message_buf: Vec<u8> = message.into();
    stream
        .write(&(message_buf.len() as u32).to_be_bytes())
        .context(format!("sending message length of {}", message_buf.len()))?;
    stream
        .write_all(message_buf.as_slice())
        .context("sending message message")?;
    stream.flush()?;
    Ok(())
}

fn calculate_block_length(
    torrent: &Torrent,
    piece_index: usize,
    block_size: u32,
) -> (usize, u32, u32) {
    let piece_count = torrent.info.pieces.0.len();
    let piece_length = torrent.info.piece_length;

    let piece_length = if piece_index == piece_count - 1 {
        // last piece might have a different length
        // TODO: handle the case of multiple files
        let total_length = torrent.content_length();
        total_length - piece_length * (piece_count - 1)
    } else {
        piece_length
    };

    let block_count = f32::ceil(piece_length as f32 / block_size as f32) as u32;

    let remainder = piece_length as u32 % block_size;
    let last_block_length = if remainder == 0 {
        block_size
    } else {
        remainder
    };

    let block_size_sum = (block_count - 1) * block_size + last_block_length;
    debug_assert_eq!(
        block_size_sum, piece_length as u32,
        "blocks doesn't add up to the requried piece_length"
    );

    (piece_length, block_count, last_block_length)
}

pub fn download_piece(
    stream: &mut TcpStream,
    torrent: &Torrent,
    piece_index: usize,
    block_size: u32,
) -> anyhow::Result<Vec<u8>> {
    let (piece_length, block_count, last_block_length) =
        calculate_block_length(torrent, piece_index, block_size);

    let mut piece = vec![0u8; piece_length];

    for i in 0..(block_count - 1) {
        let offset = i * block_size;
        download_block(stream, piece_index as u32, offset, block_size, &mut piece)?;
    }

    // download last blocks
    let offset = (block_count - 1) * block_size;
    download_block(
        stream,
        piece_index as u32,
        offset,
        last_block_length,
        &mut piece,
    )?;

    Ok(piece)
}

fn download_block(
    stream: &mut TcpStream,
    piece_index: u32,
    offset: u32,
    length: u32,
    piece: &mut [u8],
) -> anyhow::Result<()> {
    let message = PeerMessage::Request {
        piece_index,
        offset,
        length,
    };
    send_message(stream, message).context(format!("requesting piece[{piece_index}][{offset}]"))?;
    let (block_piece_index, block_offset, block) = match receive_message(stream)
        .context(format!("waiting for piece[{piece_index}][{offset}]"))?
    {
        PeerMessage::Piece {
            piece_index: _,
            offset,
            piece,
        } => (piece_index, offset, piece),
        message => bail!("expected a Unchoke but found a {message:?}"),
    };

    debug_assert_eq!(
        piece_index, block_piece_index,
        "requestd piece index doesn't match recieved piece index"
    );

    debug_assert_eq!(
        offset, block_offset,
        "requestd block offset doesn't match recieved block offset"
    );

    let block_length = block.len() as u32;
    debug_assert_eq!(
        length, block_length,
        "requestd block length doesn't match recieved block length"
    );

    piece[block_offset as usize..(block_offset + block_length) as usize].copy_from_slice(&block);

    Ok(())
}

pub fn initiate_download(stream: &mut TcpStream) -> anyhow::Result<()> {
    match receive_message(stream).context("waiting for bitfield")? {
        PeerMessage::Bitfield { .. } => (),
        message => bail!("expected a Bitfield but found a {message:?}"),
    }

    send_message(stream, PeerMessage::Interested).context("sending interested")?;

    match receive_message(stream).context("waiting for unchoke")? {
        PeerMessage::UnChoke => (),
        message => bail!("expected a Unchoke but found a {message:?}"),
    }

    Ok(())
}

pub fn validate_piece(torrent: &Torrent, piece_index: usize, piece: &[u8]) -> anyhow::Result<()> {
    let slice_hash = torrent.info.pieces.0[piece_index];
    let current_hash: [u8; 20] = {
        let mut hasher = Sha1::new();
        hasher.update(piece);
        hasher.finalize().into()
    };

    if slice_hash != current_hash {
        bail!(
            "hashes don't match expected: {}, but found: {}",
            hex::encode(slice_hash),
            hex::encode(current_hash)
        )
    }

    Ok(())
}
