//! atmo-monitor-stm32
//! Library for atmospheric sensing device
//! Device includes e-ink screen to show results
//! Sensors are BME680 for temp/press and
//! Plantower PMS 7003 for particulate measurement

#![no_main]
#![no_std]

mod fmt;

#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

// our hal
use embassy_stm32 as _;

// library modules
pub mod bme680_device;
pub mod parameter;
pub mod pms7003_device;
pub mod screen;

/// Enumeration passed on channel to display controller
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DisplayInfo {
    Bme680Data(bme680_device::Bme680Data),
    Pms7003Data(pms7003_device::PmSensorData),
}

// defmt-test 0.3.0 has the limitation that this `#[tests]` attribute can only be used
// once within a crate. the module can be in any file but there can only be at most
// one `#[tests]` module in this library crate
#[cfg(test)]
#[defmt_test::tests]
mod unit_tests {
    use defmt::assert;

    #[test]
    fn it_works() {
        assert!(true)
    }
}
