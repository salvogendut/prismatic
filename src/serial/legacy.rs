//! Iris without transport injection owns its TCP listeners.
use iris::config::MachineConfig;
pub struct Endpoints(Vec<u16>);
impl Endpoints {
    pub fn prepare(
        config: &mut MachineConfig,
        settings: &crate::expert::ExpertSettings,
    ) -> Result<Self, String> {
        let mut ports = vec![settings.serial_port_a, settings.serial_port_b];
        let hardware = crate::settings::MachineSettings {
            profile: config.machine.profile,
            ..Default::default()
        };
        ports.extend_from_slice(&settings.platform_ports[..hardware.platform_serial_lines()]);
        let mut probes = Vec::new();
        for (index, port) in ports.iter_mut().enumerate() {
            let probe = std::net::TcpListener::bind(("127.0.0.1", *port)).map_err(|e| {
                format!(
                    "Serial {}: cannot bind TCP port {port}: {e}",
                    ["A", "B", "L1", "C", "D"][index]
                )
            })?;
            *port = probe.local_addr().map_err(|e| e.to_string())?.port();
            probes.push(probe);
        }
        config.serial_port_a = Some(ports[0]);
        config.serial_port_b = Some(ports[1]);
        #[cfg(iris_modern_models)]
        if ports.len() > 2 {
            use iris::config::MachineProfile::*;
            match config.machine.profile {
                FuelIp34 => config.fuel.l1_port = Some(ports[2]),
                TezroIp53 => {
                    config.tezro.l1_port = Some(ports[2]);
                    config.tezro.serial_port_c = Some(ports[3]);
                    config.tezro.serial_port_d = Some(ports[4]);
                }
                Origin3200Ip35 => config.origin3200.l1_port = Some(ports[2]),
                Onyx3000Ip35 => config.onyx3000.l1_port = Some(ports[2]),
                _ => {}
            }
        }
        Ok(Self(ports))
    }
    pub fn describe(&self, index: usize) -> String {
        let channel = ["A", "B", "L1", "C", "D"][index];
        let port = self.0[index];
        if port == 0 {
            format!("Serial {channel}: TCP automatic (port reported on Iris console)")
        } else {
            format!("Serial {channel}: TCP 127.0.0.1:{port} (Iris owned)")
        }
    }
    pub fn platform_report(&self) -> String {
        (2..self.0.len())
            .map(|i| format!("\n{}", self.describe(i)))
            .collect()
    }
}
