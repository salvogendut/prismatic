use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    #[default]
    Tcp,
    Pty,
    Off,
}

impl Route {
    pub fn cycle(&mut self) {
        *self = match self {
            Self::Tcp if cfg!(unix) => Self::Pty,
            Self::Tcp | Self::Pty => Self::Off,
            Self::Off => Self::Tcp,
        };
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Pty => "Pseudoterminal",
            Self::Off => "Off",
        }
    }

    pub fn tcp_port(self, port: u16) -> Option<u16> {
        (self == Self::Tcp).then_some(port)
    }

    pub fn validate(self) -> Result<(), String> {
        if self == Self::Pty && !cfg!(unix) {
            return Err("Serial pseudoterminals require a Unix host; choose TCP or Off".into());
        }
        Ok(())
    }
}

#[cfg(iris_serial)]
mod native;
#[cfg(iris_serial)]
pub use native::Endpoints;
#[cfg(any(not(iris_serial), not(iris_platform_serial)))]
mod legacy;
#[cfg(not(iris_serial))]
pub use legacy::Endpoints;
