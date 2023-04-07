use bytes::BytesMut;

use std::io;

use crate::request::decode::MAX_HEADERS;

pub struct Response<'a> {
    headers: [&'static str; MAX_HEADERS],
    headers_len: usize,
    status_message: StatusMessage,
    body: Body,
    res_buf: &'a mut BytesMut,
}

enum Body {
    StaticStr(&'static str),
    Str(String),
    Vec(Vec<u8>),
    Dummy,
}
struct StatusMessage {
    code: usize,
    msg: &'static str,
}

impl<'a> Response<'a> {
    pub fn new(res_buf: &'a mut BytesMut) -> Response {
        let headers: [&'static str; 16] = [""; 16];

        Response {
            headers,
            headers_len: 0,
            body: Body::Dummy,
            status_message: StatusMessage {
                code: 200,
                msg: "Ok",
            },
            res_buf,
        }
    }

    #[inline]
    pub fn status_code(&mut self, code: usize, msg: &'static str) -> &mut Self {
        self.status_message = StatusMessage { code, msg };
        self
    }

    #[inline]
    pub fn header(&mut self, header: &'static str) -> &mut Self {
        self.headers[self.headers_len] = header;
        self.headers_len += 1;
        self
    }

    #[inline]
    pub fn str(&mut self, s: &'static str) -> io::Result<()> {
        self.body = Body::StaticStr(s);
        Ok(())
    }

    pub fn send<S: AsRef<str>>(&mut self, content: S) -> io::Result<()> {
        match content.as_ref() {
            s if s.len() == 0 => self.body = Body::Dummy,
            s => self.body = Body::Str(s.to_owned()),
        }
        Ok(())
    }

    pub fn json<Value: serde::ser::Serialize>(&mut self, value: &Value) -> io::Result<()> {
        let json_str = serde_json::to_string(value)?;
        self.header("Content-Type: application/json");
        self.send(json_str)?;
        Ok(())
    }

    pub fn bytes(&mut self, content: &[u8]) -> io::Result<()> {
        match content.len() {
            0 => self.body = Body::Dummy,
            _ => self.body = Body::Vec(content.to_vec()),
        }
        Ok(())
    }

    #[inline]
    pub fn body_mut(&mut self) -> &mut BytesMut {
        match self.body {
            Body::Dummy => {}
            Body::StaticStr(s) => {
                self.res_buf.extend_from_slice(s.as_bytes());
                self.body = Body::Dummy;
            }
            Body::Str(ref s) => {
                self.res_buf.extend_from_slice(s.as_bytes());
                self.body = Body::Dummy;
            }
            Body::Vec(ref v) => {
                self.res_buf.extend_from_slice(v);
                self.body = Body::Dummy;
            }
        }
        self.res_buf
    }

    #[inline]
    fn body_len(&self) -> usize {
        match self.body {
            Body::Dummy => self.res_buf.len(),
            Body::StaticStr(s) => s.len(),
            Body::Str(ref s) => s.len(),
            Body::Vec(ref v) => v.len(),
        }
    }

    #[inline]
    fn get_body(&mut self) -> &[u8] {
        match self.body {
            Body::Dummy => self.res_buf.as_ref(),
            Body::StaticStr(s) => s.as_bytes(),
            Body::Str(ref s) => s.as_bytes(),
            Body::Vec(ref v) => v,
        }
    }
}

impl<'a> Drop for Response<'a> {
    fn drop(&mut self) {
        unsafe { self.res_buf.set_len(0) };
    }
}

pub fn encode(mut res: Response, buf: &mut BytesMut) {
    if res.status_message.code == 200 {
        buf.extend_from_slice(b"HTTP/1.1 200 Ok\r\nServer: M\r\nDate: ");
    } else {
        buf.extend_from_slice(b"HTTP/1.1 ");
        let mut code = itoa::Buffer::new();
        buf.extend_from_slice(code.format(res.status_message.code).as_bytes());
        buf.extend_from_slice(b" ");
        buf.extend_from_slice(res.status_message.msg.as_bytes());
        buf.extend_from_slice(b"\r\nServer: M\r\nDate: ");
    }
    crate::response::date::append_date(buf);
    buf.extend_from_slice(b"\r\nContent-Length: ");
    let mut length = itoa::Buffer::new();
    buf.extend_from_slice(length.format(res.body_len()).as_bytes());

    // SAFETY: we already have bound check when insert headers
    let headers = unsafe { res.headers.get_unchecked(..res.headers_len) };
    for h in headers {
        buf.extend_from_slice(b"\r\n");
        buf.extend_from_slice(h.as_bytes());
    }

    buf.extend_from_slice(b"\r\n\r\n");
    buf.extend_from_slice(res.get_body());
}

pub fn encode_error(e: io::Error, buf: &mut BytesMut) {
    error!("error in service: err = {:?}", e);
    let msg_string = e.to_string();
    let msg = msg_string.as_bytes();

    buf.extend_from_slice(b"HTTP/1.1 500 Internal Server Error\r\nServer: M\r\nDate: ");
    crate::response::date::append_date(buf);
    buf.extend_from_slice(b"\r\nContent-Length: ");
    let mut length = itoa::Buffer::new();
    buf.extend_from_slice(length.format(msg.len()).as_bytes());

    buf.extend_from_slice(b"\r\n\r\n");
    buf.extend_from_slice(msg);
}

// impl io::Write for the response body
pub struct BodyWriter<'a>(pub &'a mut BytesMut);

impl<'a> io::Write for BodyWriter<'a> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
