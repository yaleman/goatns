use std::str::from_utf8;

use url::Url;

use crate::utils::{find_tail_match, loc_size_to_u8, name_as_bytes};
use std::thread::sleep;
use std::time::Duration;

/// Test function to keep checking the server for startup
pub async fn wait_for_server(status_url: Url) {
    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    for i in 0..10 {
        match client.get(status_url.clone()).send().await {
            Ok(value) => {
                eprintln!("OK: {value:?}");
                if let Ok(text) = value.text().await {
                    eprintln!("Server response: {text}");
                    if text == crate::web::STATUS_OK.to_string() {
                        println!("API is up!");
                        break;
                    }
                }
            }
            Err(err) => eprintln!("ERR: {err:?}"),
        }
        sleep(Duration::from_secs(1));
        if i == 9 {
            panic!("Couldn't connect to test server after 10 seconds!");
        }
    }
}

#[test]
fn test_loc_size_to_u8() {
    assert_eq!(loc_size_to_u8(10.0), 0x13);
    assert_eq!(loc_size_to_u8(100.0), 0x14);
    eprintln!("testing 90000000.0 = 0x99");
    assert_eq!(loc_size_to_u8(90000000.0), 0x99);
    eprintln!("{:3x}", loc_size_to_u8(20000000.0));
}

#[test]
pub fn test_find_tail_match() {
    let name = "foo.example.com".as_bytes().to_vec();
    let target = "zot.example.com".as_bytes().to_vec();
    let result = find_tail_match(&name, &target);

    assert_eq!(result, 3);
    let name = "foo.yeanah.xyz".as_bytes().to_vec();
    let target = "zot.example.com".as_bytes().to_vec();
    let result = find_tail_match(&name, &target);

    assert_eq!(result, 0)
}

#[test]
pub fn test_name_bytes_simple_compress() {
    let expected_result: Vec<u8> = vec![192, 12];

    let test_result = name_as_bytes("example.com".as_bytes().to_vec(), Some(12), None);
    assert_eq!(expected_result, test_result);
}
#[test]
pub fn test_name_bytes_no_compress() {
    let expected_result: Vec<u8> = vec![7, 101, 120, 97, 109, 112, 108, 101, 3, 99, 111, 109, 0];

    let test_result = name_as_bytes("example.com".as_bytes().to_vec(), None, None);
    assert_eq!(expected_result, test_result);
}

#[test]
pub fn test_name_bytes_with_compression() {
    let example_com = "example.com".as_bytes().to_vec();
    let test_input = "lol.example.com".as_bytes().to_vec();

    let expected_result: Vec<u8> = vec![3, 108, 111, 108, 192, 12];

    println!("{:?}", from_utf8(&example_com));
    println!("{:?}", from_utf8(&test_input));

    let result = name_as_bytes(test_input, Some(12), Some(&example_com));

    assert_eq!(result, expected_result);
}

#[test]
pub fn test_name_bytes_with_tail_compression() {
    let example_com = "ns1.example.com".as_bytes().to_vec();
    let test_input = "lol.example.com".as_bytes().to_vec();

    let expected_result: Vec<u8> = vec![3, 108, 111, 108, 192, 16];

    println!("{:?}", from_utf8(&example_com));
    println!("{:?}", from_utf8(&test_input));

    let result = name_as_bytes(test_input, Some(12), Some(&example_com));

    assert_eq!(result, expected_result);
}
