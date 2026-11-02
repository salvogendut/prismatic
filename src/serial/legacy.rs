//! Iris without transport injection owns its TCP listeners.
use iris::config::MachineConfig;
pub struct Endpoints([u16; 2]);
impl Endpoints {
    pub fn prepare(
        config: &mut MachineConfig,
        settings: &crate::expert::ExpertSettings,
    ) -> Result<Self, String> {
        let ports = [settings.serial_port_a, settings.serial_port_b];
        let mut probes = Vec::new();
        for (index, port) in ports.into_iter().enumerate().filter(|(_, p)| *p != 0) {
            probes.push(
                std::net::TcpListener::bind(("127.0.0.1", port)).map_err(|e| {
                    format!(
                        "Serial {}: cannot bind TCP port {port}: {e}",
                        if index == 0 { "A" } else { "B" }
                    )
                })?,
            );
        }
        config.serial_port_a = Some(ports[0]);
        config.serial_port_b = Some(ports[1]);
        Ok(Self(ports))
    }
    pub fn describe(&self, index: usize) -> String {
        let channel = if index == 0 { "A" } else { "B" };
        let port = self.0[index];
        if port == 0 {
            format!("Serial {channel}: TCP automatic (port reported on Iris console)")
        } else {
            format!("Serial {channel}: TCP 127.0.0.1:{port} (Iris owned)")
        }
    }
}
