#![no_main]
#![no_std]

use atmo_monitor_stm32 as _; // global logger + panicking-behavior + memory layout
use atmo_monitor_stm32::{
    DisplayInfo,
    bme680_device::BmeDevice,
    parameter::Parameters,
    pms7003_device::{self, PM25_SIGNAL, PmCommand},
    screen::Screen,
};
use embassy_executor::Spawner;
use embassy_futures::{select, select::Either};
use embassy_stm32::{bind_interrupts, gpio::*, i2c, mode, peripherals, spi, time::Hertz, usart};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use il0373::{Builder, Dimensions, Display, GraphicDisplay, Interface, Rotation};
use log::{debug, error, info};
use pms_7003::async_interface::Pms7003SensorAsync;
use static_cell::StaticCell;

/// Display controller channel
static DISPLAY_CHANNEL: StaticCell<Channel<NoopRawMutex, DisplayInfo, 2>> = StaticCell::new();

// constants related to display size
const COLS: u16 = 104;
const ROWS: u16 = 212;
const DISPLAY_BUFSIZE: usize = (ROWS * COLS / 8) as usize;

// display buffer
static BLACK_BUFFER: StaticCell<[u8; DISPLAY_BUFSIZE]> = StaticCell::new();
static RED_BUFFER: StaticCell<[u8; DISPLAY_BUFSIZE]> = StaticCell::new();

// uart buffers
static TX_BUFFER: StaticCell<[u8; 32]> = StaticCell::new();
static RX_BUFFER: StaticCell<[u8; 64]> = StaticCell::new();

// connect the interrupts
bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
    USART1 => usart::BufferedInterruptHandler<peripherals::USART1>;
});

/// Control enum
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
    {
        // change clock to use HSE bypass clock signal and PLL
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: Hertz(8_000_000),
            // Oscillator for bluepill, bypass for nucleo's.
            mode: HseMode::Bypass,
        });
        config.rcc.pll = Some(Pll {
            src: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL9, // 72 MHz
        });
        config.rcc.sys = Sysclk::PLL1_P;
        config.rcc.ahb_pre = AHBPrescaler::DIV1; // 72 MHz
        config.rcc.apb1_pre = APBPrescaler::DIV2; // 36 MHz
        config.rcc.apb2_pre = APBPrescaler::DIV2; // 36 MHz
        //config.rcc.adc = ADCPrescaler::DIV2; // 18 MHz
    }
    let p = embassy_stm32::init(config);

    // create the parameters
    let parameters = Parameters::new(COLS, ROWS);
    info!("parameters: {:?}", parameters);

    let power_lbo = Input::new(p.PA0, Pull::None);

    // dc - PA3, rst - PD1, busy - PC14, ena - PD2
    // sck - PA5, mosi - PA7, miso - PA6
    // epd_cs - PA4, sd_cs - PA1, sram_cs - PA2
    let display_cs = Output::new(p.PA4, Level::High, Speed::Low);
    let display_dc = Output::new(p.PA3, Level::High, Speed::Low);
    let display_rst = Output::new(p.PD1, Level::High, Speed::Low);
    let display_busy = Input::new(p.PC14, Pull::None);
    let display_ena = Output::new(p.PD0, Level::High, Speed::Low);

    // usart1 rx = PA9, tx = PA10
    info!("Initializing particulate sensor...");
    let mut usart_config = usart::Config::default();
    usart_config.baudrate = 9600;
    let tx_buf = TX_BUFFER.init([0u8; 32]);
    let rx_buf = RX_BUFFER.init([0u8; 64]);
    let usart =
        match usart::BufferedUart::new(p.USART1, p.PA10, p.PA9, tx_buf, rx_buf, Irqs, usart_config)
        {
            Ok(u) => u,
            Err(_) => panic!(),
        };

    // set - PB12, reset - PB13
    let pm25dev = Pms7003SensorAsync::new(usart);
    let pm_set = Output::new(p.PB12, Level::High, Speed::Low);
    let pm_reset = Output::new(p.PB13, Level::High, Speed::Low);

    info!("Initializing bme680 sensor...");
    // initialize i2c
    // scl - PB8, sda - PB9
    let mut i2c_config = i2c::Config::default();
    i2c_config.frequency = Hertz(100_000);
    let i2c = i2c::I2c::new(
        p.I2C1, p.PB8, p.PB9, Irqs, p.DMA1_CH6, p.DMA1_CH7, i2c_config,
    );
    let bme_dev = match BmeDevice::new(i2c) {
        Ok(dev) => dev,
        Err(e) => {
            error!("bme680 init error: {:?}", e);
            panic!()
        }
    };

    // spi
    let mut spi_config = spi::Config::default();
    spi_config.frequency = Hertz(8_000_000);
    let spi = spi::Spi::new(
        p.SPI1, p.PA5, p.PA7, p.PA6, p.DMA1_CH3, p.DMA1_CH2, spi_config,
    );

    // Initialize Display
    info!("Initializing Display...");
    let display_config = match Builder::new()
        .dimensions(Dimensions {
            rows: parameters.screen_rows,
            cols: parameters.screen_columns as u8,
        })
        .rotation(Rotation::Rotate90)
        .build()
    {
        Ok(config) => config,
        Err(_) => panic!(),
    };
    let blk_buffer = BLACK_BUFFER.init([0_u8; DISPLAY_BUFSIZE]);
    let red_buffer = RED_BUFFER.init([0_u8; DISPLAY_BUFSIZE]);
    let screen = Screen::new(
        GraphicDisplay::new(
            Display::new(
                Interface::new(spi, (display_cs, display_busy, display_dc, display_rst)),
                display_config,
            ),
            blk_buffer,
            red_buffer,
        ),
        parameters.screen_columns,
        parameters.screen_rows,
        5,
    );

    Timer::after(Duration::from_millis(800)).await;

    // data channels
    let dspctrl_channel = DISPLAY_CHANNEL.init(Channel::new());

    info!("Starting tasks...");

    spawner
        .spawn(bme680_controller(
            bme_dev,
            dspctrl_channel.sender(),
            parameters,
        ))
        .ok();
    spawner
        .spawn(display_controller(
            screen,
            display_ena,
            power_lbo,
            dspctrl_channel.receiver(),
            parameters,
        ))
        .ok();
    spawner
        .spawn(pms7003_device::pm25_controller(
            pm25dev,
            pm_reset,
            pm_set,
            dspctrl_channel.sender(),
            parameters,
        ))
        .ok();
}

/// task to read sensor data
#[embassy_executor::task]
async fn bme680_controller(
    mut bme_dev: BmeDevice<i2c::I2c<'static, mode::Async, i2c::mode::Master>>,
    sender: Sender<'static, NoopRawMutex, DisplayInfo, 2>,
    params: Parameters,
) {
    bme_dev.init().ok();
    // throw away the first reading
    bme_dev.read().ok();
    Timer::after(Duration::from_millis(
        params.bme680_first_data_delay_ms.into(),
    ))
    .await;
    loop {
        match BME_SIGNAL.wait().await {
            BmeCommand::On => {
                if let Ok(data) = bme_dev.read() {
                    sender.send(DisplayInfo::Bme680Data(data)).await;
                }
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
    mut ena_pin: Output<'static>,
    lbo_pin: Input<'static>,
    receiver: Receiver<'static, NoopRawMutex, DisplayInfo, 2>,
    params: Parameters,
) {
    loop {
        ena_pin.set_high();
        PM25_SIGNAL.signal(PmCommand::Wake);
        BME_SIGNAL.signal(BmeCommand::On);
        let mut current_data = None;
        let mut current_pmdata = None;
        loop {
            debug!("Start sensor data cycle");
            match select::select(
                receiver.receive(),
                Timer::after(Duration::from_secs(
                    params.screen_controller_timeout_sec.into(),
                )),
            )
            .await
            {
                Either::First(recv) => {
                    debug!("display_controller got {:?}", recv);
                    match recv {
                        DisplayInfo::Bme680Data(data) => {
                            current_data = Some(data);
                            BME_SIGNAL.signal(BmeCommand::Off);
                        }
                        DisplayInfo::Pms7003Data(data) => {
                            current_pmdata = Some(data);
                            PM25_SIGNAL.signal(PmCommand::Sleep);
                        }
                    }
                }
                Either::Second(_) => {
                    error!("Timeout waiting for sensors");
                }
            }
            if let (Some(d), Some(pd)) = (current_data, current_pmdata) {
                screen.power_on();
                screen.update(&d, &pd, lbo_pin.is_low());
                screen.power_off();
                break;
            }
        }
        debug!("Exit sensor data cycle");
        Timer::after(Duration::from_secs(
            params.screen_enable_shutdown_delay_sec.into(),
        ))
        .await;
        ena_pin.set_low();
        debug!("sleep cycle");
        Timer::after(Duration::from_secs(
            (params.screen_display_min_refresh_sec - params.screen_enable_shutdown_delay_sec)
                .into(),
        ))
        .await;
    }
}
