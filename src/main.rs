use serde_json;
use serde_json::json;
use std::env;
use std::str::FromStr;

// Available if you need it!
// use serde_bencode

struct BenCodeParser<'a> {
    source: &'a str,
    idx: usize,
}

impl<'a> From<&'a str> for BenCodeParser<'a> {
    fn from(value: &'a str) -> Self {
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
        match self.source.chars().nth(self.idx) {
            Some('l') => {
                self.idx += 1;
                let mut arr = Vec::new();

                while let Ok(value) = self.parse() {
                    arr.push(value);
                }

                if let Some('e') = self.next() {
                    Ok(serde_json::Value::Array(arr))
                } else {
                    Err(ParseError)
                }
            },
            Some('d') => todo!(),
            Some('i') => self.parse_integer(),
            Some(num) if num.is_digit(10) => self.parse_string(),
            _ => Err(ParseError),
        }
    }

    fn parse_string(&mut self) -> Result<serde_json::Value, ParseError> {
        let source = &self.source[self.idx..];
        match source.chars().nth(0) {
            Some(num) if num.is_digit(10) => {
                let colon_index = source.find(':').unwrap();
                let number_string = &source[..colon_index];
                let length = number_string.parse::<u64>().unwrap();
                let start = colon_index + 1;
                let end = start + length as usize;
                let string = &source[start..end];
                self.idx += end;
                Ok(serde_json::Value::String(string.to_string()))
            }
            _ => Err(ParseError),
        }
    }

    fn parse_integer(&mut self) -> Result<serde_json::Value, ParseError> {
        let source = &self.source[self.idx..];
        match source.find('e') {
            Some(end) => {
                let number_string = &source[1..end];
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

    fn next(&mut self) -> Option<char> {
        let res = self.source.chars().nth(self.idx);
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
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }
}
