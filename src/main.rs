use anyhow::Context;
use clap::{Parser, Subcommand};
use serde_bencode::value::Value as BenValue;
use serde_json::Value as JsonValue;
use std::{fs::read, path::PathBuf};

use bittorrent_starter_rust::{
    torrent::Torrent,
    tracker::{TrackerRequest, TrackerResponse},
};

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

fn urlencode(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(3 * bytes.len());
    for &byte in bytes {
        result.push('%');
        result.push_str(&hex::encode(&[byte]));
    }
    result
}

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: SubCommand,
}

#[derive(Debug, Subcommand)]
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

            let tracker_url = &torrent.announce;
            let info_hash = torrent.calculate_info_hash();
            let length = torrent.content_length();

            let tracker_url = {
                let info_hash_url = urlencode(&info_hash);
                let tracker_request = TrackerRequest::new(length);
                let tracker_request = serde_urlencoded::to_string(&tracker_request)
                    .context("url-encoding tracker")?;
                format!("{tracker_url}?{tracker_request}&info_hash={info_hash_url}")
            };

            let response = reqwest::blocking::get(tracker_url).context("tracker get request")?;
            let response = response.bytes().context("reading response bytes")?;
            let response: TrackerResponse =
                serde_bencode::from_bytes(&response).context("bendecoding response")?;
            for peer in response.peers.0.iter() {
                println!("{peer}");
            }
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
