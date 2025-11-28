use std::{
    io::{BufReader, prelude::*},
    net::{TcpListener, TcpStream},
};

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        handle_connection(stream);
    }
}

const NOT_IMPLEMENTED: &str = "HTTP/1.1 501 Not Implemented\r\n\r\n";
const OK: &str = "HTTP/1.1 200 OK\r\n\r\n";

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

    println!("Request method: {request_method}");
    println!("Request path: {request_path}");
    println!("Request method: {request_http_version}");

    if request_http_version != "HTTP/1.1" {
        let response = NOT_IMPLEMENTED;
        stream.write_all(response.as_bytes()).unwrap();
        return;
    } 

    let response = match request_method {
        "GET" => match request_path {
            "/" => OK,
            _ => NOT_IMPLEMENTED
        },
        _ => NOT_IMPLEMENTED
    };

    stream.write_all(response.as_bytes()).unwrap();

    
}