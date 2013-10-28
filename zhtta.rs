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
use extra::priority_queue::PriorityQueue;
use std::comm::*;
use std::cast;
use std::option::Option;
use std::hashmap::HashSet;
use std::path::Path;

static PORT:    int = 4414;
static IPV4_LOOPBACK: &'static str = "127.0.0.1";

struct sched_msg {
    stream: Option<std::rt::io::net::tcp::TcpStream>,
    filepath: ~std::path::PosixPath,
    priority: uint
}

impl std::cmp::Ord for sched_msg {
    fn lt(&self, other: &sched_msg) -> bool {
        return self.priority > other.priority;
    }
}


fn main() {
    let req_heap: PriorityQueue<sched_msg> = PriorityQueue::new();
    let shared_req_heap = arc::RWArc::new(req_heap);
    let add_vec = shared_req_heap.clone();
    let take_vec = shared_req_heap.clone();
    
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
                // need a better way to write THIS is Slow!
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
    
    // IP addresses to give higher priority
    let mut ip_vals: HashSet<u32> = HashSet::with_capacity(9000);
    ip_vals.insert((192 as u32 << 24) + (168 as u32 << 16));
    ip_vals.insert((127 as u32 << 24) + (143 as u32 << 16));
    ip_vals.insert((137 as u32 << 24) + (54 as u32 << 16));
    ip_vals.insert(0);
    let shared_ip_map = arc::RWArc::new(ip_vals);

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
        let shared_ip_map = shared_ip_map.clone();
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
                // More better path security!
                let unclean_path = os::getcwd().push(Path(path).to_str()).to_str();
                let mut file_path = ~os::getcwd();
                // paths are always normalized so a/b/../c becomes a/c
                if unclean_path.starts_with(file_path.to_str()) {
                    file_path = ~file_path.push(path);
                }
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
                    let mut priority = file_path.stat().unwrap().st_size as uint;
                    unsafe {
                        match stream {
                            Some(ref s) => { 
                                    let stream = cast::transmute_mut(s);
                                    let pn = stream.peer_name().unwrap();
                                    println(fmt!("Peer is: %?", pn));
                                    match pn.ip {
                                        Ipv4Addr(a, b, c, d) => {   
                                                                // Since we are sharing the ip_map it must be read
                                                                do shared_ip_map.read |map| {
                                                                    if check_ip(a,b,c,d, map) {
                                                                        priority = 1;
                                                                        println("local request!");
                                                                    }
                                                                }
                                                            },
                                        _                    =>  fail!()
                                    }
                            },
                            _    => fail!()
                        };
                    }
                    
                    let msg: sched_msg = sched_msg{stream: stream, filepath: file_path.clone(), priority: priority};
                    child_chan.send(msg);
                    
                    println(fmt!("get file request: %?", file_path));
                }
            }
            println!("connection terminates")
        }
    }
}

// Looks up an ip prefix in the hashset by trying each octet
fn check_ip(a: u8, b: u8, c: u8, d: u8, map: &HashSet<u32>) -> bool {
    let mut mut_ip = a as u32 << 24;  
    if map.contains(&mut_ip) {
        return true;
    }
    mut_ip += (b as u32 << 16);
    if map.contains(&mut_ip) {
        return true;
    }
    mut_ip += (c as u32 << 8);
    if map.contains(&mut_ip) {
        return true;
    }
    mut_ip += (d as u32);
    if map.contains(&mut_ip) {
        return true;
    }
    false
}
