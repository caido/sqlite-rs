use rand::{rngs::StdRng, Rng, SeedableRng};

use super::types::{PayloadKind, Sample, SizeBucket};

const HOSTS: &[&str] = &[
    "api.example.com",
    "www.shop.local",
    "cdn.assets.io",
    "app.google.test",
];

const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH"];

const PATHS: &[&str] = &[
    "/api/v1/users",
    "/api/v1/orders",
    "/api/v1/sessions",
    "/api/v2/products",
    "/graphql",
    "/login",
    "/search",
];

const USERNAMES: &[&str] = &[
    "alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi",
];

pub fn generate_stream(seed: u64, count: u64) -> impl Iterator<Item = Sample> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..count).map(move |i| {
        let kind = PayloadKind::from_weight(rng.gen());
        let size_bucket = SizeBucket::from_weight(rng.gen());
        let bytes = generate_sample(&mut rng, i, kind, size_bucket);
        Sample {
            kind,
            size_bucket,
            bytes,
        }
    })
}

pub fn generate_sample(
    rng: &mut impl Rng,
    index: u64,
    kind: PayloadKind,
    size_bucket: SizeBucket,
) -> Vec<u8> {
    let (min, max) = size_bucket.target_range();
    let target = rng.gen_range(min..=max);

    let body = match kind {
        PayloadKind::Json => json_body(rng, index, target),
        PayloadKind::Html => html_body(rng, index, target),
        PayloadKind::Form => form_body(rng, index, target),
    };

    wrap_http(rng, index, kind, &body)
}

fn wrap_http(rng: &mut impl Rng, index: u64, kind: PayloadKind, body: &[u8]) -> Vec<u8> {
    let method = METHODS[rng.gen_range(0..METHODS.len())];
    let host = HOSTS[rng.gen_range(0..HOSTS.len())];
    let path = PATHS[rng.gen_range(0..PATHS.len())];
    let content_type = match kind {
        PayloadKind::Json => "application/json",
        PayloadKind::Html => "text/html; charset=utf-8",
        PayloadKind::Form => "application/x-www-form-urlencoded",
    };

    let mut out = Vec::with_capacity(body.len() + 256);
    out.extend_from_slice(
        format!(
            "{method} {path}/{index} HTTP/1.1\r\n\
             Host: {host}\r\n\
             User-Agent: sqlite-compress-bench/0.1\r\n\
             Accept: */*\r\n\
             Content-Type: {content_type}\r\n\
             Content-Length: {}\r\n\
             X-Request-Id: {index:016x}\r\n\
             Connection: keep-alive\r\n\
             \r\n",
            body.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(body);
    out
}

fn json_body(rng: &mut impl Rng, index: u64, target: usize) -> Vec<u8> {
    let user = USERNAMES[rng.gen_range(0..USERNAMES.len())];
    let mut body = format!("{{\"id\":{index},\"user\":\"{user}\",\"status\":\"ok\",\"items\":[");

    let mut item = 0u32;
    while body.len() + 80 < target {
        if item > 0 {
            body.push(',');
        }
        let price = rng.gen_range(1..10_000);
        body.push_str(&format!(
            "{{\"sku\":\"SKU-{item:04}\",\"qty\":{},\"price\":{price}}}",
            rng.gen_range(1..20)
        ));
        item += 1;
    }

    body.push_str("],\"meta\":{\"source\":\"bench\",\"version\":1}}");
    pad_to_target(body.into_bytes(), target, b' ')
}

fn html_body(rng: &mut impl Rng, index: u64, target: usize) -> Vec<u8> {
    let title = USERNAMES[rng.gen_range(0..USERNAMES.len())];
    let mut body = format!(
        "<!DOCTYPE html><html><head><title>{title}-{index}</title></head><body>\
         <header><h1>Order {index}</h1></header><main><ul>"
    );

    let mut item = 0u32;
    while body.len() + 64 < target.saturating_sub(40) {
        body.push_str(&format!(
            "<li class=\"item-{item}\" data-id=\"{index}-{item}\">Product {item} — {}</li>",
            rng.gen_range(1..9999)
        ));
        item += 1;
    }

    body.push_str("</ul></main><footer>bench</footer></body></html>");
    pad_to_target(body.into_bytes(), target, b' ')
}

fn form_body(rng: &mut impl Rng, index: u64, target: usize) -> Vec<u8> {
    let user = USERNAMES[rng.gen_range(0..USERNAMES.len())];
    let token: u64 = rng.gen();
    let mut body =
        format!("username={user}&token={token:016x}&redirect=%2Fdashboard&request_id={index}");

    let mut field = 0u32;
    while body.len() + 32 < target {
        body.push_str(&format!("&field{field}={}", rng.gen_range(0..1_000_000)));
        field += 1;
    }

    pad_to_target(body.into_bytes(), target, b'0')
}

fn pad_to_target(mut bytes: Vec<u8>, target: usize, pad: u8) -> Vec<u8> {
    if bytes.len() < target {
        bytes.resize(target, pad);
    }
    bytes
}
