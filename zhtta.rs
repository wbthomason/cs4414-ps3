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
use std::rt::io::net::ip::{SocketAddr, Ipv4Addr};
use std::io::println;
use std::cell::Cell;
use std::{os, str, io, run, task, vec};
use extra::arc;
use extra::priority_queue::PriorityQueue;
use std::comm::*;
use std::cast;
use extra::comm::DuplexStream;
use std::hashmap::HashSet;
use std::path::Path;

static PORT:  int = 4414;
static IP: &'static str = "127.0.0.1";
static mut visitor_count: uint = 0;

#[deriving(Clone)]
struct access_t {
    filepath: ~std::path::PosixPath,
    size: i64, 
    data: ~[u8]
}

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

impl std::cmp::Ord for access_t {
    fn lt(&self, other: &access_t)  ->  bool {
        self.size > other.size
    }
}

fn main() {
    let req_heap: PriorityQueue<sched_msg> = PriorityQueue::new();
    let shared_req_heap = arc::RWArc::new(req_heap);
    let add_vec = shared_req_heap.clone();
    let take_vec = shared_req_heap.clone();

    let cache: PriorityQueue<access_t> = PriorityQueue::new();
    let shared_cache = arc::RWArc::new(cache);
    let rem_cache = shared_cache.clone();
    let check_cache = shared_cache.clone();

    let (port, chan) = stream();
    let chan = SharedChan::new(chan);

    do spawn {
        loop {
            timer::sleep(9000);
            do rem_cache.write |rc| {
                (*rc).clear();
            }
        }
    }

    // dequeue file requests, and send responses.
    // FIFO
    do spawn {
        let (sm_port, sm_chan) = stream();

        
        // a task for sending responses.
        do spawn {
            loop {
                let mut tf: sched_msg = sm_port.recv(); // wait for the dequeued request to handle
                let fpath = tf.filepath.clone();
                println(fmt!("begin serving file [%?]", tf.filepath.to_str()));
                // A web server should always reply a HTTP header for any legal HTTP request.
                let extension = fpath.components().last();
                match extension.rfind('.') {
                    Some(x) =>  { 
                        match extension.slice_from(x+1) {
                            "html"|"xhtml"|"txt"|"xml"  => { tf.stream.write("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n".as_bytes()); }
                            _                           => { tf.stream.write("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream; charset=UTF-8\r\n\r\n".as_bytes()); }
                        } 
                    }
                    _       =>  { tf.stream.write("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream; charset=UTF-8\r\n\r\n".as_bytes()); }
                }

                do check_cache.write |cc| {
                    let mut cached = false;
                    for elem in (*cc).iter() {
                        if elem.filepath == fpath {
                            tf.stream.write(elem.data);
                            cached = true;
                            break;
                        }
                    }
                    if !cached {
                        if tf.filepath.get_mode().unwrap() % 2 == 1 {
                            execFile(&mut tf);
                            // No cache for dynamically-generated files.
                        }
                        else {
                            let data = writeFile(&mut tf); 
                            let size = fpath.get_size().unwrap();
                            if (*cc).len() < 10 {
                                (*cc).push(access_t { filepath: fpath.clone(), size: size, data: data });
                            }
                            else if (*cc).top().size < size { (*cc).replace(access_t { filepath: fpath.clone(), size: size, data: data }); }
                        }
                    }
                }

                println(fmt!("finish file [%?]", fpath.to_str()));
            }
        }
        
        loop {
            port.recv(); // wait for arrving notification
            do take_vec.write |vec| {
                if ((*vec).len() > 0) {
                    // LIFO didn't make sense in service scheduling, so we modify it as FIFO by using shift_opt() rather than pop().
                    let tf: sched_msg = (*vec).pop();
                    println(fmt!("shift from queue, size: %ud", (*vec).len()));
                    sm_chan.send(tf); // send the request to send-response-task to serve.
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

    let ip = match FromStr::from_str(IP) { Some(ip) => ip, 
                                           None => { println(fmt!("Error: Invalid IP address <%s>", IP));
                                                     return;},
                                         };
                                         
                                         
    let socket = net::tcp::TcpListener::bind(SocketAddr {ip: ip, port: PORT as u16});
    
    println(fmt!("Listening on %s:%d ...", ip.to_str(), PORT));
    let mut acceptor = socket.listen().unwrap();
    

    for stream in acceptor.incoming() {
        let stream = Cell::new(stream);

        let incr_count = shared_count.clone();
        
        // Start a new task to handle the each connection
        let child_chan = chan.clone();
        let shared_ip_map = shared_ip_map.clone();
        let child_add_vec = add_vec.clone();

        do spawn {
            do incr_count.write |count| {
                *count += 1;
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
                    // Requests scheduling

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



fn execFile(file_data: &mut sched_msg) {
    let (port, chan) = DuplexStream();
    do task::spawn_supervised {
        do_gash(&chan)
    }
    match io::file_reader(file_data.filepath) {
        Ok(rd)      =>  {   let closer = ~['\"',' ', '-','-','>'];
                            while !rd.eof() {
                            let rd_byte = rd.read_byte() as u8;

                            match rd_byte {
                                0x3c =>  { let mut open: ~[u8] = ~[0,0,0,0,0,0,0,0,0,0,0,0,0,0];
                                                          rd.read(open, 14);
                                                          if str::from_utf8(open) == ~"!--#exec cmd=\"" {
                                                                let mut cmd: ~[u8] = ~[];
                                                                let mut cmd_byte: u8;
                                                                let mut i = 0;
                                                                loop {
                                                                    cmd_byte = rd.read_byte() as u8;
                                                                    if cmd_byte == closer[i] as u8 {
                                                                        if i == closer.len() - 1 {
                                                                            break;
                                                                        }

                                                                        i += 1;
                                                                    }
                                                                    else {
                                                                        i = 0;
                                                                    }

                                                                    if cmd_byte != 0xFF {
                                                                        cmd.push(cmd_byte);
                                                                    }
                                                                }

                                                          port.send(str::from_utf8(cmd.slice_to(cmd.len() - 4)));
                                                          let result = port.recv();    
                                                          file_data.stream.write(result.as_bytes());
                                                          }  
                                                        },
                                _                   =>  { if rd_byte != 0xFF {
                                                                file_data.stream.write(&[rd_byte]); 
                                                          }
                                                        }
                            }
                          }
                          port.send(~"end");
                        }
        Err(err)    =>  { println(err); }
    }

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

fn writeFile(tf: &mut sched_msg) ->  ~[u8] {
    let mut file: ~[u8] = ~[];
    let mut writes = 0;
    match io::file_reader(tf.filepath) {
        Ok(rd)      =>  { while !rd.eof() {
                            let mut buffer: ~[u8] = vec::with_capacity(20971520u);
                            unsafe { vec::raw::set_len(&mut buffer, 20971520u); }
                            let read = rd.read(buffer, 20971520u);
                            unsafe { vec::raw::set_len(&mut buffer, read); }
                            file.push_all(buffer);
                            tf.stream.write(buffer);
                            writes += 1;
                          }
                          println(fmt!("%d", writes));
                        }
        Err(err)    =>  { println(err); }
    }
    file
}


