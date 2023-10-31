use sha1::{Digest, Sha1};
use std::env;
use std::io::Read;

fn decode_bencoded_value(encoded_value: &[u8]) -> serde_bencode::value::Value {
    serde_bencode::from_bytes(encoded_value).unwrap_or_else(|err| {
        panic!(
            "could decode input {}\n\n\t{err}\n\n",
            std::string::String::from_utf8_lossy(encoded_value)
        )
    })
}

fn bencode_to_json(bencode: &serde_bencode::value::Value) -> serde_json::Value {
    use serde_bencode::value::Value;
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

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        // You can use print statements as follows for debugging, they'll be visible when running tests.
        eprintln!("Logs from your program will appear here!");

        // Uncomment this block to pass the first stage
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value.as_ref());
        let decoded_json = bencode_to_json(&decoded_value);
        println!("{decoded_json}");
    } else if command == "info" {
        let mut file = std::fs::File::open(&args[2]).unwrap();
        let mut buf = Vec::new();
        let _buf_length = file.read_to_end(&mut buf);
        let decoded_value = decode_bencoded_value(buf.as_ref());
        let decoded_json = bencode_to_json(&decoded_value);

        let announce = decoded_json.get("announce").unwrap();
        if let serde_json::Value::String(url) = announce {
            println!("Tracker URL: {url}");
        }
        let info = decoded_json.get("info").unwrap();
        let length = info.get("length").unwrap();
        if let serde_json::Value::Number(length) = length {
            println!("Length: {length}");
        }

        use serde_bencode::value::Value;
        let mut hasher = Sha1::new();
        if let Value::Dict(meta) = decoded_value {
            if let Some(dict) = meta.get("info".as_bytes()) {
                let bytes = serde_bencode::to_bytes(dict).unwrap();
                hasher.update(bytes);
                let result = hasher.finalize();
                let result = result
                    .iter()
                    .map(|byte| format!("{byte:02x?}"))
                    .collect::<String>();
                println!("Info Hash: {result}");

                if let Value::Dict(dict) = dict {
                    if let Value::Int(piece_length) = dict.get("piece length".as_bytes()).unwrap() {
                        println!("Piece Length: {piece_length}");
                        if let Value::Bytes(pieces) = dict.get("pieces".as_bytes()).unwrap() {
                            println!("Piece Hashes:");
                            for chunk in pieces.chunks(20) {
                                let hash = chunk.iter().map(|byte| format!("{byte:02x?}")).collect::<String>();
                                println!("{hash}");
                            }
                        }
                    }
                }
            }
        }
    } else {
        println!("unknown command: {}", args[1])
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
