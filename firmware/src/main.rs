#![no_std]
#![no_main]
#[cfg(not(any(feature = "rp235xa", feature = "rp235xb")))]
compile_error!("select exactly one board feature: rp235xa or rp235xb");
#[cfg(all(feature = "rp235xa", feature = "rp235xb"))]
compile_error!("select only one RP2350 variant");
mod capture;
mod usb_audio;
use defmt_rtt as _;
defmt::timestamp!("{=u64:us}", embassy_time::Instant::now().as_micros());
use cmx918_firmware::{
    Error,
    audio::{AudioQueue, MAX_PACKET_BYTES},
    cat::{Command, Parser, Reply, Tuning},
    chip::Chip,
    dsp::Demodulator,
};
use core::{
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::{
    bind_interrupts, i2c,
    peripherals::{I2C0, USB},
    usb,
};
use embassy_sync::{
    blocking_mutex::{Mutex, raw::CriticalSectionRawMutex},
    channel::Channel,
    signal::Signal,
};
use embassy_time::{Duration, Timer, with_timeout};
use embassy_usb::{
    Builder, Config,
    class::cdc_acm::{CdcAcmClass, State},
    driver::{Endpoint, EndpointIn},
};
use static_cell::StaticCell;

bind_interrupts!(pub struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
    I2C0_IRQ => i2c::InterruptHandler<I2C0>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
});
type Bus = i2c::I2c<'static, I2C0, i2c::Async>;
type UsbDriver = usb::Driver<'static, USB>;
type In = usb::Endpoint<'static, USB, usb::In>;
static REQUESTS: Channel<CriticalSectionRawMutex, Command, 1> = Channel::new();
static REPLIES: Channel<CriticalSectionRawMutex, Reply, 1> = Channel::new();
static AUDIO: Mutex<CriticalSectionRawMutex, RefCell<AudioQueue>> =
    Mutex::new(RefCell::new(AudioQueue::new()));
static AUDIO_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static USB_STALLS: AtomicU32 = AtomicU32::new(0);
static USB_EPOCH: AtomicU32 = AtomicU32::new(0);

fn clear_audio() {
    AUDIO.lock(|q| q.borrow_mut().clear());
    AUDIO_CHANGED.signal(());
}
fn audio_active(active: bool) {
    ACTIVE.store(active, Ordering::Release);
    clear_audio();
}

#[embassy_executor::task]
async fn usb_task(mut device: embassy_usb::UsbDevice<'static, UsbDriver>) {
    device.run().await;
}

#[embassy_executor::task]
async fn cat_task(mut cdc: CdcAcmClass<'static, UsbDriver>) {
    loop {
        cdc.wait_connection().await;
        let mut parser = Parser::default();
        let mut epoch = USB_EPOCH.load(Ordering::Acquire);
        loop {
            let mut packet = [0; 64];
            let length =
                match with_timeout(Duration::from_secs(1), cdc.read_packet(&mut packet)).await {
                    Ok(Ok(n)) => n,
                    Ok(Err(_)) => break,
                    Err(_) => {
                        parser.expire();
                        continue;
                    }
                };
            let current_epoch = USB_EPOCH.load(Ordering::Acquire);
            if epoch != current_epoch {
                // A read waiting across reset may return the first packet of
                // the new connection. Discard old parser state, not that packet.
                epoch = current_epoch;
                parser = Parser::default();
            }
            let mut failed = false;
            for byte in &packet[..length] {
                let Some(command) = parser.push(*byte) else {
                    continue;
                };
                let reply = match command {
                    Ok(command) => {
                        REQUESTS.send(command).await;
                        REPLIES.receive().await // chip work is bounded, audio is a separate task
                    }
                    Err(_) => Reply::literal(b"?;"),
                };
                if epoch != USB_EPOCH.load(Ordering::Acquire) {
                    failed = true;
                    break;
                }
                if reply.len != 0
                    && !matches!(
                        with_timeout(
                            Duration::from_millis(100),
                            cdc.write_packet(&reply.bytes[..reply.len])
                        )
                        .await,
                        Ok(Ok(()))
                    )
                {
                    failed = true;
                    break;
                }
            }
            if failed {
                break;
            }
        }
        Timer::after_millis(1).await;
    }
}

#[embassy_executor::task]
async fn audio_task(mut output: In) {
    loop {
        output.wait_enabled().await;
        if !ACTIVE.load(Ordering::Acquire) {
            Timer::after_millis(1).await;
            continue;
        }
        let mut packet = [0; MAX_PACKET_BYTES];
        let len = if READY.load(Ordering::Acquire) {
            AUDIO.lock(|q| q.borrow_mut().packet(&mut packet))
        } else {
            24
        };
        // The HAL queues one DPRAM packet, then waits for the host's next IN
        // token before the following write. No independent MCU 1ms pacing clock.
        match select(
            AUDIO_CHANGED.wait(),
            with_timeout(Duration::from_millis(4), output.write(&packet[..len])),
        )
        .await
        {
            Either::First(()) => {} // Cancel a pending packet from the old configuration.
            Either::Second(Ok(Ok(()))) => {}
            Either::Second(_) => {
                let _ = USB_STALLS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                    Some(v.saturating_add(1))
                });
                clear_audio();
                Timer::after_millis(1).await;
            }
        }
    }
}

struct Radio {
    tuning: Tuning,
    dsp: Demodulator,
    running: bool,
    error: u32,
    faults: u32,
    settling_samples: usize,
}
impl Radio {
    fn new() -> Self {
        Self {
            tuning: Tuning::default(),
            dsp: Demodulator::new(Tuning::default()),
            running: false,
            error: 0,
            faults: 0,
            settling_samples: 0,
        }
    }
    async fn apply(
        &mut self,
        tuning: Tuning,
        chip: &mut Chip<Bus, embassy_time::Delay>,
        capture: &mut capture::Capture,
    ) -> Result<(), Error> {
        READY.store(false, Ordering::Release);
        self.running = false;
        clear_audio();
        capture.stop()?;
        with_timeout(Duration::from_secs(3), chip.configure(tuning.config()))
            .await
            .map_err(|_| Error::Timeout)??;
        self.dsp = Demodulator::new(tuning);
        // Discard the first 100 ms while chip/DSP filters settle. Hardware
        // testing with a strong carrier found clipping in the first 16 ms
        // after retuning; priming the FIFO alone preserves those transients.
        self.settling_samples = 2_400;
        capture.start()?;
        with_timeout(Duration::from_millis(100), chip.mute(false))
            .await
            .map_err(|_| Error::Timeout)??;
        self.tuning = tuning;
        self.running = true;
        self.error = 0;
        READY.store(true, Ordering::Release);
        defmt::info!(
            "RX dial={} coarse={} mode={} input=24000 output=12000",
            tuning.frequency,
            tuning.coarse_frequency(),
            tuning.mode.digit()
        );
        Ok(())
    }
    async fn fault(
        &mut self,
        error: Error,
        chip: &mut Chip<Bus, embassy_time::Delay>,
        capture: &mut capture::Capture,
    ) {
        READY.store(false, Ordering::Release);
        self.running = false;
        self.error = error as u32;
        self.faults = self.faults.saturating_add(1);
        clear_audio();
        let _ = capture.stop();
        let _ = with_timeout(Duration::from_millis(100), chip.mute(true)).await;
        defmt::warn!(
            "receiver stopped: error={} faults={}; CAT FA/MD/ZZRX can retry",
            self.error,
            self.faults
        );
    }
    async fn command(
        &mut self,
        command: Command,
        chip: &mut Chip<Bus, embassy_time::Delay>,
        capture: &mut capture::Capture,
    ) -> Reply {
        let tuning = match command {
            Command::Frequency(Some(frequency)) => Some(Tuning {
                frequency,
                ..self.tuning
            }),
            Command::Mode(Some(mode)) => Some(Tuning {
                mode,
                ..self.tuning
            }),
            Command::Retry => Some(self.tuning),
            _ => None,
        };
        if let Some(tuning) = tuning {
            if let Err(error) = self.apply(tuning, chip, capture).await {
                self.fault(error, chip, capture).await;
                return Reply::literal(b"?;");
            }
            return Reply::literal(b""); // Kenwood setters do not echo
        }
        match command {
            Command::Frequency(None) => Reply::frequency(self.tuning),
            Command::Mode(None) => Reply::mode(self.tuning),
            Command::If => Reply::information(self.tuning),
            Command::Id => Reply::literal(b"ID020;"), // TS-480 subset compatibility identity
            Command::Ai => Reply::literal(b"AI0;"),
            Command::Fr => Reply::literal(b"FR0;"),
            Command::Ack => Reply::literal(b""),
            Command::Status => {
                let (under, over) = AUDIO.lock(|q| {
                    let q = q.borrow();
                    (q.underruns, q.overruns)
                });
                Reply::status(
                    self.running,
                    self.error,
                    self.faults,
                    under,
                    over,
                    USB_STALLS.load(Ordering::Relaxed),
                )
            }
            _ => Reply::literal(b"?;"),
        }
    }
    fn produce(&mut self, capture: &mut capture::Capture) -> Result<(), Error> {
        for _ in 0..2 {
            let mut iq = [0; capture::BLOCK_BYTES];
            if capture.read_block(&mut iq)?.is_none() {
                break;
            }
            let mut pcm = [0; capture::SAMPLES / 2];
            let mut used = 0;
            for sample in iq.chunks_exact(4) {
                let i = i16::from_le_bytes([sample[0], sample[1]]);
                let q = i16::from_le_bytes([sample[2], sample[3]]);
                let audio = self.dsp.process(i, q);
                if self.settling_samples != 0 {
                    self.settling_samples -= 1;
                } else if let Some(value) = audio {
                    pcm[used] = value;
                    used += 1;
                }
            }
            if ACTIVE.load(Ordering::Acquire) {
                AUDIO.lock(|q| q.borrow_mut().push(&pcm[..used]));
            }
        }
        Ok(())
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default()); // inherited: 12 MHz crystal, 150 MHz system
    defmt::info!(
        "CMX918 USB Audio + CAT {}; SDA=0 SCL=1 RX=8 CS=9 CLK=10",
        env!("CARGO_PKG_VERSION")
    );
    let mut info = [0u32; 9];
    // SAFETY: ROM writes at most nine aligned u32 words into this live buffer.
    // Only SYS_INFO_CHIP_INFO is requested; no flash/OTP mutation is performed.
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
    // Private, unallocated development identity; distinct PID from upstream bulk firmware.
    let mut config = Config::new(0xc0de, 0x0919);
    config.manufacturer = Some("CMX918 project");
    config.product = Some("CMX918 Audio CAT Receiver");
    config.serial_number = Some(core::str::from_utf8(serial).unwrap());
    config.max_power = 100;
    // IAD composite class (required for the CDC function on Windows).
    config.device_class = 0xef;
    config.device_sub_class = 2;
    config.device_protocol = 1;
    config.composite_with_iads = true;
    static CONFIG: StaticCell<[u8; 512]> = StaticCell::new();
    static BOS: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL: StaticCell<[u8; 64]> = StaticCell::new();
    static AUDIO_CONTROL: StaticCell<usb_audio::Control> = StaticCell::new();
    static CDC: StaticCell<State<'static>> = StaticCell::new();
    let mut builder = Builder::new(
        usb::Driver::new(p.USB, Irqs),
        config,
        CONFIG.init([0; 512]),
        BOS.init([0; 256]),
        MSOS.init([0; 256]),
        CONTROL.init([0; 64]),
    );
    let audio = usb_audio::microphone(&mut builder, AUDIO_CONTROL.init(usb_audio::Control::new()));
    let cdc = CdcAcmClass::new(&mut builder, CDC.init(State::new()), 64);
    spawner.spawn(usb_task(builder.build()).unwrap());
    spawner.spawn(cat_task(cdc).unwrap());
    spawner.spawn(audio_task(audio).unwrap());
    let mut config = i2c::Config::default();
    config.frequency = 400_000;
    let bus = i2c::I2c::new_async(p.I2C0, p.PIN_1, p.PIN_0, Irqs, config);
    let mut chip = Chip::new(bus, embassy_time::Delay);
    let mut capture = capture::Capture::new(p.PIO0, p.DMA_CH0, p.PIN_8, p.PIN_9, p.PIN_10, Irqs);
    let mut radio = Radio::new();
    if let Err(error) = radio.apply(radio.tuning, &mut chip, &mut capture).await {
        radio.fault(error, &mut chip, &mut capture).await;
    }
    loop {
        if let Either::First(command) = select(REQUESTS.receive(), Timer::after_micros(250)).await {
            let reply = radio.command(command, &mut chip, &mut capture).await;
            // One outstanding command, and its CDC reader is already waiting.
            let _ = REPLIES.try_send(reply);
        }
        if radio.running
            && let Err(error) = radio.produce(&mut capture)
        {
            radio.fault(error, &mut chip, &mut capture).await;
        }
    }
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    READY.store(false, Ordering::Release);
    defmt::error!("panic: {}", defmt::Display2Format(info));
    cortex_m::interrupt::disable();
    loop {
        cortex_m::asm::wfi();
    }
}
