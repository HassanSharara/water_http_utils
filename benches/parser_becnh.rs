
use criterion::{black_box, criterion_group, criterion_main, Criterion};
// Replace 'your_crate' with your actual crate name from Cargo.toml
use water_http_utils::request::HttpRequest;
use httparse;

const RAW_DATA: &[u8] = b"GET /search?q=rust+http+server HTTP/1.1\r\n\
                        Host: search.example.com\r\n\
                        User-Agent: RustTestClient/2.0\r\n\
                        Accept: */*\r\n\
                        Cache-Control: no-cache\r\n\
                        X-Forwarded-For: 192.168.0.1\r\n\
                        \r\n";

fn bench_parsers(c: &mut Criterion) {
    let mut group = c.benchmark_group("HTTP Parsing");

    // 1. Benchmark Your Parser
    // Note: HC (Header Count) is set to 16 to match your tests
    group.bench_function("water_http_parser", |b| {
        b.iter(|| {
            let res = HttpRequest::<16>::from_bytes::<16>(black_box(RAW_DATA));
            black_box(res).unwrap();
        })
    });

    // 2. Benchmark httparse
    group.bench_function("httparse", |b| {
        b.iter(|| {
            let mut headers = [httparse::EMPTY_HEADER; 16];
            let mut req = httparse::Request::new(&mut headers);
            let res = req.parse(black_box(RAW_DATA));
            black_box(res).unwrap();
        })
    });

    group.finish();
}

criterion_group!(benches, bench_parsers);
criterion_main!(benches);