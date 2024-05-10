#![allow(unused)]
use anyhow::Context;
use clap::{Parser, Subcommand};
use pieces::Pieces;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_bencode::value::Value as BenValue;
use serde_json::Value as JsonValue;
use sha1::{Digest, Sha1};
use std::{fs::read, path::PathBuf, str::FromStr};

use SubCommand::*;

fn decode_bencoded_value<B: AsRef<[u8]>>(encoded_value: B) -> BenValue {
    serde_bencode::from_bytes(encoded_value.as_ref()).expect("failed to deserialize bencode")
}

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
            if (b'0' <= byte && byte <= b'9')
                || (b'A' <= byte && byte <= b'Z')
                || (b'a' <= byte && byte <= b'z')
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Content {
    /// The `length` of the file in bytes
    SingleFile { length: usize },

    /// For the purposes of the other keys in the [`Info`], the multi-file case is treated as only having a single
    /// file by concatenating the files in the order they appear in the files list.
    MultiFile { files: Vec<TorrentFile> },
}

#[derive(Debug, Serialize, Deserialize)]
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
    println!("Tracker URL: {}", torrent.announce);
    println!("Length: {}", torrent.content_length());

    let info_bytes = serde_bencode::to_bytes(&torrent.info).context("re-encode info section")?;
    let info_hash = calculate_info_hash(&torrent).context("calculating info hash")?;

    println!(
        "Info Hash: {}",
        info_hash
            .iter()
            .map(|byte| format!("{byte:02x?}"))
            .collect::<String>()
    );

    println!("Piece Length: {}", torrent.info.piece_length);
    let piece_hashes = torrent
        .info
        .pieces
        .0
        .iter()
        .map(|chunk| {
            chunk
                .iter()
                .map(|byte| format!("{byte:02x?}"))
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    println!("Piece Hashes:");
    println!("{}", piece_hashes.join("\n"));
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Decode { bencode } => {
            let value =
                serde_bencode::from_str::<BenValue>(&bencode).context("bencode decoding")?;
            println!("{}", bencode_to_json(&value));
        }
        SubCommand::Info { file_path } => {
            let buf = read(&file_path).context("opening torrent file")?;
            let torrent: Torrent = serde_bencode::from_bytes(&buf).context("parse torrent file")?;
            render_torrent_info(&torrent)?;
        }
        Peers { file_path } => {
            let buf = read(file_path).unwrap();
            let decoded_value = decode_bencoded_value(&buf);

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

    #[derive(Debug)]
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
    mod decoding {
        use super::*;

        #[cfg(test)]
        mod strings {
            use serde_json::json;

            use super::*;

            #[test]
            fn basic_string() {
                let input = "6:orange";
                let expected = json!("orange");
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }

            #[test]
            fn basic_url() {
                let input = "55:http://bittorrent-test-tracker.codecrafters.io/announce";
                let expected = json!("http://bittorrent-test-tracker.codecrafters.io/announce");
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }
        }

        #[cfg(test)]
        mod integers {
            use serde_json::json;

            use super::*;

            #[test]
            fn positive_i32_integer() {
                let input = "i1249266168e";
                let expected = json!(1249266168);
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }

            #[test]
            fn positive_i64_integer() {
                let input = "i4294967300e";
                let expected = json!(4294967300i64);
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }

            #[test]
            fn negative_i32_integer() {
                let input = "i-52e";
                let expected = json!(-52);
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }
        }

        #[cfg(test)]
        mod lists {
            use serde_json::json;

            use super::*;

            #[test]
            fn empty_list() {
                let input = "le";
                let expected = json!([]);
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }

            #[test]
            fn linear_list() {
                let input = "l9:pineapplei261ee";
                let expected = json!(["pineapple", 261]);
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));

                let input = "li261e9:pineapplee";
                let expected = json!([261, "pineapple"]);
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }

            #[test]
            fn nested_list() {
                let input = "lli4eei5ee";
                let expected = json!([[4], 5]);
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }
        }

        #[cfg(test)]
        mod dictionaries {
            use serde_json::json;

            use super::*;

            #[test]
            fn empty_dictionary() {
                let input = "de";
                let expected = json!({});
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output))
            }

            #[test]
            fn linear_dictionary() {
                let input = "d3:foo5:grape5:helloi52ee";
                let expected = json!({"foo":"grape","hello":52});
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }

            #[test]
            fn nested_dictionary() {
                let input =
                    "d10:inner_dictd4:key16:value14:key2i42e8:list_keyl5:item15:item2i3eeee";
                let expected = json!({"inner_dict":{"key1":"value1","key2":42,"list_key":["item1","item2",3]}});
                let output = decode_bencoded_value(input);
                assert_eq!(expected, bencode_to_json(&output));
            }
        }
    }

    #[cfg(test)]
    mod torrent_file {
        use serde_json::json;
        use std::fs::read;

        use super::*;

        #[test]
        fn torrent_info() {
            let buf = read("sample.torrent").unwrap();
            let torrent: Torrent = serde_bencode::from_bytes(&buf).unwrap();

            let expected_tracker = "http://bittorrent-test-tracker.codecrafters.io/announce";
            let expected_length = 92063;
            let expected_hash = "d69f91e6b2ae4c542468d1073a71d4ea13879a7f";

            let info_hash_str = calculate_info_hash(&torrent)
                .unwrap()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();

            assert_eq!(expected_tracker, torrent.announce);
            assert_eq!(expected_length, torrent.content_length());
            assert_eq!(info_hash_str, expected_hash);
        }
    }
}
