#![allow(unused)]
use clap::{Parser, Subcommand};
use reqwest;
use serde_bencode::value::Value;
use sha1::{Digest, Sha1};
use std::{
    env,
    io::Read,
    path::{Path, PathBuf},
    str::FromStr,
};

use SubCommand::*;

fn decode_bencoded_value(encoded_value: &[u8]) -> serde_bencode::value::Value {
    serde_bencode::from_bytes(encoded_value).unwrap_or_else(|err| {
        panic!(
            "could decode input {}\n\n\t{err}\n\n",
            std::string::String::from_utf8_lossy(encoded_value)
        )
    })
}

fn bencode_to_json(bencode: &serde_bencode::value::Value) -> serde_json::Value {
    match bencode {
        Value::Bytes(bytes) => serde_json::Value::String(String::from_utf8_lossy(bytes).into()),
        Value::Int(num) => serde_json::Value::Number(serde_json::value::Number::from(*num)),
        Value::List(list) => {
            let mut arr = Vec::new();
            for elem in list {
                arr.push(bencode_to_json(elem));
            }
            serde_json::Value::Array(arr)
        }
        Value::Dict(dict) => {
            let mut map = serde_json::value::Map::new();

            for (key, value) in dict {
                let key = String::from_utf8_lossy(key);
                let value = bencode_to_json(value);
                map.insert(key.into(), value);
            }

            serde_json::Value::Object(map)
        }
    }
}

fn read_bin_file<P: AsRef<Path>>(path: P) -> Vec<u8> {
    std::fs::read(path).unwrap()
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

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    let cli = Cli::parse();

    match cli.command {
        Decode { bencode } => println!(
            "{}",
            bencode_to_json(&decode_bencoded_value(bencode.as_ref()))
        ),
        Info { file_path } => {
            let buf = read_bin_file(&file_path);

            let Value::Dict(meta) = decode_bencoded_value(&buf) else {
                panic!("No meta dict in torrent file");
            };

            let url = if let Some(Value::Bytes(url)) = meta.get("announce".as_bytes()) {
                String::from_utf8_lossy(url)
            } else {
                panic!("No urls in the torrent file")
            };

            println!("Tracker URL: {url}");
            let mut hasher = Sha1::new();
            let (info, info_hash) = if let Some(info) = meta.get("info".as_bytes()) {
                let info_bytes = serde_bencode::to_bytes(info).unwrap();
                hasher.update(info_bytes);
                let info_hash = hasher.finalize();

                if let Value::Dict(info) = info {
                    (info, info_hash)
                } else {
                    panic!("Fuck this shit")
                }
            } else {
                panic!("No info in torrent file");
            };

            let length = if let Some(Value::Int(length)) = info.get("length".as_bytes()) {
                length
            } else {
                panic!("No length in the torrent file");
            };
            println!("Length: {length}");

            println!(
                "Info Hash: {}",
                info_hash
                    .iter()
                    .map(|byte| format!("{byte:02x?}"))
                    .collect::<String>()
            );

            let piece_length =
                if let Value::Int(piece_length) = info.get("piece length".as_bytes()).unwrap() {
                    piece_length
                } else {
                    panic!("No piece length in info of torrent");
                };
            println!("Piece Length: {piece_length}");

            let pieces = if let Value::Bytes(pieces) = info.get("pieces".as_bytes()).unwrap() {
                pieces
            } else {
                panic!("No pieces in info of torrent");
            };

            println!("Piece Hashes:");
            for chunk in pieces.chunks(20) {
                let hash = chunk
                    .iter()
                    .map(|byte| format!("{byte:02x?}"))
                    .collect::<String>();
                println!("{hash}");
            }
        }
        Peers { file_path } => {
            let buf = read_bin_file(&args[2]);
            let decoded_value = decode_bencoded_value(buf.as_ref());

            let meta = if let Value::Dict(meta) = decoded_value {
                meta
            } else {
                panic!("No meta dict in torrent file");
            };

            let tracker_url = if let Some(Value::Bytes(url)) = meta.get("announce".as_bytes()) {
                String::from_utf8_lossy(url)
            } else {
                panic!("No tracker url in torrent file");
            };

            let mut hasher = Sha1::new();
            let (info, info_hash) = if let Some(info) = meta.get("info".as_bytes()) {
                let info_bytes = serde_bencode::to_bytes(info).unwrap();
                hasher.update(info_bytes);
                let info_hash = hasher.finalize();

                if let Value::Dict(info) = info {
                    (info, info_hash)
                } else {
                    panic!("Fuck this shit")
                }
            } else {
                panic!("No info in torrent file");
            };

            let length = if let Some(Value::Int(length)) = info.get("length".as_bytes()) {
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
}

#[cfg(test)]
mod bencode_decoding {
    use super::*;

    #[cfg(test)]
    mod strings {
        use serde_json::json;

        use super::*;

        #[test]
        fn basic_string() {
            let input = "6:orange";
            let expected = json!("orange");
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, bencode_to_json(&output));
        }

        #[test]
        fn basic_url() {
            let input = "55:http://bittorrent-test-tracker.codecrafters.io/announce";
            let expected = json!("http://bittorrent-test-tracker.codecrafters.io/announce");
            let output = decode_bencoded_value(input.as_ref());
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
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, bencode_to_json(&output));
        }

        #[test]
        fn positive_i64_integer() {
            let input = "i4294967300e";
            let expected = json!(4294967300i64);
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, bencode_to_json(&output));
        }

        #[test]
        fn negative_i32_integer() {
            let input = "i-52e";
            let expected = json!(-52);
            let output = decode_bencoded_value(input.as_ref());
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
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, bencode_to_json(&output));
        }

        #[test]
        fn linear_list() {
            let input = "l9:pineapplei261ee";
            let expected = json!(["pineapple", 261]);
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, bencode_to_json(&output));

            let input = "li261e9:pineapplee";
            let expected = json!([261, "pineapple"]);
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, bencode_to_json(&output));
        }

        #[test]
        fn nested_list() {
            let input = "lli4eei5ee";
            let expected = json!([[4], 5]);
            let output = decode_bencoded_value(input.as_ref());
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
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, bencode_to_json(&output))
        }

        #[test]
        fn linear_dictionary() {
            let input = "d3:foo5:grape5:helloi52ee";
            let expected = json!({"foo":"grape","hello":52});
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, bencode_to_json(&output));
        }

        #[test]
        fn nested_dictionary() {
            let input = "d10:inner_dictd4:key16:value14:key2i42e8:list_keyl5:item15:item2i3eeee";
            let expected =
                json!({"inner_dict":{"key1":"value1","key2":42,"list_key":["item1","item2",3]}});
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, bencode_to_json(&output));
        }
    }

    #[cfg(test)]
    mod torrent_file {
        use serde_json::json;

        use super::*;

        #[test]
        fn torrent_info() {
            let mut file = std::fs::File::open("sample.torrent").unwrap();
            let mut buf = Vec::new();
            let _buf_length = file.read_to_end(&mut buf);

            let expected_tracker = json!("http://bittorrent-test-tracker.codecrafters.io/announce");
            let expected_length = json!(92063);
            let expected_hash = "d69f91e6b2ae4c542468d1073a71d4ea13879a7f";

            let decoded_value = decode_bencoded_value(buf.as_ref());
            let decoded_json = bencode_to_json(&decoded_value);

            let output_tracker = decoded_json.get("announce").unwrap();
            assert_eq!(expected_tracker, *output_tracker);

            let info = decoded_json.get("info").unwrap();
            let output_length = info.get("length").unwrap();
            assert_eq!(expected_length, *output_length);

            use serde_bencode::value::Value;
            let mut hasher = Sha1::new();
            if let Value::Dict(meta) = decoded_value {
                if let Some(dict) = meta.get("info".as_bytes()) {
                    let bytes = serde_bencode::to_bytes(dict).unwrap();
                    hasher.update(bytes);
                    let output_hash = hasher.finalize();
                    let output_hash = output_hash
                        .iter()
                        .map(|byte| format!("{byte:02x?}"))
                        .collect::<String>();
                    assert_eq!(expected_hash, output_hash);
                }
            }
        }
    }
}
