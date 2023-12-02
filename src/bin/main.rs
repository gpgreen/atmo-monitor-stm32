#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]

use atmo_monitor_stm32 as _; // global logger + panicking-behavior + memory layout
use atmo_monitor_stm32::{
    bme680_device::BmeDevice,
    parameter::Parameters,
    pms7003_device::{self, PmCommand, PM25_SIGNAL},
    screen::Screen,
    DisplayInfo,
};
use defmt::{debug, error, info, unwrap, Format};
use embassy_executor::Spawner;
use embassy_futures::{select, select::Either};
use embassy_stm32::{
    bind_interrupts, dma::NoDma, gpio::*, i2c, peripherals, rcc::AdcClockSource, spi, time::Hertz,
    usart,
};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use il0373::{Builder, Dimensions, Display, GraphicDisplay, Interface, Rotation};
use pms_7003::async_interface::Pms7003SensorAsync;
use static_cell::{make_static, StaticCell};

/// Display controller channel
static DISPLAY_CHANNEL: StaticCell<Channel<NoopRawMutex, DisplayInfo, 2>> = StaticCell::new();

// constants related to display size
const COLS: u16 = 104;
const ROWS: u16 = 212;
const DISPLAY_BUFSIZE: usize = (ROWS * COLS / 8) as usize;

// display buffer
static mut BLACK_BUFFER: [u8; DISPLAY_BUFSIZE] = [0; DISPLAY_BUFSIZE];
static mut RED_BUFFER: [u8; DISPLAY_BUFSIZE] = [0; DISPLAY_BUFSIZE];

// connect the interrupts
bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::InterruptHandler<peripherals::I2C1>;
    USART1 => usart::BufferedInterruptHandler<peripherals::USART1>;
});

/// Control enum
#[derive(Debug, Clone, Copy, Format)]
pub enum BmeCommand {
    On,
    Off,
}

pub static BME_SIGNAL: Signal<CriticalSectionRawMutex, BmeCommand> = Signal::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("atmo-monitor!");

    //defmt::trace!("trace");
    //defmt::debug!("debug");
    //defmt::info!("info");
    //defmt::warn!("warn");
    //defmt::error!("error");

    let mut config = embassy_stm32::Config::default();
    config.rcc.sysclk = Some(Hertz(72_000_000));
    config.rcc.hclk = Some(Hertz(72_000_000));
    config.rcc.pclk1 = Some(Hertz(32_000_000));
    config.rcc.pclk2 = Some(Hertz(64_000_000));
    config.rcc.adc = Some(AdcClockSource::PllDiv1);
    let p = embassy_stm32::init(config);

    // create the parameters
    let parameters = Parameters::new(COLS, ROWS);
    info!("parameters: {}", parameters);

    // dc - PC7, rst - PB4, busy - PB5, ena - PB3
    // sck - PA5, mosi - PA7, miso - PA6
    // epd_cs - PB6
    let display_cs = Output::new(p.PB6, Level::High, Speed::Low);
    let display_dc = Output::new(p.PC7, Level::High, Speed::Low);
    let display_rst = Output::new(p.PB4, Level::High, Speed::Low);
    let display_busy = Input::new(p.PB5, Pull::None);
    let display_ena = Output::new(p.PB3, Level::High, Speed::Low);

    // usart1 rx = PA9, tx = PA10
    info!("Initializing particulate sensor...");
    let mut usart_config = usart::Config::default();
    usart_config.baudrate = 9600;
    let tx_buf = &mut make_static!([0u8; 32])[..];
    let rx_buf = &mut make_static!([0u8; 64])[..];
    let usart =
        usart::BufferedUart::new(p.USART1, Irqs, p.PA10, p.PA9, tx_buf, rx_buf, usart_config);
    let pm25dev = Pms7003SensorAsync::new(usart);
    let pm_set = Output::new(p.PA2, Level::High, Speed::Low);
    let pm_reset = Output::new(p.PA3, Level::High, Speed::Low);

    info!("Initializing bme680 sensor...");
    // initialize i2c
    let i2c = i2c::I2c::new(
        p.I2C1,
        p.PB8,
        p.PB9,
        Irqs,
        NoDma,
        NoDma,
        Hertz(100_000),
        i2c::Config::default(),
    );
    let bme_dev = BmeDevice::new(i2c);

    // spi
    let mut spi_config = spi::Config::default();
    spi_config.frequency = Hertz(8_000_000);
    let spi = spi::Spi::new(
        p.SPI1, p.PA5, p.PA7, p.PA6, p.DMA1_CH3, p.DMA1_CH2, spi_config,
    );

    // Initialize Display
    info!("Initializing Display...");
    let display_config = Builder::new()
        .dimensions(Dimensions {
            rows: parameters.screen_rows,
            cols: parameters.screen_columns as u8,
        })
        .rotation(Rotation::Rotate90)
        .build()
        .unwrap();
    let screen = Screen::new(
        GraphicDisplay::new(
            Display::new(
                Interface::new(spi, (display_cs, display_busy, display_dc, display_rst)),
                display_config,
            ),
            unsafe { &mut BLACK_BUFFER },
            unsafe { &mut RED_BUFFER },
        ),
        parameters.screen_columns,
        parameters.screen_rows,
        5,
    );

    Timer::after(Duration::from_millis(800)).await;

    // data channels
    let dspctrl_channel = DISPLAY_CHANNEL.init(Channel::new());

    info!("Starting tasks...");

    unwrap!(spawner.spawn(bme680_controller(
        bme_dev,
        dspctrl_channel.sender(),
        parameters,
    )));
    unwrap!(spawner.spawn(display_controller(
        screen,
        display_ena.degrade(),
        dspctrl_channel.receiver(),
        parameters,
    )));
    unwrap!(spawner.spawn(pms7003_device::pm25_controller(
        pm25dev,
        pm_reset.degrade(),
        pm_set.degrade(),
        dspctrl_channel.sender(),
        parameters,
    )));
}

/// task to read sensor data
#[embassy_executor::task]
async fn bme680_controller(
    mut bme_dev: BmeDevice<i2c::I2c<'static, peripherals::I2C1>>,
    sender: Sender<'static, NoopRawMutex, DisplayInfo, 2>,
    params: Parameters,
) {
    bme_dev.init();
    // throw away the first reading
    bme_dev.read();
    Timer::after(Duration::from_millis(
        params.bme680_first_data_delay_ms.into(),
    ))
    .await;
    loop {
        match BME_SIGNAL.wait().await {
            BmeCommand::On => {
                let data = bme_dev.read();
                sender.send(DisplayInfo::Bme680Data(data)).await;
            }
            BmeCommand::Off => {}
        }
    }
}

/// task to control display
///
/// signal both sensors to collect data
/// when both have responded, then signal the sensors to suspend
/// display the data
/// wait for display interval and repeat
#[embassy_executor::task]
async fn display_controller(
    mut screen: Screen,
    mut ena_pin: Output<'static, AnyPin>,
    receiver: Receiver<'static, NoopRawMutex, DisplayInfo, 2>,
    params: Parameters,
) {
    let mut current_data = None;
    let mut current_pmdata = None;
    loop {
        ena_pin.set_high();
        PM25_SIGNAL.signal(PmCommand::On);
        BME_SIGNAL.signal(BmeCommand::On);
        loop {
            match select::select(
                receiver.receive(),
                Timer::after(Duration::from_secs(
                    params.screen_controller_timeout_sec.into(),
                )),
            )
            .await
            {
                Either::First(recv) => {
                    debug!("display_controller got {}", recv);
                    match recv {
                        DisplayInfo::Bme680Data(data) => {
                            current_data = Some(data);
                            BME_SIGNAL.signal(BmeCommand::Off);
                        }
                        DisplayInfo::Pms7003Data(data) => {
                            current_pmdata = Some(data);
                            PM25_SIGNAL.signal(PmCommand::Off);
                        }
                    }
                }
                Either::Second(_) => {
                    error!("Timeout waiting for sensors");
                    break;
                }
            }
            if let (Some(d), Some(pd)) = (current_data, current_pmdata) {
                screen.power_on();
                screen.update(&d, &pd);
                screen.power_off();
                break;
            }
        }
        Timer::after(Duration::from_secs(20)).await;
        ena_pin.set_low();
        current_data = None;
        current_pmdata = None;
        Timer::after(Duration::from_secs(
            (params.screen_display_min_refresh_sec - 20).into(),
        ))
        .await;
    }
}
