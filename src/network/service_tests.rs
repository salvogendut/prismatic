//! Exercise the frontend's saved settings through Iris's real packet backend.
use super::*;
use iris::net::{
    GatewayConfig, Link, Rings,
    packet::{Ether, Ipv4Route},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

struct Backend {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    rings: Rings,
    route: Ipv4Route,
    #[cfg(iris_live_network)]
    control: Arc<iris::net::NatControl>,
}

impl Backend {
    fn new(settings: &NetworkSettings) -> Self {
        let mut config = MachineConfig::default();
        settings.apply(&mut config).unwrap();
        let gateway = GatewayConfig::from_network(&config.network());
        let route = Ipv4Route::new(
            gateway.gateway_mac,
            [8, 0, 0x69, 0x12, 0x34, 0x56],
            gateway.client_ip,
            gateway.gateway_ip,
        );
        let mut link = Link::open(&config.network()).unwrap().unwrap();
        let running = link.running();
        #[cfg(iris_live_network)]
        let control = link.nat();
        let mut backend = link.backend().unwrap();
        running.store(true, Ordering::Release);
        let thread = Some(std::thread::spawn(move || backend.run()));
        Self {
            running,
            thread,
            rings: link.into_rings(),
            route,
            #[cfg(iris_live_network)]
            control,
        }
    }

    fn udp(&mut self, port: u16, payload: &[u8]) -> Vec<u8> {
        self.rings
            .to_net
            .push(self.route.udp(1, 40000, port, payload))
            .unwrap();
        self.rings.tx_wake.notify();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            while let Ok(frame) = self.rings.from_net.pop() {
                if let Some(ip) = Ether::parse(&frame).and_then(|eth| eth.ipv4())
                    && let Some(udp) = ip.udp()
                    && udp.dport() == 40000
                {
                    assert_eq!(ip.src(), self.route.dst);
                    assert_eq!(ip.dst(), self.route.src);
                    return udp.payload().to_vec();
                }
            }
            assert!(Instant::now() < deadline, "No reply from UDP port {port}");
            self.rings.rx_wake.wait(Duration::from_millis(10));
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.rings.tx_wake.notify();
        self.thread.take().unwrap().join().unwrap();
    }
}

fn rpc(xid: u32, program: u32, procedure: u32, args: &[u8]) -> Vec<u8> {
    let mut packet = Vec::new();
    for word in [xid, 0, 2, program, 3, procedure, 0, 0, 0, 0] {
        packet.extend(word.to_be_bytes());
    }
    packet.extend(args);
    packet
}
fn word(packet: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(packet[offset..offset + 4].try_into().unwrap())
}
fn opaque(bytes: &[u8]) -> Vec<u8> {
    let mut result = (bytes.len() as u32).to_be_bytes().to_vec();
    result.extend(bytes);
    while !result.len().is_multiple_of(4) {
        result.push(0);
    }
    result
}

#[test]
fn configured_nat_serves_tftp_and_nfs_from_selected_folders() {
    let nfs = tempfile::tempdir().unwrap();
    let tftp = tempfile::tempdir().unwrap();
    std::fs::write(nfs.path().join("shared.txt"), b"NFS test").unwrap();
    std::fs::write(tftp.path().join("bootfile"), b"TFTP boot test").unwrap();
    let settings = NetworkSettings {
        nat_subnet: "10.23.0.0/24".into(),
        nfs_dir: Some(nfs.path().to_str().unwrap().into()),
        tftp_dir: Some(tftp.path().to_str().unwrap().into()),
        ..Default::default()
    };
    let mut backend = Backend::new(&settings);
    assert_eq!(
        backend.udp(69, b"\x00\x01bootfile\x00octet\x00"),
        b"\x00\x03\x00\x01TFTP boot test"
    );
    // MOUNT v3 returns a filehandle for the selected share's root.
    let mount = backend.udp(1234, &rpc(42, 100005, 1, &opaque(b"/")));
    assert_eq!(word(&mount, 0), 42);
    assert_eq!(word(&mount, 20), 0); // RPC accepted.
    assert_eq!(word(&mount, 24), 0); // MOUNT success.
    let length = word(&mount, 28) as usize;
    let root = &mount[32..32 + length];
    // LOOKUP v3 must see a file from the NFS folder, not the TFTP root.
    let mut args = opaque(root);
    args.extend(opaque(b"shared.txt"));
    let lookup = backend.udp(2049, &rpc(43, 100003, 3, &args));
    assert_eq!(word(&lookup, 0), 43);
    assert_eq!(word(&lookup, 20), 0);
    assert_eq!(word(&lookup, 24), 0); // NFS success.
    // TFTP must not expose a file which belongs only to the NFS share.
    assert_eq!(
        &backend.udp(69, b"\x00\x01shared.txt\x00octet\x00")[..2],
        b"\x00\x05"
    );
}

#[test]
#[cfg(iris_live_network)]
fn live_nat_save_changes_forward_listeners_without_restarting_backend() {
    use std::net::{TcpListener, TcpStream};
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let old = NetworkSettings::default();
    let backend = Backend::new(&old);
    let mut edited = old.clone();
    edited.forwards.push(Forward {
        protocol: Protocol::Tcp,
        host_port: port,
        guest_port: 23,
    });
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("prismatic.cfg");
    let draft = crate::settings::Settings {
        network: edited.clone(),
        ..Default::default()
    };
    // No listener while merely editing, or when persistence fails.
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"preserve").unwrap();
    assert!(draft.save(&blocker.join("bad")).is_err());
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
    draft.save(&config).unwrap();
    edited.apply_live(&old, &backend.control);
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut rebound = edited.clone();
    rebound.forward_lan = true;
    rebound.forwards[0].guest_port = 22;
    let draft = crate::settings::Settings {
        network: rebound.clone(),
        ..Default::default()
    };
    draft.save(&config).unwrap();
    rebound.apply_live(&edited, &backend.control);
    let deadline = Instant::now() + Duration::from_secs(5);
    while backend.control.apply_forwards.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    // Let the network thread finish binding after consuming the request.
    std::thread::sleep(Duration::from_millis(50));
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_ok());
    old.apply_live(&rebound, &backend.control);
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_ok() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
