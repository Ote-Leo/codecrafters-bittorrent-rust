use std::env;
use std::io::Read;

fn decode_bencoded_value(encoded_value: &[u8]) -> serde_json::Value {
    let bencode = serde_bencode::from_bytes(encoded_value).unwrap_or_else(|err| {
        panic!(
            "could decode input {}\n\n\t{err}\n\n",
            std::string::String::from_utf8_lossy(encoded_value)
        )
    });

    bencode_to_json(bencode)
}

fn bencode_to_json(bencode: serde_bencode::value::Value) -> serde_json::Value {
    use serde_bencode::value::Value;
    match bencode {
        Value::Bytes(bytes) => serde_json::Value::String(String::from_utf8_lossy(&bytes).into()),
        Value::Int(num) => serde_json::Value::Number(serde_json::value::Number::from(num)),
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
                let key = String::from_utf8_lossy(&key);
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
        // println!("Logs from your program will appear here!");

        // Uncomment this block to pass the first stage
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value.as_ref());
        println!("{decoded_value}");
    } else if command == "info" {
        let mut file = std::fs::File::open(&args[2]).unwrap();
        let mut buf = Vec::new();
        let _buf_length = file.read_to_end(&mut buf);
        let decoded_value = decode_bencoded_value(buf.as_ref());

        let announce = decoded_value.get("announce").unwrap();
        if let serde_json::Value::String(url) = announce {
            println!("Tracker URL: {url}");
        }
        let info = decoded_value.get("info").unwrap();
        let length = info.get("length").unwrap();
        if let serde_json::Value::Number(length) = length {
            println!("Length: {length}");
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
            assert_eq!(expected, output);
        }

        #[test]
        fn basic_url() {
            let input = "55:http://bittorrent-test-tracker.codecrafters.io/announce";
            let expected = json!("http://bittorrent-test-tracker.codecrafters.io/announce");
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, output);
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
            assert_eq!(expected, output);
        }

        #[test]
        fn positive_i64_integer() {
            let input = "i4294967300e";
            let expected = json!(4294967300i64);
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, output);
        }

        #[test]
        fn negative_i32_integer() {
            let input = "i-52e";
            let expected = json!(-52);
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, output);
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
            assert_eq!(expected, output);
        }

        #[test]
        fn linear_list() {
            let input = "l9:pineapplei261ee";
            let expected = json!(["pineapple", 261]);
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, output);

            let input = "li261e9:pineapplee";
            let expected = json!([261, "pineapple"]);
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, output);
        }

        #[test]
        fn nested_list() {
            let input = "lli4eei5ee";
            let expected = json!([[4], 5]);
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, output);
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
            assert_eq!(expected, output)
        }

        #[test]
        fn linear_dictionary() {
            let input = "d3:foo5:grape5:helloi52ee";
            let expected = json!({"foo":"grape","hello":52});
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, output);
        }

        #[test]
        fn nested_dictionary() {
            let input = "d10:inner_dictd4:key16:value14:key2i42e8:list_keyl5:item15:item2i3eeee";
            let expected =
                json!({"inner_dict":{"key1":"value1","key2":42,"list_key":["item1","item2",3]}});
            let output = decode_bencoded_value(input.as_ref());
            assert_eq!(expected, output);
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

            let decoded_value = decode_bencoded_value(buf.as_ref());

            let output_tracker = decoded_value.get("announce").unwrap();
            assert_eq!(expected_tracker, *output_tracker);

            let info = decoded_value.get("info").unwrap();
            let output_length = info.get("length").unwrap();
            assert_eq!(expected_length, *output_length);
        }
    }
}
