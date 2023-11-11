#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]

use atmo_monitor_stm32 as _; // global logger + panicking-behavior + memory layout
use atmo_monitor_stm32::{
    bme680_device::{Bme680Data, BmeDevice},
    parameter::Parameters,
    screen::{DisplayInfo, Screen},
};
use defmt::{debug, info, unwrap};
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts, dma::NoDma, gpio::*, i2c, peripherals, rcc::AdcClockSource, spi, time::Hertz,
    usart,
};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Duration, Instant, Timer};
use il0373::{Builder, Dimensions, Display, GraphicDisplay, Interface, Rotation};
use static_cell::StaticCell;

use defmt_rtt as _; // global logger

/// Display controller channel
static CHANNEL: StaticCell<Channel<NoopRawMutex, DisplayInfo, 2>> = StaticCell::new();

// constants related to display size
const COLS: u16 = 104;
const ROWS: u16 = 212;
const DISPLAY_BUFSIZE: usize = (ROWS * COLS / 8) as usize;

// display buffer
static mut BLACK_BUFFER: [u8; DISPLAY_BUFSIZE] = [0; DISPLAY_BUFSIZE];
static mut RED_BUFFER: [u8; DISPLAY_BUFSIZE] = [0; DISPLAY_BUFSIZE];

// connect the I2C1 interrupt
bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::InterruptHandler<peripherals::I2C1>;
    USART1 => usart::InterruptHandler<peripherals::USART1>;
});

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

    // dc - PC7, rst - PA9, busy - PA8,
    // sck - PA5, mosi - PA7, miso - PA6
    // epd_cs - PB6, sram_cs - PB10, sdmmc_cs - PB5,
    let display_cs = Output::new(p.PB6, Level::High, Speed::Low);
    //let sram_cs = Output::new(p.PB10, Level::High, Speed::Low);
    //let sdmmc_cs = Output::new(p.PB5, Level::High, Speed::Low);
    let display_dc = Output::new(p.PC7, Level::High, Speed::Low);
    let display_rst = Output::new(p.PA9, Level::High, Speed::Low);
    let display_busy = Input::new(p.PA8, Pull::None);

    // info!("Initializing particulate sensor...");
    // let usart_config = usart::Config::default();
    // let mut usart = usart::Uart::new(
    //     p.USART1,
    //     p.PC5,
    //     p.PC4,
    //     Irqs,
    //     p.DMA1_CH4,
    //     NoDma,
    //     usart_config,
    // );
    // for n in 0u32.. {
    //     let mut s: String<128> = String::new();
    //     core::write!(&mut s, "Atmo Monitor World {}!\r\n", n).unwrap();

    //     unwrap!(usart.write(s.as_bytes()).await);
    //     info!("wrote DMA");
    // }

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
    let dspctrl_channel = CHANNEL.init(Channel::new());

    info!("Starting tasks...");

    unwrap!(spawner.spawn(bme680_controller(
        bme_dev,
        dspctrl_channel.sender(),
        parameters,
    )));
    unwrap!(spawner.spawn(display_controller(
        screen,
        dspctrl_channel.receiver(),
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
    let mut have_data = false;
    bme_dev.init();
    loop {
        let data = bme_dev.read();
        sender.send(DisplayInfo::Bme680Data(data)).await;
        if !have_data {
            sender.send(DisplayInfo::Show).await;
            have_data = true;
        }
        Timer::after(Duration::from_millis(
            params.bme680_controller_loop_delay_ms.into(),
        ))
        .await;
    }
}

/// task to control display
#[embassy_executor::task]
async fn display_controller(
    mut screen: Screen,
    receiver: Receiver<'static, NoopRawMutex, DisplayInfo, 2>,
    params: Parameters,
) {
    let mut current_data = Bme680Data::default();
    let mut screen_on = false;
    // the amount of time between screen updates
    let screen_update_duration = Duration::from_secs(params.screen_display_min_refresh_sec.into());
    let mut last_update = Instant::now();
    loop {
        if let Ok(recv) = receiver.try_receive() {
            debug!("display_controller got {}", recv);
            // match on channel data, return gives whether display is on, and
            // whether displayed data should be updated
            let _data_update = match recv {
                DisplayInfo::Bme680Data(data) => {
                    current_data = data;
                    true
                }
                DisplayInfo::Show => {
                    // TODO: make sure it doesn't violate the interval
                    screen.turn_on(&current_data);
                    last_update = Instant::now();
                    screen_on = true;
                    false
                }

                DisplayInfo::Hide => {
                    screen.shutoff();
                    screen_on = false;
                    false
                }
            };
        }
        if screen_on {
            if last_update + screen_update_duration <= Instant::now() {
                screen.update(&current_data);
                last_update = Instant::now();
            }
        }
        Timer::after(Duration::from_millis(
            params.screen_controller_loop_delay_ms.into(),
        ))
        .await;
    }
}
