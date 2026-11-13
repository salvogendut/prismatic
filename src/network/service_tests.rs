//! Exercise the frontend's saved settings through Iris's real packet backend.
use super::*;
use iris::net::{
    GatewayConfig, Link, Rings,
    packet::{Ether, Ipv4Route},
};
#[cfg(any(iris_live_network, not(iris_net_running)))]
use std::sync::{Arc, atomic::Ordering};
#[cfg(not(iris_net_running))]
use std::thread::JoinHandle;
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

struct Backend {
    #[cfg(iris_net_running)]
    _running: iris::net::Running,
    #[cfg(not(iris_net_running))]
    running: Arc<AtomicBool>,
    #[cfg(not(iris_net_running))]
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
        #[cfg(not(iris_net_running))]
        let running = link.running();
        #[cfg(iris_live_network)]
        let control = link.nat();
        #[cfg(iris_net_running)]
        let _running = link.spawn("prismatic-network-test").unwrap();
        #[cfg(not(iris_net_running))]
        let thread = {
            let mut backend = link.backend().unwrap();
            running.store(true, Ordering::Release);
            Some(std::thread::spawn(move || backend.run()))
        };
        Self {
            #[cfg(iris_net_running)]
            _running,
            #[cfg(not(iris_net_running))]
            running,
            #[cfg(not(iris_net_running))]
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

#[cfg(not(iris_net_running))]
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

#[test]
#[cfg(iris_stream_network)]
fn private_stream_transports_frames_to_an_external_peer() {
    use iris::net::stream::{read_frame, write_frame};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let settings = NetworkSettings {
        mode: NetMode::Stream,
        stream: Some(format!("tcp:{}", listener.local_addr().unwrap())),
        ..Default::default()
    };
    let mut backend = Backend::new(&settings);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut peer = loop {
        match listener.accept() {
            Ok((peer, _)) => break peer,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("{e}"),
        }
    };
    peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let outgoing = backend
        .route
        .udp(1, 68, 67, b"private frame without Ethernet padding");
    backend.rings.to_net.push(outgoing.clone()).unwrap();
    backend.rings.tx_wake.notify();
    assert_eq!(read_frame(&mut peer).unwrap().unwrap(), outgoing);
    let incoming = backend
        .route
        .udp(2, 67, 68, b"peer reply without Ethernet padding");
    write_frame(&mut peer, &incoming).unwrap();
    loop {
        if let Ok(frame) = backend.rings.from_net.pop() {
            assert_eq!(frame, incoming);
            break;
        }
        assert!(Instant::now() < deadline);
        backend.rings.rx_wake.wait(Duration::from_millis(10));
    }
}

#[cfg(iris_stream_network)]
fn install_server_exchange(
    backend: &mut Backend,
    sport: u16,
    dport: u16,
    payload: &[u8],
) -> Vec<u8> {
    use iris::net::packet::{ARP_REPLY, ARP_REQUEST, arp};
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut send_at = Instant::now();
    loop {
        if Instant::now() >= send_at {
            backend
                .rings
                .to_net
                .push(backend.route.udp(1, sport, dport, payload))
                .unwrap();
            backend.rings.tx_wake.notify();
            send_at = Instant::now() + Duration::from_millis(250);
        }
        while let Ok(frame) = backend.rings.from_net.pop() {
            let eth = Ether::parse(&frame).unwrap();
            if let Some(request) = eth.arp()
                && request.op() == ARP_REQUEST
                && request.target_ip() == backend.route.src
            {
                backend
                    .rings
                    .to_net
                    .push(arp(
                        ARP_REPLY,
                        backend.route.src_mac,
                        backend.route.src,
                        request.sender_mac(),
                        request.sender_ip(),
                    ))
                    .unwrap();
                backend.rings.tx_wake.notify();
            }
            if let Some(udp) = eth.ipv4().and_then(|ip| ip.udp())
                && udp.dport() == sport
            {
                return udp.payload().to_vec();
            }
        }
        assert!(
            Instant::now() < deadline,
            "Install server did not answer port {dport}"
        );
        backend.rings.rx_wake.wait(Duration::from_millis(10));
    }
}

#[cfg(iris_stream_network)]
fn real_install_helper(backend: crate::installation::Backend, tcp: bool) {
    use crate::installation::{Server, Settings};
    let fixture = tempfile::tempdir().unwrap();
    let setup = fixture.path().join("setup with spaces");
    std::fs::create_dir_all(setup.join("media/dist")).unwrap();
    std::fs::create_dir_all(setup.join("media/stand")).unwrap();
    std::fs::write(setup.join("media/dist/sa"), b"synthetic sa").unwrap();
    std::fs::write(
        setup.join("media/stand/bootfile"),
        b"Prismatic network boot fixture",
    )
    .unwrap();
    std::fs::write(
        setup.join("instigator.yaml"),
        r#"server_ip: 192.168.0.1
netmask: 192.168.0.0/24
clients:
  - {name: test, mac: "08:00:69:12:34:56", ip: 192.168.0.2}
install_sets:
  - name: test
    layers:
      - {name: base, source: media, boot: true}
services:
  bootp: true
  tftp: {enabled: true}
  rsh: true
"#,
    )
    .unwrap();
    let mut settings = Settings {
        backend,
        folder: Some(setup.to_str().unwrap().into()),
        program: std::env::var("PRISMATIC_INSTALL_HELPER").unwrap_or_else(|_| "instigator".into()),
        ..Default::default()
    };
    settings.validate_files().unwrap();
    let mut server = Server::start_with_transport(&settings, &AtomicBool::new(false), tcp).unwrap();
    let socket = server.endpoint().strip_prefix("unix:").map(str::to_owned);
    let net = NetworkSettings {
        mode: NetMode::Stream,
        stream: Some(server.endpoint().into()),
        ..Default::default()
    };
    let mut peer = Backend::new(&net);
    peer.route.dst_mac = [8, 0, 0x69, 0, 0, 2]; // Instigator's private NIC.
    let mut bootp = vec![0; 300];
    bootp[..3].copy_from_slice(&[1, 1, 6]);
    bootp[4..8].copy_from_slice(&1234u32.to_be_bytes());
    bootp[12..16].copy_from_slice(&peer.route.src.octets());
    bootp[28..34].copy_from_slice(&peer.route.src_mac);
    let name = b"test/stand/bootfile";
    bootp[108..108 + name.len()].copy_from_slice(name);
    let reply = install_server_exchange(&mut peer, 68, 67, &bootp);
    assert_eq!(&reply[16..20], &[192, 168, 0, 2]);
    assert_eq!(&reply[20..24], &[192, 168, 0, 1]);
    let tftp = install_server_exchange(
        &mut peer,
        40000,
        69,
        b"\x00\x01test/stand/bootfile\x00octet\x00",
    );
    assert_eq!(&tftp[..4], b"\x00\x03\x00\x01");
    assert_eq!(&tftp[4..], b"Prismatic network boot fixture");
    assert!(server.report().contains("Running"));
    drop(peer);
    drop(server);
    if let Some(socket) = socket {
        assert!(!std::path::Path::new(&socket).exists());
    }
}

#[test]
#[cfg(iris_stream_network)]
#[ignore = "requires Instigator: set PRISMATIC_INSTALL_HELPER to its absolute path"]
fn native_instigator_answers_bootp_and_tftp_over_iris_stream() {
    real_install_helper(crate::installation::Backend::Native, cfg!(windows));
}

#[test]
#[cfg(all(iris_stream_network, target_os = "linux"))]
#[ignore = "requires rootless Podman and make install-server-image"]
fn podman_instigator_answers_bootp_and_tftp_over_iris_stream() {
    real_install_helper(crate::installation::Backend::Podman, false);
}

#[test]
#[cfg(iris_stream_network)]
#[ignore = "requires Instigator: set PRISMATIC_INSTALL_HELPER to its absolute path"]
fn native_instigator_answers_bootp_and_tftp_over_tcp_stream() {
    real_install_helper(crate::installation::Backend::Native, true);
}
