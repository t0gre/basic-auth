use std::{
    env, io::{BufReader, prelude::*}, net::{TcpListener, TcpStream}, sync::Arc, thread, time::Duration
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

enum Role {
    ADMIN,
    USER
}

fn serialize_role(role: Role) -> String {
    return match role {
        Role::ADMIN => "ADMIN".to_string(),
        Role::USER => "USER".to_string()
    };
}

fn parse_role(value: String) -> Option<Role> {
    return match value.as_str() {
        "ADMIN" => Some(Role::ADMIN),
        "USER" => Some(Role::USER),
        _ => None
    }
}
struct User {
    userid: String,
    password: String,
    role: Role
}

const NOT_IMPLEMENTED: &str = "HTTP/1.1 501 Not Implemented";
const OK: &str = "HTTP/1.1 200 OK";
const UNAUTHORIZED: &str = "HTTP/1.1 401 Unauthorized";
const BAD_REQUEST: &str = "HTTP/1.1 400 Bad Request";

fn upsert_user(user: User, connection: &Connection, argon2: &Argon2) {
     let userid = user.userid;
     let password = user.password;
     let role = serialize_role(user.role);

     let salt = SaltString::generate(&mut OsRng);

     let password_hash = argon2.hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string();

     let query = format!("
        INSERT INTO users(userid, password, role) VALUES('{userid}','{password_hash}','{role}')
            ON CONFLICT(userid) DO UPDATE SET password='{password_hash}', role='{role}' ;
        ");

    connection.execute(query).unwrap();
}

fn check_password(user: &User, connection: &Connection, argon2: &Argon2) -> bool {
     let userid = &user.userid;
     let password = &user.password;


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
    argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok()
}

fn is_admin(user: &User, connection: &Connection) -> bool {

    let userid = &user.userid;

    let query = format!("
        SELECT role FROM users WHERE userid='{userid}'"
    );

    let mut is_admin = false;

    connection.iterate(query, |row| {
        for &(column, val) in row.iter() {
            if column == "role" && val.unwrap().to_string() == "ADMIN" {
                    is_admin = true;
            }
        }
        true
    }
    ).unwrap();

    return is_admin;
}

fn check_credentials(credentials: &str, connection: &Connection, argon2: &Argon2) -> Option<User> {
  
  let parts = credentials.split(":").collect::<Vec<&str>>();

  let userid = parts[0].to_string();
  let password = parts[1].to_string();

  let user = User { 
    userid, 
    password, 
    role: Role::USER  // not used
};

  if check_password(&user, connection, argon2) {
    return Some(user)
  }
  return None

}

fn handle_login(credentials: &str, connection: &Connection, argon2: &Argon2) -> String {
    let user = check_credentials(credentials, connection, argon2);

    match user {
        Some(_) =>  return empty_response(OK),
        None => return empty_response(UNAUTHORIZED)
    }
}


fn main() {

    let admin_user = env::var("ADMIN_USER").unwrap();
    let admin_password = env::var("ADMIN_PASSWORD").unwrap();  
    let argon2 = Arc::new(Argon2::default());

    let admin_user = User {
        userid: admin_user,
        password: admin_password,
        role: Role::ADMIN
    };
    
    let connection = sqlite::open("auth.db").unwrap();


    let query = format!("
        CREATE TABLE IF NOT EXISTS users (userid TEXT PRIMARY KEY, password TEXT, role TEXT);
        ");

    connection.execute(query).unwrap();

    upsert_user(admin_user, &connection, &*argon2);

    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    let pool = ThreadPool::new(4);

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        let argon2clone = Arc::clone(&argon2);

        pool.execute(move |db_connection: &Connection| {
            handle_connection(stream, db_connection, &*argon2clone);
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

fn handle_user_reset(user: User, body: String, connection: &Connection, argon2: &Argon2) -> String {
    let new_password = body;

     let user = User {
        userid: user.userid,
        password: new_password,
        role: user.role
    };

    upsert_user(user, &connection, argon2);
    return empty_response(OK)
}

fn handle_user_upsert(body: String, connection: &Connection, argon2: &Argon2) -> String {
 
 let body_parts = body.split(":").collect::<Vec<&str>>();

 let userid = body_parts[0].to_string();
 let password = body_parts[1].to_string();

 let number_of_parts = body_parts.len();
 if number_of_parts < 2 || number_of_parts > 3 {
    return empty_response(BAD_REQUEST)
 }

 let mut role = Role::USER;


 if number_of_parts == 3 {
    match parse_role(body_parts[2].to_string()) {
        Some(provided_role) => {
            role = provided_role
        },
        None => { 
            // pass 
            }
 }
}

 let user = User {
                userid,
                password,
                role
            };

 upsert_user(user, &connection, argon2);
 return empty_response(OK)

}

fn handle_user_delete(body: String, connection: &Connection) -> String {
    let userid = body;
    let delete = format!("DELETE FROM users WHERE userid='{userid}'");

    if connection.execute(delete).is_ok() {
         return empty_response(OK)
    };

    return body_response(BAD_REQUEST, format!("user not found: {userid}").as_str())

}

fn handle_connection(mut stream: TcpStream, connection: &Connection, argon2: &Argon2) {
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

    let credential_check = check_credentials(&auth_credentials.as_str(), connection, argon2);

    match credential_check {
        Some(user) => {
            
            let request_first_line = &http_request[0];
  
    
            let first_line_parts = request_first_line
                .split_whitespace()
                .collect::<Vec<&str>>();

            let request_method = first_line_parts[0];
            let request_path = first_line_parts[1];
            let request_http_version = first_line_parts[2];
            
            let mut body = String::new();
            if request_method == "POST" {
            buf_reader
                    .take(content_length as u64)
                    .read_to_string(&mut body)
                    .unwrap();
            }

            if request_http_version != "HTTP/1.1" {
                let response = empty_response(NOT_IMPLEMENTED);
                stream.write_all(response.as_bytes()).unwrap();
                return;
            } 

            let is_admin = is_admin(&user, connection);

            let response = match request_method {
                "GET" => match request_path {
                    "/" => empty_response(OK), 
                    "/list" => if is_admin { list_users(connection)} else {empty_response(UNAUTHORIZED)},
                    "/login" => handle_login(auth_credentials.as_str(), connection, argon2),
                    "/sleep" => if is_admin { // just to prove the multithreading works
                        thread::sleep(Duration::from_secs(5));
                        empty_response(OK)
                    } else {empty_response(UNAUTHORIZED)}
                    _ => empty_response(NOT_IMPLEMENTED)
                },
                "POST" => match request_path {
                    "/dump" => echo_body(body), // just to prove the body extraction works
                    "/reset" => handle_user_reset(user, body, connection, argon2),
                    "/upsert" => if is_admin { handle_user_upsert(body, connection, argon2) } else {empty_response(UNAUTHORIZED)},
                    "/delete" => if is_admin { handle_user_delete(body, connection) } else {empty_response(UNAUTHORIZED)},
                    _ => empty_response(NOT_IMPLEMENTED)
                }
                _ => empty_response(NOT_IMPLEMENTED)
            };

            stream.write_all(response.as_bytes()).unwrap();

        },
        None => {
            let response = empty_response(UNAUTHORIZED);
            stream.write_all(response.as_bytes()).unwrap();
            return;
        }
    }

    
    
}