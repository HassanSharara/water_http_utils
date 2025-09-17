mod errors;

use std::collections::HashMap;
pub use errors::*;
use crate::config::global_config;
use crate::request::CreatingRequestErrors;

/// including all parsed headers
#[derive(Debug)]
 pub struct HttpHeaders<'buf,const HL: usize>
 {
     lines:[HeaderLine<'buf>;HL],
     /// defining content length for public and fast access
     pub content_length:Option<usize>,
     /// defining headers length
     pub headers_length:usize,

 }

macro_rules! try_forward {
    ($index:ident,$last_index:ident,$total_length:ident) => {
        try_forward!($index + 1,$last_index,$total_length);
    };
    ($index:ident+$v:expr,$last_index:ident,$total_length:ident) => {
        if($index+$v >= $total_length ) {return CreatingHeadersErrors::ReadMore.into();}
        $last_index = $index +$v;
    };
}

#[inline]
/// for converting usize bytes to usize object in rust
fn bytes_to_usize(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() || bytes.len() > 20 { // usize::MAX is ~20 digits
        return None;
    }

    let mut result = 0usize;
    for &byte in bytes {
        if byte < b'0' || byte > b'9' {
            return None; // Invalid digit
        }
        result = result.checked_mul(10)?.checked_add((byte - b'0') as usize)?;
    }
    Some(result)
}

impl<'buf,const HL:usize> HttpHeaders<'buf,HL>{
    /// creating new HttpHeaders from incoming bytes
    pub (crate) fn new(bytes:&'buf[u8])->Result<HttpHeaders<'buf,HL>,CreatingHeadersErrors>{
        let mut lines = [HeaderLine::empty();HL];
        let mut lines_index =0_usize;
        let total_length = bytes.len();
        let global_config = global_config();
        let mut end_indicator  = 0_u8;
        let mut last_index = 0_usize;
        let mut key = None;
        let mut content_length = None;
        for (index,byte) in bytes.iter().enumerate() {
            if index >= global_config.max_headers_size { return CreatingHeadersErrors::DangerousInvalidFormat.into();}

            match byte {
                &b':' => {
                    if key.is_some() {continue;}
                    key = Some(&bytes[last_index..index]);
                    try_forward!(index+2,last_index,total_length);
                }

                &b'\r' => {
                    end_indicator+=1;
                    if let Some(k) = key {
                        match k {
                            b"Content-Length"=>{content_length = bytes_to_usize(&bytes[last_index..index])}
                            b"content-length"=>{content_length = bytes_to_usize(&bytes[last_index..index])}
                            _ =>{}
                        }
                        let line = lines.get_mut(lines_index);
                        if let Some(line) = line {
                            line.key = std::str::from_utf8(k).unwrap();
                            line.value = (&bytes[last_index..index]).into();
                        }
                        key = None;
                        lines_index +=1;
                    }
                    try_forward!(index,last_index,total_length);
                }
                &b'\n' => {
                    end_indicator+=1;
                    if end_indicator >= 4 {
                        return Ok(
                            HttpHeaders {
                                lines,
                                headers_length:index,
                                content_length
                            }
                        )
                    }
                    else if index+1  >= bytes.len() { return CreatingHeadersErrors::ReadMore.into()}
                    last_index=index + 1;
                }
                _ => {
                    end_indicator = 0;
                }
            }
        }

        Err(CreatingHeadersErrors::ReadMore)
    }

    /// for getting specific header value based on header key
    pub fn get(&self,key:&str)->Option<&HeaderValue<'buf>>{
        for line in &self.lines {
            if line.key == key {
                return  Some(&line.value)
            }
        }
        for line in &self.lines {
            if line.key.to_lowercase() == key.to_lowercase() {
                return  Some(&line.value)
            }
        }
        None
    }


    /// getting key value as ['&str']
    pub fn get_as_str(&self,key:&str)->Option<&'buf str>{
        if let Some(value) =  self.get(key) {
            return Some(value.to_str());
        }
        None
    }

    /// getting key value as ['&[u8]']
    pub fn get_as_bytes(&self,key:&str)->Option<&'buf [u8]>{
        if let Some(value) =  self.get(key) {
            return Some(value.bytes);
        }
        None
    }
    /// getting all header lines
    pub fn lines(&self)->Vec<&HeaderLine>{
        self.lines.iter().filter(|x| !x.key.is_empty()).collect()
    }

}


impl From<CreatingHeadersErrors> for CreatingRequestErrors {
    fn from(value: CreatingHeadersErrors) -> Self {
        match value {
            CreatingHeadersErrors::InvalidFormat => {CreatingRequestErrors::InvalidHttpFormat}
            CreatingHeadersErrors::MaxHeadersSizeReachedOut => {CreatingRequestErrors::DangerousInvalidHttpFormat}
            CreatingHeadersErrors::ReadMore => { CreatingRequestErrors::InsufficientDataSoReadMore }
            CreatingHeadersErrors::DangerousInvalidFormat => {CreatingRequestErrors::DangerousInvalidHttpFormat}
        }
    }
}

/// implementing single header line functionality
#[derive(Debug,Clone,Copy)]
pub struct HeaderLine<'buf> {
    /// defining header key
    pub key:&'buf str,
    /// header value
    pub value:HeaderValue<'buf>
}

impl <'buf> HeaderLine<'buf> {

    /// creating new empty header line
    pub fn empty()->HeaderLine<'buf> {
        HeaderLine {
            key:"",
            value:HeaderValue::new(&[])
        }
    }
}

/// http header value could have multiple values or single one
#[derive(Debug,Copy,Clone)]
pub struct HeaderValue<'buf> {
    bytes:&'buf [u8],
}


impl <'buf> HeaderValue<'buf> {

    pub (crate) fn new(bytes:&'buf [u8])->HeaderValue<'buf>{
        HeaderValue {
            bytes
        }
    }

    /// separate headers values like Accept: text/html, application/xhtml+xml, application/xml;q=0.9, */*;q=0.8
    /// into values like ["text/html","application/xhtml+xml",...]
    pub fn all_injected_values(&self)->Vec<&'buf str>{
        let v:&'buf str = self.into();
        let s = v.split(", ").collect();
        s
    }

    /// returning all injected values which means values that separated by ','
    /// # return [`Vec<HeaderVWithParams>`]
    pub fn all_injected_values_with_params(&self)->Vec<HeaderVWithParams<'buf>>{
        let values = self.all_injected_values();
        let mut vs = vec![];
        for v in values {
            let h = HeaderVWithParams::new(v.as_bytes());
            if let Ok(v) = h {
                vs.push(v);
            }
        }
        vs
    }

    /// reading header value as str with zero copy of bytes
    pub fn to_str(&self)->&'buf str {
        unsafe{std::str::from_utf8_unchecked(self.bytes)}
    }
}


impl<'buf> Into<HeaderValue<'buf>> for &'buf [u8] {
    fn into(self) -> HeaderValue<'buf> {
        HeaderValue::new(self)
    }
}

impl <'buf> Into<HeaderVWithParams<'buf>> for &'buf str {
    fn into(self) -> HeaderVWithParams<'buf> {
        HeaderVWithParams::new(self.as_bytes()).unwrap()
    }
}

impl <'buf> Into<&'buf str> for &HeaderValue<'buf> {
    fn into(self) -> &'buf str {
       unsafe { std::str::from_utf8_unchecked(self.bytes)}
    }
}
impl  Into<String> for HeaderValue<'_> {
    fn into(self) -> String{
       unsafe { std::str::from_utf8_unchecked(self.bytes)}.to_string()
    }
}


/// for structuring headers values with params
#[derive(Debug)]
pub struct HeaderVWithParams<'buf> {
    data:&'buf [u8],
    value:&'buf str,
    /// all value parameters if existed
    pub params:HashMap<Option<&'buf str>,&'buf [u8]>
}



macro_rules! set_value_to_header {
    ($value:ident,$key:ident,$map:ident,$last_index:ident,$index:ident,$bytes:ident) => {
         if let Some(k) = $key {
                        $map.insert(
                            Some(unsafe{std::str::from_utf8_unchecked(k)}),
                            &$bytes[$last_index..$index]
                        );
                        $key = None;
                    }else if $value.is_none() {
                        $value = Some(unsafe { std::str::from_utf8_unchecked(&$bytes[$last_index..$index]) });
                    } else {
                        $map.insert(
                            None,
                            &$bytes[$last_index..$index]
                        );
                    }
                    $last_index = $index;
    };
}
impl<'buf> HeaderVWithParams<'buf>{

    /// generating new
    pub (crate) fn new(bytes:&'buf [u8])->Result<HeaderVWithParams<'buf>,()>{

        let mut map = HashMap::new();
        let mut value = None;
        let mut key = None;
        let mut last_index = 0_usize;
        for (index,byte)  in bytes.iter().enumerate() {
            match byte {
                &b';'=>{
                    set_value_to_header!(value,key,map,last_index,index,bytes);
                }
                &b' '=>{
                    if index == last_index+1 {
                        if index + 1 >= bytes.len() { return Err(())}
                        last_index = index + 1;
                    }
                }
                &b'='=>{
                    key = Some(&bytes[last_index..index]);
                    if index + 1 >= bytes.len() { return Err(())}
                    last_index = index+1;
                }
                _=>{}
            }
        }
        if let Some(k) = key {
            map.insert(
                Some(unsafe{std::str::from_utf8_unchecked(k)}),
                &bytes[last_index..]
            );
        }
       Ok(
           HeaderVWithParams {
               data:bytes,
               value: match value {
                   None => {unsafe{std::str::from_utf8_unchecked(bytes)}}
                   Some(v) => {v}
               },
               params:map
           }
       )
    }


    /// getting the  value as [`&str`]
    pub fn to_str(&self)->&'buf str{
        self.value
    }

    /// return the whole value as str
    pub fn whole_value_as_str(&self)->&'buf str{
        unsafe {std::str::from_utf8_unchecked(self.data)}
    }


    /// returning header value parameter
    pub fn get_param(&self,k:&str) -> Option<&'buf [u8]> {
        if let Some(data) = self.params.get(&Some(k)) {
            return Some(*data)
        }
        else {
            for (key,value) in &self.params {
                if key.is_some() {continue}
                if *value == k.as_bytes() { return Some(*value)}
            }
        }
        None
    }
}


