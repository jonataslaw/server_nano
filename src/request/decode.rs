use bytes::BytesMut;
use std::io;
use std::mem::MaybeUninit;

pub(crate) const MAX_HEADERS: usize = 16;

pub struct Decoded<'a, 'header> {
    raw: httparse::Request<'header, 'a>,
    len: usize,
    body: &'a [u8],
}

impl<'a, 'header> Decoded<'a, 'header> {
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn raw(&self) -> &httparse::Request<'header, 'a> {
        &self.raw
    }

    #[inline]
    pub fn body(&self) -> &'a [u8] {
        &self.body
    }
}

pub fn decode<'a, 'header>(
    buf: &'a BytesMut,
    headers: &'header mut [MaybeUninit<httparse::Header<'a>>; MAX_HEADERS],
) -> io::Result<Option<Decoded<'a, 'header>>> {
    let mut req = httparse::Request::new(&mut []);
    let status = match req.parse_with_uninit_headers(buf, headers) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("failed to parse http request: {e:?}");
            return Err(io::Error::new(io::ErrorKind::Other, msg));
        }
    };

    let len = match status {
        httparse::Status::Complete(amt) => amt,
        httparse::Status::Partial => return Ok(None),
    };

    // Find the Content-Length header value
    let content_length = req
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-length"))
        .and_then(|header| std::str::from_utf8(header.value).ok())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    let body_start = buf.len() - content_length;

    let body = &buf[body_start..];

    Ok(Some(Decoded {
        raw: req,
        len,
        body,
    }))
}
