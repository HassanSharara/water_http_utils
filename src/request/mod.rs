mod enums;
pub mod headers;
mod first_line;

use std::collections::HashMap;
use std::fmt::Debug;
#[cfg(feature = "write_logs")]
use std::io::Write;
/// using all first line implementations
pub use first_line::*;
use crate::request::CreatingRequestErrors::InvalidHeadersError;
use crate::request::enums::CreatingRequestSteps;
use crate::request::headers::{ CreatingHeadersErrors, HttpHeaders};

/// for parsing http request bytes
#[derive(Debug)]
pub struct HttpRequest<'buf,const HC:usize>{
    http_first_line: HttpFirstLine<'buf>,
    headers:HttpHeaders<'buf,HC>,

}

impl<'buf,const HC:usize> HttpRequest<'buf, HC> {


    #[cfg(feature = "server")]
    /// getting http request method
    pub fn method(&self)->&'buf str{
        self.http_first_line.method
    }
    /// getting http request version
    pub fn version(&self)->&'buf str{
        self.http_first_line.version
    }
    /// getting http request path
    pub fn path(&self)->&'buf HttpPath{
        &self.http_first_line.path
    }


    /// http first line
    pub fn first_line(&self)->&HttpFirstLine<'buf>{
        &self.http_first_line
    }
    /// returning all request headers referenced
    pub fn headers(&self)->&HttpHeaders<'buf,HC>{
        &self.headers
    }

    /// creating http request structure from given bytes with zero copies
    #[cfg(feature = "server")]
    pub  fn from_incoming_bytes<const N:usize>(mut bytes:&'buf [u8])->Result<HttpRequest<'buf,N>,CreatingRequestErrors>{
        let mut step = CreatingRequestSteps::init();
         let mut first_line = None;

        #[cfg(feature = "write_logs")]
        let path =  format!("./logs/{}.txt",chrono::Local::now().format("%Y%m%d_%H%M%S").to_string());
        #[cfg(feature = "write_logs")]
        let mut file = std::fs::File::create(
            path.as_str()
        ).expect(format!("can not create log file with {path}").as_str());
        #[cfg(feature = "write_logs")]
        {


            file.write(format!("\n\n method invoked : HttpRequest::from_incoming_bytes \n bytes: \n {:?} \n\n",
             String::from_utf8_lossy(bytes)
            ).as_bytes()).unwrap();
        }
        loop {
            #[cfg(feature = "write_logs")]
            {
                file.write(format!("\r\nmatching step start : {:?} \r\n",step).as_bytes()).unwrap();
            }
            match step {
                CreatingRequestSteps::FirstLine => {
                    let fl = HttpFirstLine::from_server(bytes)?;
                    let index:usize = fl.first_line_length-1;
                    first_line = Some(fl);
                    #[cfg(feature = "write_logs")]
                    {
                        file.write(format!("\r\n first line bytes detected: {:?} \r\n while left is   {:?} \r\n",
                         String::from_utf8_lossy(&bytes[..index]),
                         String::from_utf8_lossy(&bytes[index..]),
                        ).as_bytes()).unwrap();

                    }

                    bytes = &bytes[index..];
                    step = CreatingRequestSteps::Headers;
                }
                CreatingRequestSteps::Headers => {
                    return match HttpHeaders::new(bytes) {
                        Ok(h) => {

                            #[cfg(feature = "write_logs")]
                            {
                                file.write(format!("\r\n headers bytes detected: {:?} \r\n while left is   {:?} \r\n",
                                                   String::from_utf8_lossy(&bytes[..h.headers_length]),
                                                   String::from_utf8_lossy(&bytes[h.headers_length..]),
                                ).as_bytes()).unwrap();

                            }
                            let first_line = first_line.unwrap();
                            Ok(HttpRequest {
                                http_first_line: first_line,
                                headers:h,
                            })
                        }
                        Err(e) => {

                            match  e {
                                CreatingHeadersErrors::ReadMore => {
                                    return Err(
                                        CreatingRequestErrors::InsufficientDataSoReadMore
                                    )
                                }
                                _ => {
                                    InvalidHeadersError(
                                        e
                                    ).into()
                                }
                            }

                        }
                    }
                }
            }
        }
    }



    /// creating http request with fast
    #[cfg(feature = "server")]

    pub fn from_bytes<const N:usize>(bytes:&'buf [u8])->Result<HttpRequest<'buf,N>,CreatingRequestErrors>{

        let first_line = HttpFirstLine::from_server(bytes)?;

        let headers = HttpHeaders::<N>::new(&bytes[first_line.first_line_length-1..])?;

        Ok(
            HttpRequest {
                headers,
                http_first_line:first_line,
            }
        )
    }
}


/// creating request results
#[derive(Debug)]
pub enum CreatingRequestErrors{
    /// for returning invalid http bytes
    InvalidHttpFormat,

    /// if there is no sufficient data to be valid
    InsufficientDataSoReadMore,

    /// when someone trying to attack your server
    DangerousInvalidHttpFormat,
    /// when parsing http headers contains errors
    InvalidHeadersError(CreatingHeadersErrors)
}

impl<R> Into<Result<R,CreatingRequestErrors>> for CreatingRequestErrors {
    fn into(self) -> Result<R, CreatingRequestErrors> {
        Err(self)
    }
}

/// http path structure
#[derive(Debug)]
pub struct HttpPath<'buf> {
    bytes:&'buf [u8],
}

impl<'buf> HttpPath<'buf> {

    pub (crate) fn new(bytes:&'buf [u8])->HttpPath<'buf>{
        HttpPath {
            bytes
        }
    }

    /// converting total path to ['&str']
    pub fn to_str(&self) -> &'buf str {
        std::str::from_utf8(self.bytes).unwrap()
    }

    /// returning the actual bytes of path
    pub fn get_bytes(&self)->&'buf [u8]{
        self.bytes
    }
    /// forming path to path and query
    pub fn split_to_path_and_query(&self) -> (String, HashMap<String, String>) {
        let url = self.to_str();
        // Split the URL into path and query string
        let parts: Vec<&str> = url.splitn(2, '?').collect();
        let path = parts[0].to_string();

        let mut query_params = HashMap::new();
        if parts.len() > 1 {
            for pair in parts[1].split('&') {
                let kv: Vec<&str> = pair.splitn(2, '=').collect();
                if kv.len() == 2 {
                    query_params.insert(
                        kv[0].to_string(),
                        kv[1].to_string()
                    );
                }
            }
        }

        (path, query_params)
    }
}



#[cfg(test)]
mod test {
    use crate::request::HttpRequest;


    fn generate_requests() -> Vec<Vec<u8>> {
        vec![
            b"GET /home HTTP/1.1\r\nHost: example.com\r\nConnection: keep-alive\r\n\r\n".to_vec(),

            b"POST /submit HTTP/1.1\r\nHost: example.com\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 13\r\n\r\nname=Hassan"
                .to_vec(),

            b"PUT /user/123 HTTP/1.1\r\nHost: example.com\r\nContent-Type: application/json\r\nContent-Length: 17\r\n\r\n{\"age\":30}"
                .to_vec(),

            b"DELETE /post/9 HTTP/1.1\r\nHost: example.com\r\n\r\n".to_vec(),

            b"HEAD /ping HTTP/1.1\r\nHost: example.com\r\n\r\n".to_vec(),

            b"OPTIONS /api HTTP/1.1\r\nHost: example.com\r\nAllow: GET, POST\r\n\r\n".to_vec(),

            b"PATCH /user/5 HTTP/1.1\r\nHost: example.com\r\nContent-Length: 11\r\n\r\n{\"x\":true}"
                .to_vec(),

            b"GET /search?q=rust HTTP/1.1\r\nHost: example.com\r\nUser-Agent: TestAgent\r\n\r\n"
                .to_vec(),
        ]
    }
    #[test]
    fn high_volume_request_bytes_test() {
        let requests = generate_requests();
        for i in 0..100 {
            let req = &requests[i % requests.len()];
            check_request(req);
        }
    }

    #[test]
    fn test_get_request() {
        let r_bytes = b"GET /home HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n";
        check_request(r_bytes);
    }

    #[test]
    fn test_post_request() {
        let r_bytes = b"POST /submit HTTP/1.1\r\nHost: example.com\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 27\r\n\r\nusername=test&password=1234";
        check_request(r_bytes);
    }

    #[test]
    fn test_put_request() {
        let r_bytes = b"PUT /update HTTP/1.1\r\nHost: example.com\r\nAuthorization: Token 987654\r\nContent-Length: 15\r\n\r\n{\"data\":42}";
        check_request(r_bytes);
    }

    #[test]
    fn test_delete_request() {
        let r_bytes = b"DELETE /remove/123 HTTP/1.1\r\nHost: api.example.com\r\nAuthorization: Bearer abcdef\r\n\r\n";
        check_request(r_bytes);
    }

    #[test]
    fn test_request_with_multiple_headers() {
        let r_bytes = b"GET /profile HTTP/1.1\r\nHost: example.com\r\nUser-Agent: RustTestClient/2.0\r\nAccept: */*\r\nCache-Control: no-cache\r\nX-Forwarded-For: 192.168.0.1\r\n\r\n";
        check_request(r_bytes);
    }

    #[test]
    fn test_request_with_query_parameters() {
        let r_bytes = b"GET /search?q=rust+http+server HTTP/1.1\r\nHost: search.example.com\r\n\r\n";
        check_request(r_bytes);
    }

    #[test]
    fn test_request_with_custom_headers() {
        let r_bytes = b"GET /custom HTTP/1.1\r\nHost: api.example.com\r\nX-Custom-Header: MyValue\r\nX-Request-ID: 12345\r\n\r\n";
        check_request(r_bytes);
    }

    #[test]
    fn test_request_with_gzip_encoding() {
        let r_bytes = b"GET /compressed HTTP/1.1\r\nHost: example.com\r\nAccept-Encoding: gzip, deflate\r\n\r\n";
        check_request(r_bytes);
    }

    #[test]
    fn test_request_with_json_body() {
        let r_bytes = b"POST /json HTTP/1.1\r\nHost: api.example.com\r\nContent-Type: application/json\r\nContent-Length: 36\r\n\r\n{\"username\":\"test_user\",\"id\":42}";
        check_request(r_bytes);
    }

    #[test]
    fn test_request_with_form_body() {
        let r_bytes = b"POST /form HTTP/1.1\r\nHost: example.com\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 21\r\n\r\nname=John&age=30";
        check_request(r_bytes);
    }

    fn check_request(r_bytes: &[u8]) {
        let request = HttpRequest::<16>::from_bytes::<16>(r_bytes);
        match &request {
            Ok(req) => {
                println!("Method: {:?}", req.method());
                println!("Version: {:?}", req.version());
                println!("Path: {:?}", req.path().to_str());
                println!("Headers:");
                for line in req.headers.lines() {
                    println!("  {}: {:?}", line.key, line.value.to_str());
                }
            }
            Err(e) => {
                println!("Error parsing request: {:?}", e);
            }
        }
        assert!(request.is_ok());
    }
}




