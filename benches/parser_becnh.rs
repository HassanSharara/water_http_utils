use std::time::Duration;
use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion, Throughput, BatchSize};

// Only import what exists
use water_http_utils::request::HttpRequest;

const REQ_SHORT: &[u8] = b"GET / HTTP/1.0\r\nHost: example.com\r\nCookie: session=60; user_id=1\r\n\r\n";

const REQ: &[u8] = b"\
GET /wp-content/uploads/2010/03/hello-kitty-darth-vader-pink.jpg HTTP/1.1\r\n\
Host: www.kittyhell.com\r\n\
User-Agent: Mozilla/5.0 (Macintosh; U; Intel Mac OS X 10.6; ja-JP-mac; rv:1.9.2.3) Gecko/20100401 Firefox/3.6.3 Pathtraq/0.9\r\n\
Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8\r\n\
Accept-Language: ja,en-us;q=0.7,en;q=0.3\r\n\
Accept-Encoding: gzip,deflate\r\n\
Accept-Charset: Shift_JIS,utf-8;q=0.7,*;q=0.7\r\n\
Keep-Alive: 115\r\n\
Connection: keep-alive\r\n\
Cookie: wp_ozh_wsa_visits=2; wp_ozh_wsa_visit_lasttime=xxxxxxxxxx; __utma=xxxxxxxxx.xxxxxxxxxx.xxxxxxxxxx.xxxxxxxxxx.xxxxxxxxxx.x; __utmz=xxxxxxxxx.xxxxxxxxxx.x.x.utmccn=(referral)|utmcsr=reader.livedoor.com|utmcct=/reader/|utmcmd=referral|padding=under256\r\n\r\n";

fn req(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_full");
    group.throughput(Throughput::Bytes(REQ.len() as u64));

    // Benchmark httparse
    group.bench_function("httparse", |b| b.iter_batched_ref(|| {
        [httparse::EMPTY_HEADER; 16]
    }, |headers| {
        let mut req = httparse::Request::new(headers);
        let _ = black_box(req.parse(REQ));
    }, BatchSize::SmallInput));

    // Benchmark water_http_parser
    group.bench_function("water_http_parser", |b| {
        b.iter(|| {
            let res = HttpRequest::<16>::from_bytes::<16>(black_box(REQ));
            let _ = black_box(res);
        })
    });
    group.finish();
}

fn req_short(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_short");
    group.throughput(Throughput::Bytes(REQ_SHORT.len() as u64));

    group.bench_function("httparse", |b| b.iter_batched_ref(|| {
        [httparse::EMPTY_HEADER; 16]
    }, |headers| {
        let mut req = httparse::Request::new(headers);
        let _ = black_box(req.parse(REQ_SHORT));
    }, BatchSize::SmallInput));

    group.bench_function("water_http_parser", |b| {
        b.iter(|| {
            let res = HttpRequest::<16>::from_bytes::<16>(black_box(REQ_SHORT));
            let _ = black_box(res);
        })
    });
    group.finish();
}

fn header(c: &mut Criterion) {
    let mut group = c.benchmark_group("header_internal_httparse");
    const XFOOBAR: &[u8] = b"X-Foobar";
    let xfoobar_4k = XFOOBAR.repeat(4096/XFOOBAR.len());

    for p in 0..=12 {
        let n = 1 << p;
        let payload = [&xfoobar_4k[..n], b": b\r\n\r\n"].concat().leak();
        group.throughput(Throughput::Bytes(payload.len() as u64));
        group.bench_function(format!("name_{:04}b", n), |b| {
            b.iter_batched_ref(
                || [httparse::EMPTY_HEADER; 128],
                |headers| {
                    // FIX: We discard the result to avoid lifetime issues with the returned reference
                    let _ = black_box(httparse::parse_headers(black_box(payload), headers));
                },
                BatchSize::SmallInput
            )
        });
    }
    group.finish();
}

// Stub functions to keep the target list clean
fn uri(_c: &mut Criterion) {}
fn version(_c: &mut Criterion) {}
fn method(_c: &mut Criterion) {}
fn many_requests(_c: &mut Criterion) {}

const WARMUP: Duration = Duration::from_millis(100);
const MTIME: Duration = Duration::from_millis(100);
const SAMPLES: usize = 200;

criterion_group!{
    name = benches;
    config = Criterion::default().sample_size(SAMPLES).warm_up_time(WARMUP).measurement_time(MTIME);
    targets = req, req_short, header, uri, version, method, many_requests
}
criterion_main!(benches);