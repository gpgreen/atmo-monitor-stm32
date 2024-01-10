//! Reading the Plantower PMS7003 sensor

use defmt::{debug, info, Format};
use embassy_time::{Duration, Timer};
//use embedded_io_async::{Read, Write};
use crate::{parameter::Parameters, DisplayInfo};
use embassy_stm32::{peripherals, usart};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Sender;
use pms_7003::{async_interface::Pms7003SensorAsync, Error};

/// Data from the sensor
#[derive(Debug, Default, Clone, Copy, Format)]
pub struct PmSensorData {
    pub pm1_0: u16,
    pub pm2_5: u16,
    pub pm10: u16,
    pub pm1_0_atm: u16,
    pub pm2_5_atm: u16,
    pub pm10_atm: u16,
}

impl PmSensorData {
    /// copy data from a frame
    pub fn copy_from_frame(frame: &pms_7003::OutputFrame) -> Self {
        Self {
            pm1_0: frame.pm1_0,
            pm2_5: frame.pm2_5,
            pm10: frame.pm10,
            pm1_0_atm: frame.pm1_0_atm,
            pm2_5_atm: frame.pm2_5_atm,
            pm10_atm: frame.pm10_atm,
        }
    }
}

///task to read pm2.5 sensor data
#[embassy_executor::task]
pub async fn pm25_controller(
    mut dev: Pms7003SensorAsync<usart::BufferedUart<'static, peripherals::USART1>>,
    sender: Sender<'static, NoopRawMutex, DisplayInfo, 2>,
    _params: Parameters,
) {
    info!("starting pm2.5 loop");
    loop {
        //info!("sending passive cmd to sensor");
        //dev.passive().await.unwrap();
        let frame = dev.read().await.unwrap();
        debug!(
            "PM1_0: {} PM2_5: {} PM10: {} PM1_0_atm: {} PM2_5_atm {}, PM10_atm {}",
            frame.pm1_0, frame.pm2_5, frame.pm10, frame.pm1_0_atm, frame.pm2_5_atm, frame.pm10_atm,
        );
        sender
            .send(DisplayInfo::Pms7003Data(PmSensorData::copy_from_frame(
                &frame,
            )))
            .await;
        Timer::after(Duration::from_millis(250)).await;
    }
}

// /// Structure for PMS7003 device attached to Serial
// pub struct PMS7003Device<Serial> {
//     dev: Pms7003SensorAsync<Serial>,
// }

// impl<Serial> PMS7003Device<Serial>
// where
//     Serial: embedded_io_async::Read + embedded_io_async::Write + embedded_io_async::ErrorType,
// {
//     pub fn new(usart: Serial) -> Self {
//         Self {
//             dev: Pms7003SensorAsync::new(usart),
//         }
//     }

//     pub fn sleep(&mut self) -> Result<(), Error> {
//         self.dev.sleep().after?
//     }

//     pub fn wake(&mut self) -> Result<(), Error> {
//         self.dev.wake().after?
//     }

//     pub async fn passive_read(&mut self) -> Result<(), Error> {
//         self.dev.passive().after?;
//         Timer::after(Duration::from_secs(1)).await;
//         let frame = self.dev.read()?;
//         debug!("frame received {:?}", frame);
//         Ok(())
//     }
// }
