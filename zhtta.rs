//
// zhtta.rs
//
// Running on Rust 0.8
//
// Starting code for PS3
// 
// Note: it would be very unwise to run this server on a machine that is
// on the Internet and contains any sensitive files!
//
// University of Virginia - cs4414 Fall 2013
// Weilin Xu and David Evans
// Version 0.1

extern mod extra;

use std::rt::io::*;
use std::rt::io::net::ip::{SocketAddr, Ipv4Addr};
use std::io::println;
use std::cell::Cell;
use std::{os, str, io};
use extra::arc;
use std::comm::*;
use std::cast;
use std::option::Option;

static PORT:    int = 4414;
static IPV4_LOOPBACK: &'static str = "127.0.0.1";

struct sched_msg {
    stream: Option<std::rt::io::net::tcp::TcpStream>,
    filepath: ~std::path::PosixPath
}

fn main() {
    let req_vec: ~[sched_msg] = ~[];
    let shared_req_vec = arc::RWArc::new(req_vec);
    let add_vec = shared_req_vec.clone();
    let take_vec = shared_req_vec.clone();
    
    let (port, chan) = stream();
    let chan = SharedChan::new(chan);
    
    // FILO SCHEDULING done by port
    // add file requests into queue.
    do spawn {
        while(true) {
            do add_vec.write |vec| {
                let tf:sched_msg = port.recv();
                (*vec).push(tf);
                println("add to queue");
            }
        }
    }

    
    // take file requests from queue, and send a response.
    do spawn {
        while(true) {
            do take_vec.write |vec| {
                let mut tf = (*vec).pop();
                
                match io::read_whole_file(tf.filepath) {
                    Ok(file_data) => {
                        tf.stream.write(file_data);
                    }
                    Err(err) => {
                        println(err);
                    }
                }
            }
        }
    }
    
    let shared_count = arc::RWArc::new(0);

    let socket = net::tcp::TcpListener::bind(SocketAddr {ip: Ipv4Addr(0,0,0,0), port: PORT as u16});
    
    println(fmt!("Listening on tcp port %d ...", PORT));
    let mut acceptor = socket.listen().unwrap();
    
    // we can limit the incoming connection count.
    //for stream in acceptor.incoming().take(10 as uint) {
    for stream in acceptor.incoming() {
        let stream = Cell::new(stream);
        
        let incr_count = shared_count.clone();
        let child_chan = chan.clone();
        // Start a new task to handle the connection
        do spawn {
            do incr_count.write |count| {
                *count = *count + 1;
            }
            
            let mut stream = stream.take();
            let mut buf = [0, ..500];
            stream.read(buf);
            let request_str = str::from_utf8(buf);
            
            let req_group : ~[&str]= request_str.splitn_iter(' ', 3).collect();
            if req_group.len() > 2 {
                let path = req_group[1];
                println(fmt!("Request for path: \n%?", path));
                
                let file_path = ~os::getcwd().push(path.replace("/../", ""));
                if !os::path_exists(file_path) || os::path_is_dir(file_path) {
                    println(fmt!("Request received:\n%s", request_str));
                    let response: ~str = fmt!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n
                         <doctype !html><html><head><title>Hello, Rust!</title>
                         <style>body { background-color: #111; color: #FFEEAA }
                                h1 { font-size:2cm; text-align: center; color: black; text-shadow: 0 0 4mm red}
                                h2 { font-size:2cm; text-align: center; color: black; text-shadow: 0 0 4mm green}
                         </style></head>
                         <body>
                         <h1>Greetings, Krusty!</h1>
                         <h2>Visitor count: %u</h2>
                         </body></html>\r\n", incr_count.read(|c| { *c }));

                    stream.write(response.as_bytes());
                }
                else {
                    // Fun scheduling happens here!
                        // how do I get access to the value borrowed here
                        unsafe {
                    match stream {
                        Some(ref s) => { 
                                let mut stream = cast::transmute_mut(s);
                                match stream.peer_name() {
                                    Some(pn) => { println(fmt!("Peer is:%?", pn.to_str()));},
                                    None     => fail!()
                                }
                        },
                        None    => fail!()
                    };
        }
                    let msg: sched_msg = sched_msg{stream: stream, filepath: file_path.clone()};
                    child_chan.send(msg);
                    
                    println(fmt!("get file request: %?", file_path));
                }
            }
            println!("connection terminates")
        }
    }
}
