# `atmo-monitor-32`

Device to monitor atmosphere. Measures temperature, pressure,
humidity, VOC, and pm2.5. Hosted on a [Nucleo-F303RE] development
board. Device has a small eInk display to show these
variables.

## Hardware

- [Nucleo-F303RE](https://www.st.com/en/evaluation-tools/nucleo-f303re.html)
- [Adafruit BME680 breakout](http://adafru.it/3660)
- [Adafruit Tri-Color eInk](https://www.adafruit.com/product/4086)
- [Plantower PM2.5 Sensor PMS7003](https://plantower.com/en/products_33/76.html)

### Dev Board Hardware Pin assignments

| MCU Pin | Logic Signal | Nucleo CN-No |
|--------:|-------------:|-------------:|
|   PB8   |  SCL (I2C1)  |    CN5-10    |
|   PB9   |  SDA (I2C1)  |    CN5-9     |
|   PA5   |  SCK (SPI1)  |    CN5-6     |
|   PA6   |  MISO (SPI1) |    CN5-5     |
|   PA7   |  MOSI (SPI1) |    CN5-4     |
|   PB6   |  ECS         |    CN5-3     |
|   PC7   |  D/C         |    CN5-2     |
|   PA9   |  RST         |    CN5-1     |
|   PA8   |  BUSY        |    CN9-8     |
|  PB10   |  SRCS (1)    |    CN9-7     |
|   PB5   |  SDCS (1)    |    CN9-5     |
|   PC4   |  TX (USART1) |   CN10-34    |
|   PC5   |  RX (USART1) |   CN10-6     |
|         |  ENA         |              |
|  PC10   |  TX (USART3) |    CN7-1     |
|  PC11   |  RX (USART3) |    CN7-2     |

### PMS7003 Sensor Cable Wire Connections

| Wire Co | Logic Signal | Pin No |
|--------:|-------------:|-------:|
|    Blue |          VCC |      1 |
|   Black |          VCC |      2 |
|   White |          GND |      3 |
|    Grey |          GND |      4 |
|  Purple |        Reset |      5 |
|   Green |          N/C |      6 |
|  Yellow |           RX |      7 |
|  Orange |          N/C |      8 |
|     Red |           TX |      9 |
|   Brown |          Set |     10 |


## Dependencies

#### 1. `flip-link`:

```console
$ cargo install flip-link
```

#### 2. `probe-rs`:

``` console
$ # make sure to install v0.2.0 or later
$ cargo install probe-rs --features cli
```

#### 7. Run!

You are now all set to `cargo-run` your first `defmt`-powered application!
There are some examples in the `src/bin` directory.

Start by `cargo run`-ning `my-app/src/bin/hello.rs`:

``` console
$ # `rb` is an alias for `run --bin`
$ cargo rb hello
    Finished dev [optimized + debuginfo] target(s) in 0.03s
flashing program ..
DONE
resetting device
0.000000 INFO Hello, world!
(..)

$ echo $?
0
```

If you're running out of memory (`flip-link` bails with an overflow error), you can decrease the size of the device memory buffer by setting the `DEFMT_RTT_BUFFER_SIZE` environment variable. The default value is 1024 bytes, and powers of two should be used for optimal performance:

``` console
$ DEFMT_RTT_BUFFER_SIZE=64 cargo rb hello
```

## Running tests

The template comes configured for running unit tests and integration tests on the target.

Unit tests reside in the library crate and can test private API; the initial set of unit tests are in `src/lib.rs`.
`cargo test --lib` will run those unit tests.

``` console
$ cargo test --lib
(1/1) running `it_works`...
└─ app::unit_tests::__defmt_test_entry @ src/lib.rs:33
all tests passed!
└─ app::unit_tests::__defmt_test_entry @ src/lib.rs:28
```

Integration tests reside in the `tests` directory; the initial set of integration tests are in `tests/integration.rs`.
`cargo test --test integration` will run those integration tests.
Note that the argument of the `--test` flag must match the name of the test file in the `tests` directory.

``` console
$ cargo test --test integration
(1/1) running `it_works`...
└─ integration::tests::__defmt_test_entry @ tests/integration.rs:13
all tests passed!
└─ integration::tests::__defmt_test_entry @ tests/integration.rs:8
```

Note that to add a new test file to the `tests` directory you also need to add a new `[[test]]` section to `Cargo.toml`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)

- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
licensed as above, without any additional terms or conditions.

[Knurling]: https://knurling.ferrous-systems.com
[Ferrous Systems]: https://ferrous-systems.com/
[GitHub Sponsors]: https://github.com/sponsors/knurling-rs
