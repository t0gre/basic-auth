use std::{
    env,
    io::{BufReader, prelude::*},
    net::{TcpListener, TcpStream}, thread, time::Duration,
};

use base64::prelude::*;

use argon2::{
    password_hash::{
        rand_core::OsRng,
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString
    },
    Argon2
};

use::basic_auth::ThreadPool;
use sqlite::Connection;

struct User {
    userid: String,
    password: String
}

const NOT_IMPLEMENTED: &str = "HTTP/1.1 501 Not Implemented";
const OK: &str = "HTTP/1.1 200 OK";
const UNAUTHORIZED: &str = "HTTP/1.1 401 Unauthorized";


fn upsert_user(user: User, connection: &Connection) {
     let userid = user.userid;
     let password = user.password;

     let salt = SaltString::generate(&mut OsRng);
     let argon2 = Argon2::default();

     let password_hash = argon2.hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string();

     let query = format!("
        INSERT INTO users(userid) VALUES('{userid}')
            ON CONFLICT(userid) DO UPDATE SET password='{password_hash}';
        ");

    connection.execute(query).unwrap();
}

fn check_password(user: User, connection: &Connection) -> bool {
     let userid = user.userid;
     let password = user.password;


     let query = format!("
        SELECT userid, password FROM users WHERE userid='{userid}'"
        );

    
    let mut stored_password_hash = String::new();

    connection.iterate(query, |row| {
        for &(column, val) in row.iter() {
            if column == "password" {
                stored_password_hash = val.unwrap().to_string()
            }
        }
        true
    }
    ).unwrap();

    let parsed_hash = PasswordHash::new(&stored_password_hash).unwrap();
    Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok()
}

fn is_admin(user: User, connection: &Connection) -> bool {

    let userid = user.userid;

    let query = format!("
        SELECT role FROM users WHERE userid='{userid}'"
    );

    let mut is_admin = false;

    connection.iterate(query, |row| {
        for &(column, val) in row.iter() {
            if column == "role" && val.unwrap().to_string() == "admin" {
                    is_admin = true;
            }
        }
        true
    }
    ).unwrap();

    return is_admin;
}

fn check_credentials(credentials: &str, connection: &Connection) -> bool {
  
  let parts = credentials.split(":").collect::<Vec<&str>>();

  let userid = parts[0].to_string();
  let password = parts[1].to_string();

  return check_password(User { userid, password}, connection);

}

fn handle_login(credentials: &str, connection: &Connection) -> String {
    let ok = check_credentials(credentials, connection);

     if ok {
        return empty_response(OK);
    } else {
        return empty_response(UNAUTHORIZED)
    }
}


fn main() {

    let admin_user = env::var("ADMIN_USER").unwrap();
    let admin_password = env::var("ADMIN_PASSWORD").unwrap();    

    let admin_user = User {
        userid: admin_user,
        password: admin_password,
    };
    
    let connection = sqlite::open("auth.db").unwrap();


    let query = format!("
        CREATE TABLE IF NOT EXISTS users (userid TEXT PRIMARY KEY, password TEXT);
        ");

    connection.execute(query).unwrap();

    upsert_user(admin_user, &connection);

    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    let pool = ThreadPool::new(4);

    for stream in listener.incoming() {
        let stream = stream.unwrap();

         pool.execute(|db_connection: &Connection| {
            handle_connection(stream, db_connection);
        });
    }
}


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

fn list_users(db_connection: &Connection) -> String {

    let query = "SELECT userid, password FROM users";

    let mut users: Vec<String> = Vec::new();

    db_connection
    .iterate(query, |row| {

        let row_string = row.iter()
                .map(|&(column, val)| {
                    let value = val.unwrap();
                    return format!("{column}: {value}", )
                
                })
                .reduce(|rowa, rowb| format!("{rowa}, {rowb}"));

        users.push(row_string.unwrap());

        true
    })
    .unwrap();

    return body_response(OK, serde_json::to_string(&users).unwrap().as_str())
}

fn echo_body(body: String) -> String {
    return body_response(OK, &body)
}

fn handle_connection(mut stream: TcpStream, connection: &Connection) {
    let mut buf_reader = BufReader::new(&stream);
    let mut content_length = 0;
     // Read headers
    let mut http_request: Vec<String> = Vec::new();
    loop {
        let mut line = String::new();
        buf_reader.read_line(&mut line).unwrap();
        
        if line == "\r\n" || line == "\n" {
            break;  // Blank line marks end of headers
        }

        // Extract Content-Length if present
        if line.starts_with("Content-Length:") {
            content_length = line
                .split(':')
                .nth(1)
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap_or(0);
        }

        http_request.push(line.trim().to_string());
    }
    
    println!("Request: {http_request:#?}");

    // get check basic auth
    let mut auth_credentials = String::new();
    let auth_header = http_request
                                .iter()
                                .find(|line| line.starts_with("Authorization"));

    if auth_header.is_none() {
        // pass
    } else {

        let auth = auth_header.unwrap().split(":").collect::<Vec<&str>>()[1];
        let auth_parts = auth.split_whitespace().collect::<Vec<&str>>();
        let auth_scheme = auth_parts[0];


        if auth_scheme == "Basic" {
            auth_credentials = String::from_utf8(BASE64_STANDARD.decode(auth_parts[1].to_string()).unwrap()).unwrap() ;
        }
    }

    if auth_credentials == String::new() {
        let response = empty_response(UNAUTHORIZED);
        stream.write_all(response.as_bytes()).unwrap();
        return;
    }

    let credentials_ok = check_credentials(&auth_credentials.as_str(), connection);

    if !credentials_ok {
        let response = empty_response(UNAUTHORIZED);
        stream.write_all(response.as_bytes()).unwrap();
        return;
    }

    let request_first_line = &http_request[0];
  
    
    let first_line_parts = request_first_line
        .split_whitespace()
        .collect::<Vec<&str>>();

    let request_method = first_line_parts[0];
    let request_path = first_line_parts[1];
    let request_http_version = first_line_parts[2];
      
    let mut request_body = String::new();
    if request_method == "POST" {
       buf_reader
            .take(content_length as u64)
            .read_to_string(&mut request_body)
            .unwrap();
    }

    if request_http_version != "HTTP/1.1" {
        let response = empty_response(NOT_IMPLEMENTED);
        stream.write_all(response.as_bytes()).unwrap();
        return;
    } 

    let response = match request_method {
        "GET" => match request_path {
            "/" => empty_response(OK),
            "/list" => list_users(connection),
            "/login" => handle_login(auth_credentials.as_str(), connection),
            "/sleep" => {
                // just to prove the multithreading works
                thread::sleep(Duration::from_secs(5));
                empty_response(OK)
            }
            _ => empty_response(NOT_IMPLEMENTED)
        },
        "POST" => match request_path {
            // just to prove the body extraction works
            "/dump" => echo_body(request_body),
            _ => empty_response(NOT_IMPLEMENTED)
        }
        _ => empty_response(NOT_IMPLEMENTED)
    };

    stream.write_all(response.as_bytes()).unwrap();

    
}