//! Reading the BME680 sensor

use crate::fmt::debug;
use bme680::{
    self, Bme680, I2CAddress, IIRFilterSize, OversamplingSetting, PowerMode, SettingsBuilder,
};
use core::fmt;
use embassy_time::{Delay, Duration};
use embedded_hal::blocking::delay::DelayMs;
use embedded_hal::blocking::i2c::{Read, Write};

/// Data sensed by the BME680 device
#[derive(Debug, Default, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Bme680Data {
    pub temperature: f32,
    pub humidity: f32,
    pub pressure: f32,
    pub gas_resistance: u32,
    pub gas_valid: bool,
    pub heat_stable: bool,
}

/// Structure for BME680 device attached to I2C bus
pub struct BmeDevice<I2C> {
    dev: Bme680<I2C, embassy_time::Delay>,
    profile_duration: Duration,
}

impl<I2C> BmeDevice<I2C>
where
    I2C: Read + Write,
    <I2C as Read>::Error: fmt::Debug,
    <I2C as Write>::Error: fmt::Debug,
{
    /// Create a new BmeDevice, do not initialize it yet
    /// due to the bme680 module, there is some i2c traffic on the bus
    /// during this method
    pub fn new(
        i2c: I2C,
    ) -> Result<BmeDevice<I2C>, bme680::Error<<I2C as Read>::Error, <I2C as Write>::Error>> {
        let mut delayer = Delay;
        Ok(BmeDevice {
            dev: Bme680::init(i2c, &mut delayer, I2CAddress::Secondary)?,
            profile_duration: Duration::from_secs(0),
        })
    }

    /// Initialize the BmeDevice so it can read data
    pub fn init(
        &mut self,
    ) -> Result<(), bme680::Error<<I2C as Read>::Error, <I2C as Write>::Error>> {
        let settings = SettingsBuilder::new()
            .with_humidity_oversampling(OversamplingSetting::OS2x)
            .with_pressure_oversampling(OversamplingSetting::OS4x)
            .with_temperature_oversampling(OversamplingSetting::OS8x)
            .with_temperature_filter(IIRFilterSize::Size3)
            .with_gas_measurement(Duration::from_millis(1500).into(), 320, 25)
            .with_run_gas(true)
            .build();
        let mut delayer = Delay;
        self.dev.set_sensor_settings(&mut delayer, settings)?;
        if let Ok(d) = Duration::try_from(self.dev.get_profile_dur(&settings.0)?) {
            self.profile_duration = d;
            debug!("bme680 delay: {}ms", self.profile_duration);
            debug!("bme680 initialized");
            Ok(())
        } else {
            panic!("Unable to create duration from value stored in sensor");
        }
    }

    /// Read data from the BmeDevice
    pub fn read(
        &mut self,
    ) -> Result<Bme680Data, bme680::Error<<I2C as Read>::Error, <I2C as Write>::Error>> {
        let mut delayer = Delay;
        // Read sensor data
        self.dev
            .set_sensor_mode(&mut delayer, PowerMode::ForcedMode)?;
        delayer.delay_ms(self.profile_duration.as_millis() as u8);
        let (data, _state) = self.dev.get_sensor_data(&mut delayer)?;

        let reading = Bme680Data {
            temperature: data.temperature_celsius(),
            humidity: data.humidity_percent(),
            pressure: data.pressure_hpa(),
            gas_resistance: data.gas_resistance_ohm(),
            gas_valid: data.gas_valid(),
            heat_stable: data.heat_stable(),
        };
        debug!("Temperature {}°C", reading.temperature);
        debug!("Pressure {}hPa", reading.pressure);
        debug!("Humidity {}%", reading.humidity);
        debug!("Gas Resistance {}Ω", reading.gas_resistance);
        debug!(
            "gas valid: {} gas heater stable: {}",
            reading.gas_valid, reading.heat_stable
        );
        Ok(reading)
    }
}
