use water_http_utils::request::HttpRequest;

fn main() {

    let re = b"GET /path?q=2&a=1&2=2&&ss=2 HTTP/1.1\r\nHost: example.com\r\nUser-Agent: MyClient/1.0\r\nAccept: */*\r\n\r\n";

    let req = HttpRequest::<16>::from_incoming_bytes::<16>(re);
    if let Ok( req ) = req {
        let path = req.path();
        println!("{:?}",path.to_query());
    }
}