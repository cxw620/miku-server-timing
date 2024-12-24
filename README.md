# miku-server-timing

[![Latest Version](https://img.shields.io/crates/v/miku-server-timing.svg)](https://crates.io/crates/miku-server-timing)

An axum layer to inject the `Server-Timing` HTTP header into the response.

For a reference on the header please see [developer.mozilla.org](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Server-Timing).

## Examples

Using the layer to inject the `Server-Timing` Header.

```rust
    let app = Router::new()
        .route("/", get(handler))
        .layer(miku_server_timing::ServerTimingLayer::new("HelloService"));
```

```http
HTTP/1.1 200 OK
content-type: text/html; charset=utf-8
content-length: 22
server-timing: HelloService;dur=102.0
date: Wed, 19 Apr 2023 15:25:40 GMT

<h1>Hello, World!</h1>
```

Using the layer to inject the Server-Timing Header with description.

```rust
    let app = Router::new()
        .route("/", get(handler))
        .layer(
            miku_server_timing::ServerTimingLayer::new("HelloService")
                .with_description("whatever")
        );
```

```http
HTTP/1.1 200 OK
content-type: text/html; charset=utf-8
content-length: 22
server-timing: HelloService;desc="whatever";dur=102.0
date: Wed, 19 Apr 2023 15:25:40 GMT

<h1>Hello, World!</h1>
```

## Special thanks

[axum-server-timing](https://github.com/JensWalter/axum-server-timing)

This crate is a fork of the above crate, modified to gain better performance, and can serve as a simple replacement.
