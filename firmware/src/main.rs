#![no_std]
#![no_main]

#[cfg(not(any(feature = "rp235xa", feature = "rp235xb")))]
compile_error!("select exactly one board feature: rp235xa or rp235xb");
#[cfg(all(feature = "rp235xa", feature = "rp235xb"))]
compile_error!("select only one RP2350 variant");

mod capture;

use defmt_rtt as _;
defmt::timestamp!("{=u64:us}", embassy_time::Instant::now().as_micros());

use cmx918_firmware::{Config as RadioConfig, Error, chip::Chip, protocol::*};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::{
    bind_interrupts, i2c,
    peripherals::{I2C0, USB},
    usb,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embassy_usb::{
    Builder, Config, Handler,
    driver::{Endpoint, EndpointIn, EndpointOut},
};
use static_cell::StaticCell;

bind_interrupts!(pub struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
    I2C0_IRQ => i2c::InterruptHandler<I2C0>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
});
type In = usb::Endpoint<'static, USB, usb::In>;
type Out = usb::Endpoint<'static, USB, usb::Out>;
type Bus = i2c::I2c<'static, I2C0, i2c::Async>;
static REQUESTS: Channel<ThreadModeRawMutex, [u8; REQUEST_LEN], 1> = Channel::new();
static REPLIES: Channel<ThreadModeRawMutex, [u8; REPLY_LEN], 1> = Channel::new();
static BLOCKS: Channel<ThreadModeRawMutex, (Block, u32), 8> = Channel::new();
static STREAMING: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU32 = AtomicU32::new(0);
static USB_FAULT: AtomicBool = AtomicBool::new(false);

struct UsbEvents;
impl Handler for UsbEvents {
    fn suspended(&mut self, suspended: bool) {
        defmt::debug!("USB suspended={}", suspended);
        if suspended {
            STREAMING.store(false, Ordering::Release);
        }
    }
    fn reset(&mut self) {
        defmt::debug!("USB reset");
        STREAMING.store(false, Ordering::Release);
    }
    fn configured(&mut self, configured: bool) {
        defmt::info!("USB configured={}", configured);
        if !configured {
            STREAMING.store(false, Ordering::Release);
        }
    }
    fn enabled(&mut self, enabled: bool) {
        defmt::debug!("USB enabled={}", enabled);
        if !enabled {
            STREAMING.store(false, Ordering::Release);
        }
    }
}

#[embassy_executor::task]
async fn usb_task(mut usb: embassy_usb::UsbDevice<'static, usb::Driver<'static, USB>>) {
    usb.run().await;
}

#[embassy_executor::task]
async fn control_task(mut input: Out, mut output: In) {
    loop {
        input.wait_enabled().await;
        let mut command = [0; REQUEST_LEN];
        let mut used = 0;
        loop {
            let mut packet = [0; 64];
            match with_timeout(Duration::from_secs(1), input.read(&mut packet)).await {
                Ok(Ok(length)) => {
                    for byte in &packet[..length] {
                        command[used] = *byte;
                        used += 1;
                        if used == REQUEST_LEN {
                            REQUESTS.send(command).await;
                            let reply = REPLIES.receive().await;
                            if !matches!(
                                with_timeout(Duration::from_secs(1), output.write(&reply)).await,
                                Ok(Ok(()))
                            ) {
                                STREAMING.store(false, Ordering::Release);
                            }
                            used = 0;
                        }
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => {
                    used = 0;
                } // discard incomplete command after bounded inactivity
            }
        }
    }
}

#[embassy_executor::task]
async fn stream_task(mut output: In) {
    loop {
        let (block, rate) = BLOCKS.receive().await;
        if !STREAMING.load(Ordering::Acquire)
            || block.generation != GENERATION.load(Ordering::Acquire)
        {
            continue;
        }
        // Finish an in-flight frame even if STOP arrives. Generation tags let
        // the host reject stale frames. A partial USB write is a stream fault.
        let transfer = async {
            output.write(&block.header(rate)).await?;
            for packet in block.data.chunks(64) {
                output.write(packet).await?;
            }
            Ok::<(), embassy_usb::driver::EndpointError>(())
        };
        if !matches!(
            with_timeout(Duration::from_millis(20), transfer).await,
            Ok(Ok(()))
        ) {
            USB_FAULT.store(true, Ordering::Release);
            STREAMING.store(false, Ordering::Release);
            defmt::warn!("USB data transfer failed or exceeded 20 ms; stopping stream");
        }
    }
}

struct Radio {
    config: Option<RadioConfig>,
    running: bool,
    generation: u32,
    sequence: u32,
    synthetic_index: u64,
    started: Instant,
    last_status: Instant,
    chip_status: u8,
    rssi: i16,
    chip_valid: bool,
    dropped_blocks: u32,
    fault_count: u32,
    last_error: u32,
}
impl Radio {
    fn new() -> Self {
        Self {
            config: None,
            running: false,
            generation: 0,
            sequence: 0,
            synthetic_index: 0,
            started: Instant::now(),
            last_status: Instant::now(),
            chip_status: 0,
            rssi: 0,
            chip_valid: false,
            dropped_blocks: 0,
            fault_count: 0,
            last_error: 0,
        }
    }

    fn reply(&self, raw: &[u8; REQUEST_LEN], error: Option<Error>) -> [u8; REPLY_LEN] {
        let mut reply = [0; REPLY_LEN];
        reply[..4].copy_from_slice(&MAGIC);
        reply[4] = VERSION;
        reply[5] = raw[5] | 0x40;
        reply[6] = error.map_or(0, |e| e as u8);
        reply[7] = u8::from(self.config.is_some())
            | (u8::from(self.running) << 1)
            | (u8::from(self.config.is_some_and(|c| c.synthetic())) << 2)
            | 8
            | (u8::from(self.chip_valid) << 4);
        reply[8..12].copy_from_slice(&raw[8..12]);
        put_u32(&mut reply, 12, self.generation);
        if let Some(c) = self.config {
            for (offset, value) in [
                (16, c.frequency),
                (20, c.rate),
                (24, c.bandwidth),
                (28, c.flags),
            ] {
                put_u32(&mut reply, offset, value);
            }
        }
        put_u32(&mut reply, 32, self.chip_status as u32);
        put_u32(&mut reply, 36, self.rssi as i32 as u32);
        put_u32(&mut reply, 40, self.dropped_blocks);
        put_u32(&mut reply, 44, self.last_error);
        put_u32(&mut reply, 48, self.fault_count);
        put_u32(&mut reply, 52, 0x1f); // candidate rate bitmap, in RATES order
        put_u32(&mut reply, 56, 0); // no hardware-qualified rates yet
        put_u32(
            &mut reply,
            60,
            if cfg!(feature = "rp235xb") { 0xb } else { 0xa },
        );
        reply
    }

    async fn stop(
        &mut self,
        chip: &mut Chip<Bus, embassy_time::Delay>,
        capture: &mut capture::Capture,
    ) -> Result<(), Error> {
        if self.running {
            defmt::info!(
                "stop generation={} blocks={}",
                self.generation,
                self.sequence
            );
        }
        STREAMING.store(false, Ordering::Release);
        self.running = false;
        while BLOCKS.try_receive().is_ok() {}
        capture.stop()?;
        if self.config.is_some_and(|c| !c.synthetic()) {
            with_timeout(Duration::from_millis(100), chip.mute(true))
                .await
                .map_err(|_| Error::Timeout)??;
        }
        Ok(())
    }

    async fn request(
        &mut self,
        request: Request,
        chip: &mut Chip<Bus, embassy_time::Delay>,
        capture: &mut capture::Capture,
    ) -> Result<(), Error> {
        match request.opcode {
            GET_STATUS => {}
            RESET => {
                STREAMING.store(false, Ordering::Release);
                self.running = false;
                while BLOCKS.try_receive().is_ok() {}
                capture.stop()?;
                self.config = None;
                self.chip_valid = false;
                with_timeout(Duration::from_millis(100), chip.reset())
                    .await
                    .map_err(|_| Error::Timeout)??;
                self.last_error = 0;
                defmt::info!("CMX918 software reset completed; configuration cleared");
            }
            PROBE => {
                if self.running {
                    return Err(Error::Protocol);
                }
                self.chip_valid = false;
                let (status, rssi) = with_timeout(Duration::from_millis(100), chip.status())
                    .await
                    .map_err(|_| Error::Timeout)??;
                self.chip_status = status;
                self.rssi = rssi;
                self.chip_valid = true;
                self.last_status = Instant::now();
                self.last_error = 0;
                defmt::info!(
                    "CMX918 I2C address 0x55 responds: STATUS={=u8:#x} RSSI(raw 1/16 dB)={}",
                    status,
                    rssi
                );
            }
            CONFIGURE => {
                let config = request.config.validate()?;
                defmt::info!(
                    "configure frequency={} Hz rate={} bandwidth={} flags={=u32:#x}",
                    config.frequency,
                    config.rate,
                    config.bandwidth,
                    config.flags
                );
                // A new explicit configuration can recover from absent/stuck chip
                // state. Stop the acquisition engine before any I2C changes.
                STREAMING.store(false, Ordering::Release);
                self.running = false;
                while BLOCKS.try_receive().is_ok() {}
                capture.stop()?;
                self.config = None;
                self.chip_valid = false;
                if !config.synthetic() {
                    with_timeout(Duration::from_secs(3), chip.configure(config))
                        .await
                        .map_err(|_| Error::Timeout)??;
                } else {
                    // Synthetic mode can run with CMX918 absent. Best-effort mute
                    // of a connected chip; it never gates the USB-only benchmark.
                    let _ = with_timeout(Duration::from_millis(100), chip.mute(true)).await;
                }
                self.config = Some(config);
                self.last_error = 0;
                defmt::debug!("configuration applied; awaiting START");
            }
            START => {
                let config = self.config.ok_or(Error::NotConfigured)?;
                self.stop(chip, capture).await?;
                self.generation = self.generation.wrapping_add(1).max(1);
                self.sequence = 0;
                self.synthetic_index = 0;
                if !config.synthetic() {
                    capture.start()?;
                    if let Err(error) = with_timeout(Duration::from_millis(100), chip.mute(false))
                        .await
                        .map_err(|_| Error::Timeout)
                        .and_then(|r| r)
                    {
                        let _ = capture.stop();
                        return Err(error);
                    }
                }
                self.started = Instant::now();
                self.running = true;
                self.last_error = 0;
                USB_FAULT.store(false, Ordering::Release);
                GENERATION.store(self.generation, Ordering::Release);
                STREAMING.store(true, Ordering::Release);
                defmt::info!(
                    "start generation={} synthetic={}",
                    self.generation,
                    config.synthetic()
                );
            }
            STOP => self.stop(chip, capture).await?,
            _ => return Err(Error::Protocol),
        }
        Ok(())
    }

    fn produce(&mut self, capture: &mut capture::Capture) -> Result<(), Error> {
        let config = self.config.ok_or(Error::NotConfigured)?;
        // Bounded work per tick; enough for >240 ksps while servicing control.
        for _ in 0..4 {
            let mut block = Block {
                generation: self.generation,
                sequence: self.sequence,
                first_sample: 0,
                flags: if config.synthetic() { 4 } else { 0 },
                data: [0; BLOCK_BYTES],
            };
            if config.synthetic() {
                let due = self.started.elapsed().as_micros() * u64::from(config.rate) / 1_000_000;
                if due - self.synthetic_index < SAMPLES as u64 {
                    break;
                }
                block.first_sample = self.synthetic_index;
                for (i, sample) in block.data.chunks_exact_mut(4).enumerate() {
                    let word = (self.synthetic_index + i as u64) as u16;
                    sample[..2].copy_from_slice(&word.to_le_bytes());
                    sample[2..].copy_from_slice(&(!word).to_le_bytes());
                }
                self.synthetic_index += SAMPLES as u64;
            } else {
                let Some(first) = capture.read_block(&mut block.data)? else {
                    break;
                };
                block.first_sample = first;
            }
            if BLOCKS.try_send((block, config.rate)).is_err() {
                self.dropped_blocks = self.dropped_blocks.saturating_add(1);
                return Err(Error::Usb);
            }
            self.sequence = self.sequence.wrapping_add(1);
        }
        Ok(())
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default()); // 12 MHz MCU crystal, 150 MHz system
    defmt::info!(
        "CMX918 SDR {} boot; RP2350 variant={} clock=150 MHz; no qualified rates",
        env!("CARGO_PKG_VERSION"),
        if cfg!(feature = "rp235xb") { "B" } else { "A" }
    );
    defmt::info!("I2C SDA=GP0 SCL=GP1 addr=0x55; SPI RX=GP8 CS=GP9 CLK=GP10");
    let mut info = [0u32; 9];
    // SAFETY: ROM writes at most nine u32 words into this aligned live buffer.
    // SYS_INFO_CHIP_INFO=1 returns the supported mask, package ID, and 64-bit
    // chip ID. No flash commands or persistent writes are performed.
    let result = unsafe { embassy_rp::rom_data::get_sys_info(info.as_mut_ptr(), info.len(), 1) };
    let mut uid = [0u8; 8];
    if result == 4 {
        uid[..4].copy_from_slice(&info[3].to_be_bytes());
        uid[4..].copy_from_slice(&info[2].to_be_bytes());
    }
    static SERIAL: StaticCell<[u8; 16]> = StaticCell::new();
    let serial = SERIAL.init([b'0'; 16]);
    for (i, byte) in uid.iter().enumerate() {
        serial[2 * i] = b"0123456789ABCDEF"[(byte >> 4) as usize];
        serial[2 * i + 1] = b"0123456789ABCDEF"[(byte & 15) as usize];
    }
    let driver = usb::Driver::new(p.USB, Irqs);
    // Private development identity, not an allocated product VID/PID.
    let mut config = Config::new(0xc0de, 0x0918);
    config.manufacturer = Some("CMX918 SDR project");
    config.product = Some("CMX918 SDR laboratory firmware");
    config.serial_number = Some(core::str::from_utf8(serial).unwrap());
    config.max_power = 100;
    static CONFIG: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL: StaticCell<[u8; 64]> = StaticCell::new();
    static EVENTS: StaticCell<UsbEvents> = StaticCell::new();
    let mut builder = Builder::new(
        driver,
        config,
        CONFIG.init([0; 256]),
        BOS.init([0; 256]),
        MSOS.init([0; 256]),
        CONTROL.init([0; 64]),
    );
    builder.handler(EVENTS.init(UsbEvents));
    let (command_out, reply_in, data_in) = {
        let mut function = builder.function(0xff, 0, 0);
        let mut interface = function.interface();
        let mut alternate = interface.alt_setting(0xff, 0, 0, None);
        (
            alternate.endpoint_bulk_out(None, 64),
            alternate.endpoint_bulk_in(None, 64),
            alternate.endpoint_bulk_in(None, 64),
        )
    };
    let usb = builder.build();
    spawner.spawn(usb_task(usb).unwrap());
    spawner.spawn(control_task(command_out, reply_in).unwrap());
    spawner.spawn(stream_task(data_in).unwrap());
    let mut i2c_config = i2c::Config::default();
    i2c_config.frequency = 400_000;
    let bus = i2c::I2c::new_async(p.I2C0, p.PIN_1, p.PIN_0, Irqs, i2c_config);
    let mut chip = Chip::new(bus, embassy_time::Delay);
    let mut capture = capture::Capture::new(p.PIO0, p.DMA_CH0, p.PIN_8, p.PIN_9, p.PIN_10, Irqs);
    let mut radio = Radio::new();
    // Read-only preflight, also available explicitly over USB as PROBE. This
    // gives RTT an I2C result even before host USB permissions are configured.
    let probe = Request {
        opcode: PROBE,
        id: 0,
        config: RadioConfig {
            frequency: 0,
            rate: 0,
            bandwidth: 0,
            flags: 0,
        },
    };
    if let Err(error) = radio.request(probe, &mut chip, &mut capture).await {
        radio.last_error = error as u32;
        defmt::warn!(
            "CMX918 startup I2C probe failed error={} (SDA GP0, SCL GP1)",
            error as u32
        );
    }
    loop {
        match select(REQUESTS.receive(), Timer::after_millis(1)).await {
            Either::First(raw) => {
                let result = match Request::parse(&raw) {
                    Ok(request) => radio.request(request, &mut chip, &mut capture).await,
                    Err(error) => Err(error),
                };
                if let Err(error) = result {
                    radio.last_error = error as u32;
                    defmt::warn!(
                        "command opcode={} id={} failed error={}",
                        raw[5],
                        get_u32(&raw, 8),
                        error as u32
                    );
                }
                REPLIES.send(radio.reply(&raw, result.err())).await;
            }
            Either::Second(()) => {}
        }
        if radio.running {
            let result = if !STREAMING.load(Ordering::Acquire) || USB_FAULT.load(Ordering::Acquire)
            {
                Err(Error::Usb)
            } else {
                radio.produce(&mut capture)
            };
            if let Err(error) = result {
                defmt::error!(
                    "stream fault={} generation={} blocks={}",
                    error as u32,
                    radio.generation,
                    radio.sequence
                );
                radio.fault_count = radio.fault_count.saturating_add(1);
                let _ = radio.stop(&mut chip, &mut capture).await;
                radio.last_error = error as u32;
            }
        }
        if radio.last_status.elapsed().as_millis() >= 1000
            && radio.config.is_some_and(|c| !c.synthetic())
        {
            match with_timeout(Duration::from_millis(3), chip.status()).await {
                Ok(Ok((status, rssi))) => {
                    defmt::debug!("CMX918 STATUS={=u8:#x} RSSI(raw 1/16 dB)={}", status, rssi);
                    radio.chip_status = status;
                    radio.rssi = rssi;
                    radio.chip_valid = true;
                }
                _ => {
                    if radio.chip_valid {
                        defmt::warn!("CMX918 status read failed");
                    }
                    radio.chip_valid = false;
                }
            }
            radio.last_status = Instant::now();
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    STREAMING.store(false, Ordering::Release);
    defmt::error!("panic: {}", defmt::Display2Format(info));
    cortex_m::interrupt::disable();
    loop {
        cortex_m::asm::wfi();
    }
}
