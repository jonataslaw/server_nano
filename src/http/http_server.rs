//! http server implementation on top of `MAY`

use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::net::ToSocketAddrs;


use bytes::{Buf, BytesMut};

#[cfg(unix)]
use may::io::WaitIo;
use may::net::{TcpListener, TcpStream};
use may::{coroutine, go};

use crate::request::request::RawRequest;
use crate::response::response::Response;

const BUF_LEN: usize = 4096 * 8;

macro_rules! t_c {
    ($e: expr) => {
        match $e {
            Ok(val) => val,
            Err(err) => {
                error!("call = {:?}\nerr = {:?}", stringify!($e), err);
                continue;
            }
        }
    };
}

pub trait HttpService {
    fn handler(&mut self, req: RawRequest, rsp: &mut Response) -> io::Result<()>;
}

pub trait HttpServiceFactory: Send + Sized + 'static {
    type Service: HttpService + Send;

    fn new_service(&self, id: usize) -> Self::Service;

    fn start<L: ToSocketAddrs>(self, addr: L) -> io::Result<coroutine::JoinHandle<()>> {
        let listener = TcpListener::bind(addr)?;
        go!(
            coroutine::Builder::new().name("TcpServerFac".to_owned()),
            move || {
                #[cfg(unix)]
                use std::os::fd::AsRawFd;
                #[cfg(windows)]
                use std::os::windows::io::AsRawSocket;
                for stream in listener.incoming() {
                    let mut stream = t_c!(stream);
                    #[cfg(unix)]
                    let id = stream.as_raw_fd() as usize;
                    #[cfg(windows)]
                    let id = stream.as_raw_socket() as usize;
                    // t_c!(stream.set_nodelay(true));
                    let service = self.new_service(id);
                    let builder = may::coroutine::Builder::new().id(id);
                    go!(
                        builder,
                        move || if let Err(e) = each_connection_loop(&mut stream, service) {
                            error!("service err = {:?}", e);
                            stream.shutdown(std::net::Shutdown::Both).ok();
                        }
                    )
                    .unwrap();
                }
            }
        )
    }
}

#[cfg(unix)]
#[inline]
fn nonblock_read(stream: &mut impl Read, req_buf: &mut BytesMut) -> io::Result<usize> {
    let mut read_cnt = 0;
    loop {
        let read_buf: &mut [u8] = unsafe { std::mem::transmute(req_buf.chunk_mut()) };
        match stream.read(read_buf) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed")),
            Ok(n) => {
                read_cnt += n;
                unsafe { req_buf.advance_mut(n) };
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => return Ok(read_cnt),
            Err(err) => return Err(err),
        }
    }
}

#[cfg(unix)]
#[inline]
fn nonblock_write(stream: &mut impl Write, write_buf: &mut BytesMut) -> io::Result<usize> {
    let len = write_buf.len();
    if len == 0 {
        return Ok(0);
    }

    let mut written = 0;
    while written < len {
        match stream.write(unsafe { write_buf.get_unchecked(written..) }) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed")),
            Ok(n) => written += n,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
            Err(err) => return Err(err),
        }
    }
    write_buf.advance(written);
    Ok(written)
}

#[inline]
pub(crate) fn reserve_buf(buf: &mut BytesMut) {
    let capacity = buf.capacity();
    if capacity < 1024 {
        buf.reserve(BUF_LEN - capacity);
    }
}

pub struct HttpServer<T>(pub T);

#[cfg(unix)]
fn each_connection_loop<T: HttpService>(stream: &mut TcpStream, mut service: T) -> io::Result<()> {
    let mut req_buf = BytesMut::with_capacity(BUF_LEN);
    let mut res_buf = BytesMut::with_capacity(BUF_LEN);
    let mut body_buf = BytesMut::with_capacity(BUF_LEN);

    loop {
        stream.reset_io();

        let inner_stream = stream.inner_mut();

        // write out the responses
        nonblock_write(inner_stream, &mut res_buf)?;

        // read the socket for requests
        reserve_buf(&mut req_buf);
        let read_cnt = nonblock_read(inner_stream, &mut req_buf)?;

        // prepare the requests
        if read_cnt > 0 {
            loop {
                let mut headers = [MaybeUninit::uninit(); request::MAX_HEADERS];
                let req = match request::decode(&mut headers, &mut req_buf, stream)? {
                    Some(req) => req,
                    None => break,
                };
                let mut rsp = Response::new(&mut body_buf);
                match service.handler(req, &mut rsp) {
                    Ok(()) => response::encode(rsp, &mut res_buf),
                    Err(e) => {
                        eprintln!("service err = {:?}", e);
                        response::encode_error(e, &mut res_buf);
                    }
                }
            }
        }

        if res_buf.is_empty() {
            stream.wait_io();
        }
    }
}

#[cfg(not(unix))]
fn each_connection_loop<T: HttpService>(stream: &mut TcpStream, mut service: T) -> io::Result<()> {
    use crate::{request, response};

    let mut req_buf = BytesMut::with_capacity(BUF_LEN);
    let mut res_buf = BytesMut::with_capacity(BUF_LEN);
    let mut body_buf = BytesMut::with_capacity(BUF_LEN);

    loop {
        // Ensure there is enough space in the buffer
        reserve_buf(&mut req_buf);

        // Prepare a temporary buffer for reading
        let mut temp_buf = vec![0u8; BUF_LEN];
        let read_cnt = stream.read(&mut temp_buf)?;
        if read_cnt == 0 {
            // Connection was closed
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
        }

        // Append the data read into the request buffer
        req_buf.extend_from_slice(&temp_buf[..read_cnt]);

        // Prepare the requests
        if read_cnt > 0 {
            loop {
                let mut headers = [MaybeUninit::uninit(); request::request::MAX_HEADERS];
                let req = match request::request::decode(&mut headers, &mut req_buf, stream)? {
                    Some(req) => req,
                    None => break,
                };
                let mut rsp = Response::new(&mut body_buf);
                match service.handler(req, &mut rsp) {
                    Ok(()) => response::response::encode(rsp, &mut res_buf),
                    Err(e) => {
                        eprintln!("service err = {:?}", e);
                        response::response::encode_error(e, &mut res_buf);
                    }
                }
            }
        }

        // Send the result back to client
        while !res_buf.is_empty() {
            let written = stream.write(&res_buf)?;
            if written == 0 {
                // No bytes were written, the connection might have been closed
                return Err(io::Error::new(io::ErrorKind::WriteZero, "write zero byte"));
            }
            res_buf.advance(written);
        }

        // Clear the buffer after ensuring all data is sent
        res_buf.clear();
    }
}


impl<T: HttpService + Clone + Send + Sync + 'static> HttpServer<T> {
    pub fn start<L: ToSocketAddrs>(self, addr: L) -> io::Result<coroutine::JoinHandle<()>> {
        let listener = TcpListener::bind(addr)?;
        let service = self.0;
        go!(
            coroutine::Builder::new().name("TcpServer".to_owned()),
            move || {
                for stream in listener.incoming() {
                    let mut stream = t_c!(stream);
                    let service = service.clone();
                    go!(
                        move || if let Err(e) = each_connection_loop(&mut stream, service) {
                            error!("service err = {:?}", e);
                            stream.shutdown(std::net::Shutdown::Both).ok();
                        }
                    );
                }
            }
        )
    }
}
