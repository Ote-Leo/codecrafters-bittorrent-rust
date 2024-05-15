use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use serde_bencode::value::Value as BenValue;
use serde_json::Value as JsonValue;
use std::{
    fs::{read, File},
    io::{Read, Write},
    net::{SocketAddrV4, TcpStream},
    path::PathBuf,
};

use bittorrent_starter_rust::{
    peer::{
        download_piece, initiate_download, send_message, validate_piece, HandShake, PeerMessage,
    },
    torrent::Torrent,
    tracker::{Peers, TrackerRequest, TrackerResponse},
};

const BLOCK_SIZE: u32 = 1 << 14;

fn bencode_to_json(bencode: &BenValue) -> JsonValue {
    // TODO: find a way to make this work
    // serde_json::to_value(&bencode).expect("failed to json serialize bencode")

    use BenValue::*;

    match bencode {
        Bytes(bytes) => JsonValue::String(String::from_utf8_lossy(bytes).into()),
        Int(num) => JsonValue::Number(serde_json::value::Number::from(*num)),
        List(list) => {
            let mut arr = Vec::new();
            for elem in list {
                arr.push(bencode_to_json(elem));
            }
            JsonValue::Array(arr)
        }
        Dict(dict) => {
            let mut map = serde_json::value::Map::new();

            for (key, value) in dict {
                let key = String::from_utf8_lossy(key);
                let value = bencode_to_json(value);
                map.insert(key.into(), value);
            }

            JsonValue::Object(map)
        }
    }
}

fn urlencode<B: AsRef<[u8]>>(bytes: B) -> String {
    let mut result = String::with_capacity(3 * bytes.as_ref().len());
    for &byte in bytes.as_ref() {
        result.push('%');
        result.push_str(&hex::encode([byte]));
    }
    result
}

fn extract_peers(torrent: &Torrent, info_hash: Option<[u8; 20]>) -> anyhow::Result<Peers> {
    let tracker_url = {
        let announce = &torrent.announce;
        let info_hash_url = urlencode(info_hash.unwrap_or_else(|| torrent.calculate_info_hash()));
        let tracker_request = TrackerRequest::new(torrent.content_length());
        let tracker_request =
            serde_urlencoded::to_string(tracker_request).context("url-encoding tracker")?;
        format!("{announce}?{tracker_request}&info_hash={info_hash_url}")
    };

    let response = reqwest::blocking::get(tracker_url)
        .context("tracker get request")?
        .bytes()
        .context("reading response bytes")?;
    let response: TrackerResponse =
        serde_bencode::from_bytes(&response).context("bendecoding response")?;

    Ok(response.peers)
}

type PeerId = [u8; 20];

fn establish_handshake(
    torrent: &Torrent,
    peer: &SocketAddrV4,
    info_hash: Option<[u8; 20]>,
) -> anyhow::Result<(TcpStream, PeerId)> {
    let mut stream = TcpStream::connect(peer).context("establishing connection with peer")?;
    let handshake = HandShake::new(info_hash.unwrap_or_else(|| torrent.calculate_info_hash()));

    let mut bytes: [u8; 68] = handshake.into();
    stream.write_all(&bytes).context("sending handshake")?;
    stream.read(&mut bytes).context("receiving handshake")?;
    let handshake: HandShake = bytes.try_into().context("converting handshake")?;
    Ok((stream, handshake.peer_id))
}

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: SubCommand,
}

#[derive(Debug, Subcommand)]
#[clap(rename_all = "snake_case")]
enum SubCommand {
    /// Decode becoded data into json
    Decode {
        /// The bencoded data
        bencode: String,
    },
    /// Extract torrent file info
    Info {
        /// Path to the torrent file
        file_path: PathBuf,
    },
    /// Extract torrent file peers
    Peers {
        /// Path to the torrent file
        file_path: PathBuf,
    },
    /// Establish a peer handshake for a given torrent file
    #[clap(name = "handshake")]
    HandShake {
        /// Path to the torrent file
        file_path: PathBuf,
        /// Add of the peer
        peer: SocketAddrV4,
    },
    /// Download a specific piece from a torrent
    DownloadPiece {
        /// Path to place the piece in
        #[clap(short, long)]
        output: PathBuf,
        /// Path to the torrent file
        file_path: PathBuf,
        /// Piece index to download
        piece_index: usize,
    },
    /// Download a  torrent
    Download {
        /// Path to place the torrent
        #[clap(short, long)]
        output: PathBuf,
        /// Path to the torrent file
        file_path: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        SubCommand::Decode { bencode } => {
            let value =
                serde_bencode::from_str::<BenValue>(&bencode).context("bencode decoding")?;
            println!("{}", bencode_to_json(&value));
        }
        SubCommand::Info { file_path } => {
            let buf = read(file_path).context("opening torrent file")?;
            let torrent: Torrent = serde_bencode::from_bytes(&buf).context("parse torrent file")?;
            println!("{torrent}");
        }
        SubCommand::Peers { file_path } => {
            let buf = read(file_path).context("opening torrent file")?;
            let torrent: Torrent = serde_bencode::from_bytes(&buf).context("parse torrent file")?;

            for peer in extract_peers(&torrent, None)?.0.iter() {
                println!("{peer}");
            }
        }
        SubCommand::HandShake { file_path, peer } => {
            let buf = read(file_path).context("opening torrent file")?;
            let torrent: Torrent = serde_bencode::from_bytes(&buf).context("parse torrent file")?;
            let (_, peer_id) = establish_handshake(&torrent, &peer, None)?;
            println!("Peer ID: {}", hex::encode(peer_id));
        }
        SubCommand::DownloadPiece {
            output,
            file_path,
            piece_index,
        } => {
            let buf = read(file_path).context("opening torrent file")?;
            let torrent: Torrent = serde_bencode::from_bytes(&buf).context("parse torrent file")?;

            let pieces_count = torrent.info.pieces.0.len();
            if piece_index >= pieces_count {
                bail!("index {piece_index} out of {pieces_count}")
            }

            let info_hash = torrent.calculate_info_hash();
            let mut peers = extract_peers(&torrent, Some(info_hash.clone()))?;
            // TODO: pick peers in smarter way
            let Some(peer) = peers.0.pop() else {
                bail!("the torrent doesn't have any peers")
            };

            let (mut stream, _) = establish_handshake(&torrent, &peer, Some(info_hash))?;

            initiate_download(&mut stream)?;
            let piece = download_piece(&mut stream, &torrent, piece_index, BLOCK_SIZE)?;
            validate_piece(&torrent, piece_index, &piece)?;

            // saving to disk
            let mut piece_file = File::create(&output).context("creating output file")?;
            piece_file
                .write_all(&piece)
                .context("writing piece to file")?;

            println!(
                "Piece {piece_index} downloaded to {}.",
                output.as_path().display()
            );
        }
        SubCommand::Download { output, file_path } => {
            let buf = read(&file_path).context("opening torrent file")?;
            let torrent: Torrent = serde_bencode::from_bytes(&buf).context("parse torrent file")?;

            let info_hash = torrent.calculate_info_hash();
            let mut peers = extract_peers(&torrent, Some(info_hash.clone()))?;
            // TODO: pick peers in smarter way
            let Some(peer) = peers.0.pop() else {
                bail!("the torrent doesn't have any peers")
            };
            let mut file = File::create(&output).context("creating output file")?;

            // TODO : propbably some async 😅
            for piece_index in 0..torrent.info.pieces.0.len() {
                let (mut stream, _) = establish_handshake(&torrent, &peer, Some(info_hash))?;
                initiate_download(&mut stream)?;
                let piece = download_piece(&mut stream, &torrent, piece_index, BLOCK_SIZE)?;
                validate_piece(&torrent, piece_index, &piece)?;
                file.write_all(&piece)
                    .context(format!("writing piece {piece_index} to file"))?;
                send_message(
                    &mut stream,
                    PeerMessage::Have {
                        piece_index: piece_index as u32,
                    },
                )?;
            }

            println!(
                "Downloaded {} to {}.",
                file_path.display(),
                output.as_path().display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    mod torrent_file {
        use bittorrent_starter_rust::torrent::{Content, Info, Pieces, Torrent};
        use std::fs::read;

        #[test]
        fn torrent_info() {
            let buf = read("sample.torrent").unwrap();
            let torrent: Torrent = serde_bencode::from_bytes(&buf).unwrap();

            let expected_torrent = Torrent {
                announce: "http://bittorrent-test-tracker.codecrafters.io/announce".to_string(),
                info: Info {
                    name: "sample.txt".to_string(),
                    piece_length: 32768,
                    pieces: Pieces(vec![
                        [
                            232, 118, 246, 122, 42, 136, 134, 232, 243, 107, 19, 103, 38, 195, 15,
                            162, 151, 3, 2, 45,
                        ],
                        [
                            110, 34, 117, 230, 4, 160, 118, 102, 86, 115, 110, 129, 255, 16, 181,
                            82, 4, 173, 141, 53,
                        ],
                        [
                            240, 13, 147, 122, 2, 19, 223, 25, 130, 188, 141, 9, 114, 39, 173, 158,
                            144, 154, 204, 23,
                        ],
                    ]),
                    content: Content::SingleFile { length: 92063 },
                },
            };

            assert_eq!(torrent, expected_torrent);
        }
    }
}
