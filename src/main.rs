use std::{
    io::{BufReader, prelude::*},
    net::{TcpListener, TcpStream}, thread, time::Duration,
};

use::basic_auth::ThreadPool;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    let pool = ThreadPool::new(4);

    for stream in listener.incoming() {
        let stream = stream.unwrap();

         pool.execute(|| {
            handle_connection(stream);
        });
    }
}

const NOT_IMPLEMENTED: &str = "HTTP/1.1 501 Not Implemented";
const OK: &str = "HTTP/1.1 200 OK";

fn empty_response(status_line: &str) -> String {
    return format!(
            "{status_line}\r\nContent-Length: 0\r\n\r\n"
        );
}

fn body_response(status_line: &str, contents: &str) -> String {
    let length = contents.len();
    return format!(
            "{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}"
        );
    }

fn handle_connection(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.unwrap())
        .take_while(|line| !line.is_empty())
        .collect();

    println!("Request: {http_request:#?}");

    let request_first_line = &http_request[0];
    let first_line_parts = request_first_line
        .split_whitespace()
        .collect::<Vec<&str>>();

    let request_method = first_line_parts[0];
    let request_path = first_line_parts[1];
    let request_http_version = first_line_parts[2];

    if request_http_version != "HTTP/1.1" {
        let response = empty_response(NOT_IMPLEMENTED);
        stream.write_all(response.as_bytes()).unwrap();
        return;
    } 

    let response = match request_method {
        "GET" => match request_path {
            "/" => empty_response(OK),
            "/sleep" => {
                thread::sleep(Duration::from_secs(5));
                empty_response(OK)
            }
            _ => empty_response(NOT_IMPLEMENTED)
        },
        _ => empty_response(NOT_IMPLEMENTED)
    };

    stream.write_all(response.as_bytes()).unwrap();

    
}