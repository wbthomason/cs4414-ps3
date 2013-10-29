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
// Version 0.3

extern mod extra;

use std::rt::io::*;
use std::rt::io::net::ip::SocketAddr;
use std::io::println;
use std::cell::Cell;
use std::{os, str, io, run, vec, hashmap, task};
use extra::arc;
use std::comm::*;
use extra::comm;
use extra::comm::DuplexStream;

static PORT:    int = 4414;
static IP: &'static str = "127.0.0.1";
static mut visitor_count: uint = 0;

struct sched_msg {
    stream: Option<std::rt::io::net::tcp::TcpStream>,
    filepath: ~std::path::PosixPath
}

fn main() {
    let req_vec: ~[sched_msg] = ~[];
    let shared_req_vec = arc::RWArc::new(req_vec);
    let add_vec = shared_req_vec.clone();
    let take_vec = shared_req_vec.clone();

    //let cache: ~hashmap::HashMap<~std::path::PosixPath, ~str> = hashmap::HashMap<~std::path::PosixPath, ~str>::new();
    //let shared_cache = arc::RWArc::new(cache);
    
    let (port, chan) = stream();
    let chan = SharedChan::new(chan);
    
    // dequeue file requests, and send responses.
    // FIFO
    do spawn {
        let (sm_port, sm_chan) = stream();
        
        // a task for sending responses.
        do spawn {
            loop {
                let mut tf: sched_msg = sm_port.recv(); // wait for the dequeued request to handle
                match io::read_whole_file(tf.filepath) { // killed if file size is larger than memory size.
                    Ok(file_data) => {
                        println(fmt!("begin serving file [%?]", tf.filepath));
                        // A web server should always reply a HTTP header for any legal HTTP request.
                        tf.stream.write("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n".as_bytes());
                         // Check if execute bit is set
                        if tf.filepath.get_mode().unwrap() % 2 == 1 {
                            println(fmt!("Processing dynamic file [%?]", tf.filepath));
                            let dyn_file_data = execFile(str::from_utf8(file_data));
                            tf.stream.write(dyn_file_data.as_bytes());
                        } 
                        //No caching of dynamically generated files
                        else {
                           // if 
                            tf.stream.write(file_data);
                        }
                        println(fmt!("finish file [%?]", tf.filepath));
                    }
                    Err(err) => {
                        println(err);
                    }
                }
            }
        }
        
        loop {
            port.recv(); // wait for arrving notification
            do take_vec.write |vec| {
                if ((*vec).len() > 0) {
                    // LIFO didn't make sense in service scheduling, so we modify it as FIFO by using shift_opt() rather than pop().
                    let tf_opt: Option<sched_msg> = (*vec).shift_opt();
                    let tf = tf_opt.unwrap();
                    println(fmt!("shift from queue, size: %ud", (*vec).len()));
                    sm_chan.send(tf); // send the request to send-response-task to serve.
                }
            }
        }
    }

    let ip = match FromStr::from_str(IP) { Some(ip) => ip, 
                                           None => { println(fmt!("Error: Invalid IP address <%s>", IP));
                                                     return;},
                                         };
                                         
    let socket = net::tcp::TcpListener::bind(SocketAddr {ip: ip, port: PORT as u16});
    
    println(fmt!("Listening on %s:%d ...", ip.to_str(), PORT));
    let mut acceptor = socket.listen().unwrap();
    
    for stream in acceptor.incoming() {
        let stream = Cell::new(stream);
        
        // Start a new task to handle the each connection
        let child_chan = chan.clone();
        let child_add_vec = add_vec.clone();
        do spawn {
            unsafe {
                visitor_count += 1;
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
                         </body></html>\r\n", unsafe{visitor_count});

                    stream.write(response.as_bytes());
                }
                else {
                    // Requests scheduling
                    let msg: sched_msg = sched_msg{stream: stream, filepath: file_path.clone()};
                    let (sm_port, sm_chan) = std::comm::stream();
                    sm_chan.send(msg);
                    
                    do child_add_vec.write |vec| {
                        let msg = sm_port.recv();
                        (*vec).push(msg); // enqueue new request.
                        println("add to queue");
                    }
                    child_chan.send(""); //notify the new arriving request.
                    println(fmt!("get file request: %?", file_path));
                }
            }
            println!("connection terminates")
        }
    }
}

pub fn execFile(file_data: &str) -> ~str {
    let index = 0;
    let closes: ~[(uint, uint)] = file_data.matches_index_iter("\" -->").collect();
    let (port, chan) = DuplexStream();
    do task::spawn_supervised {
        do_gash(&chan)
    }
    
    let mut i = 0;
    let mut prev = 0;
    let mut result = ~"";
    for (begin, end) in  file_data.matches_index_iter("<!--#exec cmd=\"") {
        if i >= closes.len() {
            break;
        }

        let (close, close_end) = closes[i];
        let cmd: ~str = file_data.slice(end, close).to_owned();
        port.send(cmd);
        result.push_str(file_data.slice(prev, begin));
        result.push_str(port.recv());
        prev = close_end+1;
        i += 1;
    }
    result.push_str(file_data.slice_from(prev));
    port.send(~"end");
    result
}

fn do_gash(chan: &DuplexStream<~str, ~str>) {
    let mut gash = run::Process::new("./gash", &[], run::ProcessOptions::new());
    let mut cmd: ~str;
    let mut result: ~str;
    let gin = gash.input();
    let gout = gash.output();
    let mut res: ~[u8];
    let mut prompt: ~[u8] = ~[];
    let mut res_byte: u8;
    for _ in range(0,8) {
        prompt.push(0);
    }

    gout.read(prompt, 8);
    // Insecure. We should use some other means of communicating the need to die.
    loop {
        cmd = chan.recv();
        if cmd == ~"end" {
            break;
        }
        cmd.push_str("\n");
        res = ~[];
        gin.write(cmd.as_bytes());
        loop {
            res_byte = gout.read_byte() as u8;
            if res_byte == '\0' as u8{
                break;
            }

            res.push(res_byte);
        }
        gout.read(prompt, 7);
        result = str::from_utf8(res);
        chan.send(result.clone());
    }
    gash.destroy();
}
