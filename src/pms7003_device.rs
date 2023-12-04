//! Reading the Plantower PMS7003 sensor

use crate::{parameter::Parameters, DisplayInfo};
use defmt::{debug, error, info, Format};
use embassy_stm32::{
    gpio::{AnyPin, Output},
    peripherals, usart,
};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::channel::Sender;
use embassy_sync::signal::Signal;
use pms_7003::{async_interface::Pms7003SensorAsync, Error};

/// Control enum
#[derive(Debug, Clone, Copy, Format)]
pub enum PmCommand {
    On,
    Off,
}

pub static PM25_SIGNAL: Signal<CriticalSectionRawMutex, PmCommand> = Signal::new();

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

fn print_error(ctx: &str, e: Error) {
    match e {
        Error::SendFailed => error!("{}: Send failed", ctx),
        Error::ReadFailed => error!("{}: Read failed", ctx),
        Error::ChecksumError => error!("{}: checksum error", ctx),
        Error::IncorrectResponse => error!("{}: incorrect response", ctx),
        Error::NoResponse => error!("{}: no response", ctx),
    }
}

/// task to read pm2.5 sensor data
#[embassy_executor::task]
pub async fn pm25_controller(
    mut dev: Pms7003SensorAsync<usart::BufferedUart<'static, peripherals::USART1>>,
    _reset_pin: Output<'static, AnyPin>,
    _set_pin: Output<'static, AnyPin>,
    sender: Sender<'static, NoopRawMutex, DisplayInfo, 2>,
    _params: Parameters,
) {
    let mut first = true;
    info!("starting pm2.5 loop");
    loop {
        // wait for start signal
        match PM25_SIGNAL.wait().await {
            PmCommand::On => {
                info!("Start collecting pm2.5");
                if !first {
                    if let Err(e) = dev.wake().await {
                        print_error("pm25dev.wake", e);
                    }
                } else {
                    first = false;
                }
                let avg = pm25_get_data(&mut dev).await;
                sender.send(DisplayInfo::Pms7003Data(avg)).await;
            }
            PmCommand::Off => {
                info!("Stop collecting pm2.5");
                // put the device to sleep, due to race conditions, may
                // receive a data packet instead of the sleep response
                // if this happens, retry the sleep
                loop {
                    match dev.sleep().await {
                        Ok(_) => break,
                        Err(Error::IncorrectResponse) => {
                            debug!("sleep incorrect response");
                            continue;
                        }
                        Err(e) => {
                            print_error("pm25dev.sleep", e);
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn pm25_get_data(
    dev: &mut Pms7003SensorAsync<usart::BufferedUart<'static, peripherals::USART1>>,
) -> PmSensorData {
    debug!("pm2.5 get data loop");
    let mut data = [PmSensorData::default(); 5];
    let mut offset = 0;
    loop {
        match dev.read().await {
            Ok(frame) => {
                debug!(
                    "PM1_0: {} PM2_5: {} PM10: {} PM1_0_atm: {} PM2_5_atm {}, PM10_atm {}",
                    frame.pm1_0,
                    frame.pm2_5,
                    frame.pm10,
                    frame.pm1_0_atm,
                    frame.pm2_5_atm,
                    frame.pm10_atm,
                );
                if offset < data.len() {
                    data[offset] = PmSensorData::copy_from_frame(&frame);
                    offset += 1;
                }
                if offset == data.len() {
                    break;
                }
            }
            Err(e) => {
                print_error("pm25_get_data", e);
            }
        }
    }
    PmSensorData::average(&data)
}
