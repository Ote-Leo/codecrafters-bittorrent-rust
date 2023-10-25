use serde_json;
use std::env;
use std::str::FromStr;

// Available if you need it!
// use serde_bencode

fn decode_bencode_string(encoded_value: &str) -> serde_json::Value {
    let colon_index = encoded_value.find(':').unwrap();
    let number_string = &encoded_value[..colon_index];
    let length = number_string.parse::<i64>().unwrap();
    let string = &encoded_value[colon_index + 1..colon_index + 1 + length as usize];
    serde_json::Value::String(string.to_string())
}

fn decode_bencode_integer(encoded_value: &str) -> serde_json::Value {
    if let Some(end_index) = encoded_value.find('e') {
        let number_string = &encoded_value[1..end_index];

        if number_string.len() == 0 {
            panic!("Empty number string {}", encoded_value)
        } else if number_string.starts_with("0") && number_string.len() != 1 {
            panic!("Numbers cannot start with a leading zero: {}", encoded_value)
        } else if number_string.starts_with("-0") {
            panic!("Illegal starting pattern `-0`: {}", encoded_value)
        } else if number_string.contains('.') {
            panic!("floatind point: {}", encoded_value)
        }

        match serde_json::value::Number::from_str(number_string) {
            Ok(number) => serde_json::Value::Number(number),
            err => panic!("Couldn't parse {} as a number; due to {:#?}", number_string, err)
        }
    } else {
        panic!("input {} doesn't end with an `e`", encoded_value)
    }
}


fn decode_bencoded_value(encoded_value: &str) -> serde_json::Value {
    match encoded_value.chars().next() {
        Some(number) if number.is_digit(10) => decode_bencode_string(encoded_value),
        Some('i') => decode_bencode_integer(encoded_value),
        Some(_) => panic!("Unhandled encoded value: {}", encoded_value),
        None => panic!("Empty String")
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
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }
}
