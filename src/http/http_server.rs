use crate::request::decode;
use crate::Decoded;
use crate::Response;

use bytes::{Buf, BufMut, BytesMut};
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::net::ToSocketAddrs;

#[cfg(unix)]
use may::io::WaitIo;
use may::net::{TcpListener, TcpStream};
use may::{coroutine, go};

const BUF_LEN: usize = 4096 * 16;

pub trait HttpService {
    fn handler(&mut self, req: Decoded, res: &mut Response) -> io::Result<()>;
}

#[cfg(unix)]
#[inline]
fn nonblock_read(stream: &mut impl Read, req_buf: &mut BytesMut) -> io::Result<usize> {
    let read_buf: &mut [u8] = unsafe { std::mem::transmute(&mut *req_buf.chunk_mut()) };
    let len = read_buf.len();
    let mut read_cnt = 0;
    while read_cnt < len {
        match stream.read(unsafe { read_buf.get_unchecked_mut(read_cnt..) }) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed")),
            Ok(n) => read_cnt += n,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
            Err(err) => return Err(err),
        }
    }

    unsafe { req_buf.advance_mut(read_cnt) };
    Ok(read_cnt)
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
fn reserve_buf(buf: &mut BytesMut) {
    let capacity = buf.capacity();
    if capacity < 1024 {
        buf.reserve(BUF_LEN - capacity);
    }
}

pub struct HttpServer<T>(pub T);

#[cfg(unix)]
fn each_connection_loop<T: HttpService>(stream: &mut TcpStream, mut service: T) -> io::Result<()> {
    use crate::response::response;
    let mut req_buf = BytesMut::with_capacity(BUF_LEN);
    let mut res_buf = BytesMut::with_capacity(BUF_LEN);
    let mut body_buf = BytesMut::with_capacity(BUF_LEN);
    loop {
        stream.reset_io();

        let inner_stream = stream.inner_mut();

        // read the socket for requests
        reserve_buf(&mut req_buf);
        let read_cnt = nonblock_read(inner_stream, &mut req_buf)?;

        // prepare the requests
        if read_cnt > 0 {
            let mut headers = unsafe { MaybeUninit::uninit().assume_init() };
            while let Some(req) = decode::decode(&req_buf, &mut headers)? {
                let len = req.len();
                let mut res = Response::new(&mut body_buf);
                match service.handler(req, &mut res) {
                    Ok(()) => response::encode(res, &mut res_buf),
                    Err(e) => response::encode_error(e, &mut res_buf),
                }
                headers = unsafe { std::mem::transmute(headers) };
                req_buf.advance(len);
            }
        }

        // write out the responses
        nonblock_write(inner_stream, &mut res_buf)?;

        // Clear the buffers for the next connection
        req_buf.clear();
        res_buf.clear();
        body_buf.clear();

        stream.wait_io();
    }
}

#[cfg(windows)]
fn each_connection_loop<T: HttpService>(stream: &mut TcpStream, mut service: T) -> io::Result<()> {
    use crate::response::response;
    let mut req_buf = BytesMut::with_capacity(BUF_LEN);
    let mut res_buf = BytesMut::with_capacity(BUF_LEN);
    let mut body_buf = BytesMut::with_capacity(BUF_LEN);
    loop {
        // read the socket for requests
        reserve_buf(&mut req_buf);
        let read_buf: &mut [u8] = unsafe { std::mem::transmute(&mut *req_buf.chunk_mut()) };
        let read_cnt = stream.read(read_buf)?;
        if read_cnt == 0 {
            //connection was closed
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
        }
        unsafe { req_buf.advance_mut(read_cnt) };

        // prepare the requests
        if read_cnt > 0 {
            let mut headers = [MaybeUninit::<httparse::Header>::uninit(); decode::MAX_HEADERS];
            while let Some(req) = decode::decode(&req_buf, &mut headers)? {
                let len = req.len();
                let mut res = Response::new(&mut body_buf);
                match service.handler(req, &mut res) {
                    Ok(()) => response::encode(res, &mut res_buf),
                    Err(e) => response::encode_error(e, &mut res_buf),
                }
                headers = [MaybeUninit::<httparse::Header>::uninit(); decode::MAX_HEADERS];
                req_buf.advance(len);
            }
        }

        req_buf.clear();
        res_buf.clear();
        body_buf.clear();

        // send the result back to client
        stream.write_all(res_buf.as_ref())?;
    }
}

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
