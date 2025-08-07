# Monero crawler library

## Status of developmenent

First version  0.1.0 works, use at your own risk.

## About

A Monero crawler library written in Rust

## Objective

Gather data about peers in the monero network in an efficient way

## Features

- [x] check peers for capabilities and filter them

## Installation

```
cargo add --git https://github.com/Cyrix126/monero-crawler-rs monero-crawler-lib
```

## Usage

Use the builder `CrawlBuilder` to build a `Crawl` that can run `discover_peers()`

See the documentation for the settable fields to customize the behavior of the crawler and the type of peers to return. 

## Example

See the [example](./examples/crawl.rs) that crawl the network to return p2pool compatible nodes.

```
cargo run --release --example crawl
```

## Bug Reporting

Open an [issue](https://github.com/Cyrix126/monero-crawler-rs/issue) or contact me by email through [email](mailto:gupaxx@baermail.fr)

## Contributing

You can contribute by opening a PR.


## Security

## Documentation

```
cargo doc --open
```

## License

![GPL v3](assets/images/gplv3-with-text-136x68.png)

[See the licenses of various dependencies.](https://github.com/Cyrix126/monero-crawler-rs/blob/main/server/crawler/Cargo.toml)

### Donations
If you'd like to thank me for the development of monero-crawler-rs and/or motivate me to improve it you're welcome to send any amount of XMR to the following address:

![QR CODE DONATION ADDRESS](assets/donation_qr.png)
```
4AGJScWSv45E28pmwck9YRP21KuwGx6fuMYV9kTxXFnWEij5FVEUyccBs7ExDy419DJXRPw3u57TH5BaGbsHTdnf6SvY5p5
```

Every donations will be converted to hours of work !

#### Donation transparency

A Kuno page exist so you can easily keep track of the amount funded in this project.

This project is used for Gupaxx, donations will be sent to the same address.
  
[Gupaxx Kuno](https://kuno.anne.media/fundraiser/dsrr/)  
In case you don't want to rely on the kuno website, the secret view key is:  

```
6c6f841e1eda3fba95f2261baa4614e3ec614af2a97176bbae2c0be5281d1d0f
```
