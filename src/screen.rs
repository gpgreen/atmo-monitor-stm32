use crate::{bme680_device::Bme680Data, pms7003_device::PmSensorData};
use core::fmt::Write;
use defmt::debug;
use embassy_stm32::{gpio::*, peripherals, spi::Spi};
use embassy_time::Delay;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;
use il0373::{Color, GraphicDisplay, Interface};
use micromath::F32Ext;
use profont::{PROFONT_10_POINT, PROFONT_12_POINT, PROFONT_24_POINT};

/// type of the SramDisplayInterface for this app
type STMInterface<'a> = Interface<
    Spi<'a, peripherals::SPI1, peripherals::DMA1_CH3, peripherals::DMA1_CH2>,
    Output<'a, peripherals::PB6>,
    Input<'a, peripherals::PB5>,
    Output<'a, peripherals::PC7>,
    Output<'a, peripherals::PB4>,
>;

/// type of the SramGraphicDisplay for this app
type STMDisplay<'a> = GraphicDisplay<'a, STMInterface<'a>>;

/// Structure to represent an eInk display attached to the SPI bus
pub struct Screen {
    pub display_width: u16,
    pub display_height: u16,
    pub margin: u16,
    hdwr: STMDisplay<'static>,
}

impl Screen {
    /// Create a new display
    pub fn new(
        display: STMDisplay<'static>,
        display_width: u16,
        display_height: u16,
        margin: u16,
    ) -> Screen {
        Screen {
            hdwr: display,
            display_width,
            display_height,
            margin,
        }
    }

    /// Turn off the display
    pub fn power_off(&mut self) {
        debug!("Power off display");
        //self.hdwr.clear(Color::White).ok();
    }

    /// Turn on display
    pub fn power_on(&mut self) {
        debug!("Power on display");
    }

    /// Update data on the display
    pub fn update(&mut self, sensor_data: &Bme680Data, sensor_pmdata: &PmSensorData) {
        debug!("display update");

        let mut delay = Delay;
        self.hdwr.reset(&mut delay).ok();

        // clear the display
        self.hdwr.clear(Color::White).ok();

        // Choose text style 10point at 6x12 pixels
        let char_blk_style = MonoTextStyle::new(&PROFONT_10_POINT, Color::Black);
        let char_rd_style = MonoTextStyle::new(&PROFONT_10_POINT, Color::Red);
        let med_char_rd_style = MonoTextStyle::new(&PROFONT_12_POINT, Color::Red);
        let lg_char_blk_style = MonoTextStyle::new(&PROFONT_24_POINT, Color::Black);

        let x_start: i32 = self.margin.into();
        let y_start: i32 = self.margin as i32 + 10;

        Text::new(
            "Atmo Monitor v0.1.0",
            Point::new(x_start + 30, y_start),
            med_char_rd_style,
        )
        .draw(&mut self.hdwr)
        .unwrap();

        let mut buf: String<32> = String::new();
        write!(&mut buf, "Humidity: {}\u{25}", sensor_data.humidity.trunc()).unwrap();
        Text::new(
            buf.as_str(),
            Point::new(x_start, y_start + 14),
            char_blk_style,
        )
        .draw(&mut self.hdwr)
        .unwrap();
        buf.clear();
        write!(&mut buf, "Pressure: {}hPa", sensor_data.pressure.trunc()).unwrap();
        Text::new(
            buf.as_str(),
            Point::new(x_start, y_start + 14 + 14),
            char_blk_style,
        )
        .draw(&mut self.hdwr)
        .unwrap();
        buf.clear();
        let style = if sensor_data.gas_valid && sensor_data.heat_stable {
            write!(&mut buf, "Gas: {}ohms", sensor_data.gas_resistance).unwrap();
            char_blk_style
        } else {
            write!(&mut buf, "Gas invalid").unwrap();
            char_rd_style
        };
        Text::new(
            buf.as_str(),
            Point::new(x_start, y_start + 14 + 14 + 14),
            style,
        )
        .draw(&mut self.hdwr)
        .unwrap();
        buf.clear();
        let mut x = self.display_height - self.margin - 10;
        let y = self.display_width - self.margin - 10;
        let char_width = if sensor_pmdata.pm2_5_atm < 10 {
            1
        } else if sensor_pmdata.pm2_5_atm < 100 {
            2
        } else if sensor_pmdata.pm2_5_atm < 1000 {
            3
        } else {
            4
        };
        x -= 18 * char_width;
        write!(&mut buf, "{}", sensor_pmdata.pm2_5_atm).unwrap();
        Text::new(
            buf.as_str(),
            Point::new(x.into(), y.into()),
            lg_char_blk_style,
        )
        .draw(&mut self.hdwr)
        .unwrap();
        buf.clear();
        write!(&mut buf, "PM2.5").unwrap();
        Text::new(
            buf.as_str(),
            Point::new(x.into(), (y - 28).into()),
            char_blk_style,
        )
        .draw(&mut self.hdwr)
        .unwrap();
        buf.clear();
        write!(&mut buf, "{}\u{B0}C", sensor_data.temperature.trunc()).unwrap();
        Text::new(
            buf.as_str(),
            Point::new(x_start + 10, y.into()),
            lg_char_blk_style,
        )
        .draw(&mut self.hdwr)
        .unwrap();
        self.hdwr.update().ok();
        self.hdwr.deep_sleep().ok();
    }
}
