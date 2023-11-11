use defmt::Format;

#[derive(Format, Clone, Copy)]
pub struct Parameters {
    pub screen_columns: u16,
    pub screen_rows: u16,
    pub screen_margin: u16,
    pub bme680_controller_loop_delay_ms: u32,
    pub screen_controller_loop_delay_ms: u32,
    pub screen_display_min_refresh_sec: u32,
}

impl Parameters {
    pub fn new(screen_columns: u16, screen_rows: u16) -> Parameters {
        Parameters {
            screen_columns,
            screen_rows,
            screen_margin: 5,
            bme680_controller_loop_delay_ms: 500,
            screen_controller_loop_delay_ms: 50,
            screen_display_min_refresh_sec: 180,
        }
    }
}
