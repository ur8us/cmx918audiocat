//! Exercise the actual descriptor builder and class request handler on the host.
use embassy_usb::{
    Builder, Config, Handler,
    class::cdc_acm::{CdcAcmClass, State},
    control::{InResponse, OutResponse, Recipient, Request, RequestType},
    driver::*,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
static USB_EPOCH: AtomicU32 = AtomicU32::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
fn audio_active(active: bool) {
    ACTIVE.store(active, Ordering::Relaxed);
}
#[path = "../src/usb_audio.rs"]
mod usb_audio;

struct MockDriver {
    next_in: usize,
    next_out: usize,
}
struct Ep(EndpointInfo);
impl Endpoint for Ep {
    fn info(&self) -> &EndpointInfo {
        &self.0
    }
    async fn wait_enabled(&mut self) {}
}
impl EndpointIn for Ep {
    async fn write(&mut self, _: &[u8]) -> Result<(), EndpointError> {
        unreachable!()
    }
}
impl EndpointOut for Ep {
    async fn read(&mut self, _: &mut [u8]) -> Result<usize, EndpointError> {
        unreachable!()
    }
}
struct MockBus;
impl Bus for MockBus {
    async fn enable(&mut self) {}
    async fn disable(&mut self) {}
    async fn poll(&mut self) -> Event {
        unreachable!()
    }
    fn endpoint_set_enabled(&mut self, _: EndpointAddress, _: bool) {}
    fn endpoint_set_stalled(&mut self, _: EndpointAddress, _: bool) {}
    fn endpoint_is_stalled(&mut self, _: EndpointAddress) -> bool {
        false
    }
    async fn remote_wakeup(&mut self) -> Result<(), Unsupported> {
        Err(Unsupported)
    }
}
struct Pipe;
impl ControlPipe for Pipe {
    fn max_packet_size(&self) -> usize {
        64
    }
    async fn setup(&mut self) -> [u8; 8] {
        unreachable!()
    }
    async fn data_out(&mut self, _: &mut [u8], _: bool, _: bool) -> Result<usize, EndpointError> {
        unreachable!()
    }
    async fn data_in(&mut self, _: &[u8], _: bool, _: bool) -> Result<(), EndpointError> {
        unreachable!()
    }
    async fn accept(&mut self) {}
    async fn reject(&mut self) {}
    async fn accept_set_address(&mut self, _: u8) {}
}
impl<'a> Driver<'a> for MockDriver {
    type EndpointOut = Ep;
    type EndpointIn = Ep;
    type Bus = MockBus;
    type ControlPipe = Pipe;
    fn alloc_endpoint_in(
        &mut self,
        ep_type: EndpointType,
        addr: Option<EndpointAddress>,
        max_packet_size: u16,
        interval_ms: u8,
    ) -> Result<Ep, EndpointAllocError> {
        self.next_in += 1;
        Ok(Ep(EndpointInfo {
            addr: addr.unwrap_or(EndpointAddress::from_parts(self.next_in, Direction::In)),
            ep_type,
            max_packet_size,
            interval_ms,
        }))
    }
    fn alloc_endpoint_out(
        &mut self,
        ep_type: EndpointType,
        addr: Option<EndpointAddress>,
        max_packet_size: u16,
        interval_ms: u8,
    ) -> Result<Ep, EndpointAllocError> {
        self.next_out += 1;
        Ok(Ep(EndpointInfo {
            addr: addr.unwrap_or(EndpointAddress::from_parts(self.next_out, Direction::Out)),
            ep_type,
            max_packet_size,
            interval_ms,
        }))
    }
    fn start(self, _: u16) -> (MockBus, Pipe) {
        (MockBus, Pipe)
    }
}
#[test]
fn composite_topology_rate_controls_and_lifecycle() {
    let mut config_buf = [0; 512];
    let mut bos = [0; 256];
    let mut msos = [0; 256];
    let mut control_buf = [0; 64];
    let mut audio = usb_audio::Control::new();
    let mut cdc = State::new();
    let mut config = Config::new(0xc0de, 0x0919);
    config.device_class = 0xef;
    config.device_sub_class = 2;
    config.device_protocol = 1;
    config.composite_with_iads = true;
    let mut builder = Builder::new(
        MockDriver {
            next_in: 0,
            next_out: 0,
        },
        config,
        &mut config_buf,
        &mut bos,
        &mut msos,
        &mut control_buf,
    );
    let ep = usb_audio::microphone(&mut builder, &mut audio);
    let _cdc = CdcAcmClass::new(&mut builder, &mut cdc, 64);
    let device = builder.build();
    drop(device);
    assert_eq!(ep.info().max_packet_size, 26);
    let total = u16::from_le_bytes([config_buf[2], config_buf[3]]) as usize;
    assert_eq!(config_buf[4], 4); // AC, AS, CDC control, CDC data
    let mut descriptors = Vec::new();
    let mut offset = 0;
    while offset < total {
        let n = config_buf[offset] as usize;
        assert!(n >= 2 && offset + n <= total);
        descriptors.push(&config_buf[offset..offset + n]);
        offset += n;
    }
    assert_eq!(descriptors.iter().filter(|d| d[1] == 11).count(), 2); // IAD functions
    let interfaces: Vec<_> = descriptors
        .iter()
        .filter(|d| d[1] == 4)
        .map(|d| (d[2], d[3], d[4], d[5], d[6]))
        .collect();
    assert_eq!(
        interfaces,
        [
            (0, 0, 0, 1, 1),
            (1, 0, 0, 1, 2),
            (1, 1, 1, 1, 2),
            (2, 0, 1, 2, 2),
            (3, 0, 2, 10, 0)
        ]
    );
    assert!(descriptors.contains(&&[9, 0x24, 1, 0, 1, 30, 0, 1, 1][..]));
    assert!(descriptors.contains(&&[12, 0x24, 2, 1, 0x10, 7, 0, 1, 0, 0, 0, 0][..]));
    assert!(descriptors.contains(&&[9, 0x24, 3, 2, 1, 1, 0, 1, 0][..]));
    assert!(descriptors.contains(&&[11, 0x24, 2, 1, 1, 2, 16, 1, 0xe0, 0x2e, 0][..]));
    assert!(descriptors.contains(&&[9, 5, 0x81, 5, 26, 0, 1, 0, 0][..]));
    let request = Request {
        direction: Direction::Out,
        request_type: RequestType::Class,
        recipient: Recipient::Endpoint,
        request: 1,
        value: 0x0100,
        index: 0x81,
        length: 3,
    };
    assert_eq!(
        audio.control_out(request, &[0xe0, 0x2e, 0]),
        Some(OutResponse::Accepted)
    );
    for bytes in [&[][..], &[0xe0, 0x2e], &[0x80, 0xbb, 0]] {
        assert_eq!(
            audio.control_out(request, bytes),
            Some(OutResponse::Rejected)
        );
    }
    assert_eq!(
        audio.control_out(
            Request {
                index: 0x82,
                ..request
            },
            &[0xe0, 0x2e, 0]
        ),
        None
    );
    assert_eq!(
        audio.control_out(
            Request {
                recipient: Recipient::Interface,
                index: 2,
                ..request
            },
            &[0; 3]
        ),
        None
    ); // don't swallow CDC
    let mut buf = [0; 3];
    for code in 0x81..=0x84 {
        match audio.control_in(
            Request {
                direction: Direction::In,
                request: code,
                ..request
            },
            &mut buf,
        ) {
            Some(InResponse::Accepted(bytes)) => assert_eq!(
                bytes,
                if code == 0x84 {
                    &[0, 0, 0]
                } else {
                    &[0xe0, 0x2e, 0]
                }
            ),
            _ => panic!("rate request rejected"),
        }
    }
    audio.set_alternate_setting(audio.interface_number(), 1);
    assert!(ACTIVE.load(Ordering::Relaxed));
    audio.suspended(true);
    assert!(!ACTIVE.load(Ordering::Relaxed));
    audio.suspended(false);
    assert!(ACTIVE.load(Ordering::Relaxed));
    audio.set_alternate_setting(audio.interface_number(), 0);
    assert!(!ACTIVE.load(Ordering::Relaxed));
    audio.reset();
    assert_eq!(USB_EPOCH.load(Ordering::Relaxed), 1);
}
