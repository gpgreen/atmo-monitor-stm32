//! Reading the Plantower PMS7003 sensor

use crate::{parameter::Parameters, DisplayInfo};
use defmt::{debug, info, Format};
use embassy_stm32::{peripherals, usart};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Sender;
use embassy_time::{Duration, Timer};
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

    /// calculate moving average from array of PmSensorData
    pub fn average(data: &[PmSensorData]) -> Self {
        let mut pm1_0_avg: u32 = 0;
        let mut pm2_5_avg: u32 = 0;
        let mut pm10_avg: u32 = 0;
        let mut pm1_0_atm_avg: u32 = 0;
        let mut pm2_5_atm_avg: u32 = 0;
        let mut pm10_atm_avg: u32 = 0;
        for d in data {
            pm1_0_avg += d.pm1_0 as u32;
            pm2_5_avg += d.pm2_5 as u32;
            pm10_avg += d.pm10 as u32;
            pm1_0_atm_avg += d.pm1_0_atm as u32;
            pm2_5_atm_avg += d.pm2_5_atm as u32;
            pm10_atm_avg += d.pm10_atm as u32;
        }
        Self {
            pm1_0: (pm1_0_avg / data.len() as u32) as u16,
            pm2_5: (pm2_5_avg / data.len() as u32) as u16,
            pm10: (pm10_avg / data.len() as u32) as u16,
            pm1_0_atm: (pm1_0_atm_avg / data.len() as u32) as u16,
            pm2_5_atm: (pm2_5_atm_avg / data.len() as u32) as u16,
            pm10_atm: (pm10_atm_avg / data.len() as u32) as u16,
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
    let mut data = [PmSensorData::default(); 5];
    let mut offset = 0;
    loop {
        //info!("sending passive cmd to sensor");
        //dev.passive().await.unwrap();
        let frame = dev.read().await.unwrap();
        debug!(
            "PM1_0: {} PM2_5: {} PM10: {} PM1_0_atm: {} PM2_5_atm {}, PM10_atm {}",
            frame.pm1_0, frame.pm2_5, frame.pm10, frame.pm1_0_atm, frame.pm2_5_atm, frame.pm10_atm,
        );
        if offset < data.len() {
            data[offset] = PmSensorData::copy_from_frame(&frame);
            offset += 1;
        }
        if offset == data.len() {
            offset = 0;
            let avg = PmSensorData::average(&data);
            sender.send(DisplayInfo::Pms7003Data(avg)).await;
        }
        Timer::after(Duration::from_millis(250)).await;
    }
}
