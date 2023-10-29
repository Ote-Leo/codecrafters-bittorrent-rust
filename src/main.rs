use std::env;
use std::io::Read;
use std::str::FromStr;

// Available if you need it!
// use serde_bencode

struct BenCodeParser<'a> {
    source: &'a [u8],
    idx: usize,
}

impl<'a> From<&'a str> for BenCodeParser<'a> {
    fn from(value: &'a str) -> Self {
        Self {
            source: value.as_bytes(),
            idx: 0,
        }
    }
}


impl<'a> From<&'a [u8]> for BenCodeParser<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self {
            source: value,
            idx: 0,
        }
    }
}

#[derive(Debug)]
struct ParseError;

impl<'a> BenCodeParser<'a> {
    fn parse(&mut self) -> Result<serde_json::Value, ParseError> {
        match self.source.get(self.idx) {
            Some(b'l') => {
                self.idx += 1;
                let mut arr = Vec::new();

                while let Ok(value) = self.parse() {
                    arr.push(value);
                }

                if let Some(b'e') = self.next() {
                    Ok(serde_json::Value::Array(arr))
                } else {
                    Err(ParseError)
                }
            }
            Some(b'd') => {
                self.idx += 1;
                let mut map = serde_json::Map::new();

                while let Ok(serde_json::Value::String(key)) = self.parse_string() {
                    let value = self.parse()?;
                    map.insert(key, value);
                }

                if let Some(b'e') = self.next() {
                    Ok(serde_json::Value::Object(map))
                } else {
                    Err(ParseError)
                }
            }
            Some(b'i') => self.parse_integer(),
            Some(num) if num.is_ascii_digit() => self.parse_string(),
            _ => Err(ParseError),
        }
    }

    fn parse_string(&mut self) -> Result<serde_json::Value, ParseError> {
        let source = &self.source[self.idx..];
        match source.get(0) {
            Some(num) if num.is_ascii_digit() => {
                let colon_index = source.iter().position(|&byte| byte == b':').unwrap();
                let number_string = std::str::from_utf8(&source[..colon_index]).unwrap();
                let length = number_string.parse::<u64>().unwrap();
                let start = colon_index + 1;
                let end = start + length as usize;
                let string = source[start..end].iter().map(|&byte| byte as char).collect::<String>();
                self.idx += end;
                Ok(serde_json::Value::String(string))
            }
            _ => Err(ParseError),
        }
    }

    fn parse_integer(&mut self) -> Result<serde_json::Value, ParseError> {
        let source = &self.source[self.idx..];
        match source.iter().position(|&byte| byte == b'e') {
            Some(end) => {
                let number_string = std::str::from_utf8(&source[1..end]).unwrap();
                serde_json::value::Number::from_str(number_string)
                    .map(|num| {
                        self.idx += end + 1;
                        serde_json::Value::Number(num)
                    })
                    .map_err(|_| ParseError)
            }
            _ => Err(ParseError),
        }
    }

    fn next(&mut self) -> Option<&u8> {
        let res = self.source.get(self.idx);
        self.idx += 1;
        res
    }
}

fn decode_bencoded_value(encoded_value: &str) -> serde_json::Value {
    BenCodeParser::from(encoded_value).parse().unwrap()
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
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{decoded_value}");
    } else if command == "info" {
        let mut file = std::fs::File::open(&args[2]).unwrap();
        let mut buf = Vec::new();
        let _ = file.read_to_end(&mut buf);
        let encoded_value = buf.iter().map(|byte| *byte as char).collect::<String>();
        let decoded_value = decode_bencoded_value(&encoded_value);

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
