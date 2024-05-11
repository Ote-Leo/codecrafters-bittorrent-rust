use anyhow::Context;
use clap::{Parser, Subcommand};
use pieces::Pieces;
use serde::{Deserialize, Serialize};
use serde_bencode::value::Value as BenValue;
use serde_json::Value as JsonValue;
use sha1::{Digest, Sha1};
use std::{fs::read, path::PathBuf, str::FromStr};

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

fn url_encode_infohash(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| {
            if byte.is_ascii_digit()
                || byte.is_ascii_uppercase()
                || byte.is_ascii_lowercase()
                || byte == b'-'
                || byte == b'_'
                || byte == b'.'
                || byte == b'~'
            {
                format!("{}", byte as char)
            } else {
                format!("%{byte:02x?}")
            }
        })
        .collect::<String>()
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Torrent {
    // TODO: using a proper url
    /// The URL of the tracker.
    announce: String,

    info: Info,
}

impl Torrent {
    /// Calculate the total number of bytes for this torrent
    fn content_length(&self) -> usize {
        match self.info.content {
            Content::SingleFile { length } => length,
            Content::MultiFile { ref files } => files.iter().map(|file| file.length).sum(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Info {
    /// The suggested name to save the file (or directory) as. It is purely advisory.
    ///
    /// In the single file case, the name key is the name of a file, in the muliple file case, it's
    /// the name of a directory.
    name: String,

    /// The number of bytes in each piece the file is split into. For the purposes of transfer,
    /// files are split into fixed-size pieces which are all the same length except for possibly
    /// the last one which may be truncated. `piece_length` is almost always a power of two, most
    /// commonly $2^{18} = 256K$ (BitTorrent prior to version 3.2 uses $2^{20} = 1M$ as default).
    #[serde(rename = "piece length")]
    piece_length: usize,

    /// A bytestring whose length is a multiple of 20. It is to be subdivided into strings of
    /// length 20, each of which is the SHA1 hash of the piece at the corresponding index.
    pieces: Pieces,

    /// There is also a key length or a key files, but not both or neither. otherwise it represents
    /// a set of files which go in a directory structure.
    #[serde(flatten)]
    content: Content,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum Content {
    /// The `length` of the file in bytes
    SingleFile { length: usize },

    /// For the purposes of the other keys in the [`Info`], the multi-file case is treated as only having a single
    /// file by concatenating the files in the order they appear in the files list.
    MultiFile { files: Vec<TorrentFile> },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct TorrentFile {
    ///  The length of the file, in bytes.
    length: usize,

    /// A list of UTF-8 encoded strings corresponding to subdirectory names, the last of which is
    /// the actual file name (a zero length list is an error case).
    path: Vec<String>,
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

fn calculate_info_hash(torrent: &Torrent) -> anyhow::Result<Vec<u8>> {
    let info_bytes =
        serde_bencode::to_bytes(&torrent.info).expect("guaranteed to be a valid bencode");
    let mut hasher = Sha1::new();
    hasher.update(&info_bytes);
    Ok(hasher.finalize().to_vec())
}

fn render_torrent_info(torrent: &Torrent) -> anyhow::Result<()> {
    let info_hash = calculate_info_hash(torrent).context("calculating info hash")?;

    println!("Tracker URL: {}", torrent.announce);
    println!("Length: {}", torrent.content_length());
    println!("Info Hash: {}", hex::encode(info_hash));
    println!("Piece Length: {}", torrent.info.piece_length);
    println!("Piece Hashes:");
    for piece in torrent.info.pieces.0.iter() {
        println!("{}", hex::encode(piece))
    }

    Ok(())
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
            render_torrent_info(&torrent)?;
        }
        SubCommand::Peers { file_path } => {
            let buf = read(file_path).unwrap();
            let decoded_value = decode_bencoded_value(buf);

            let meta = if let BenValue::Dict(meta) = decoded_value {
                meta
            } else {
                panic!("No meta dict in torrent file");
            };

            let tracker_url = if let Some(BenValue::Bytes(url)) = meta.get("announce".as_bytes()) {
                String::from_utf8_lossy(url)
            } else {
                panic!("No tracker url in torrent file");
            };

            let mut hasher = Sha1::new();
            let (info, info_hash) = if let Some(info) = meta.get("info".as_bytes()) {
                let info_bytes = serde_bencode::to_bytes(info).unwrap();
                hasher.update(info_bytes);
                let info_hash = hasher.finalize();

                if let BenValue::Dict(info) = info {
                    (info, info_hash)
                } else {
                    panic!("Fuck this shit")
                }
            } else {
                panic!("No info in torrent file");
            };

            let length = if let Some(BenValue::Int(length)) = info.get("length".as_bytes()) {
                length
            } else {
                panic!("No length in torrent file");
            };

            let info_hash_url = url_encode_infohash(&info_hash);
            let peer_id = "36525524767213958416";
            let port = 6881;
            let uploaded = 0;
            let downloaded = 0;
            let left = length;
            let compact = 1;

            let query = [
                ("peer_id", "36525524767213958416".into()),
                ("downloaded", "0".into()),
                ("uploaded", "0".into()),
                ("left", length.to_string()),
                ("compact", "1".into()),
            ];

            let mut url = reqwest::Url::from_str(&tracker_url).unwrap();
            url.set_port(Some(port)).unwrap();

            let client = reqwest::blocking::Client::new();
            // let request = client
            //     .get(url)
            //     .query(&query)
            //     .query(&[("info_hash", &info_hash)]);
            //
            // println!("{request:#?}");
            //
            // if let Ok(response) = request.send() {
            //     println!("{response:?}");
            // }
        }
    }

    Ok(())
}

mod pieces {
    use serde::{
        de::{self, Visitor},
        Deserialize, Deserializer, Serialize,
    };
    use std::fmt;

    #[derive(Debug, PartialEq, Eq)]
    pub struct Pieces(pub Vec<[u8; 20]>);
    struct PiecesVisitor;

    impl<'de> Visitor<'de> for PiecesVisitor {
        type Value = Pieces;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "a byte string whose length is a multiple of 20")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.len() % 20 != 0 {
                return Err(E::custom(format!(
                    "length is {length}, {length} mod 20 = {remainder}",
                    length = v.len(),
                    remainder = v.len() % 20
                )));
            }

            // TODO: use [`std::slice::array_chunks`] when stable
            Ok(Pieces(
                v.chunks_exact(20)
                    .map(|slice| slice.try_into().expect("guaranteed to be divisible by 20"))
                    .collect(),
            ))
        }
    }

    impl<'de> Deserialize<'de> for Pieces {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(PiecesVisitor)
        }
    }

    impl Serialize for Pieces {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let hashes = self.0.concat();
            serializer.serialize_bytes(&hashes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    mod torrent_file {
        use super::*;
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
