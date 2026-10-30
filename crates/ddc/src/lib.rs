//! A monitor's DDC/CI channel and its brightness. The channel is found from
//! the display connector (sysfs `ddc` link) when the GPU driver provides one,
//! else by scanning I2C buses for an EDID with the monitor's model name.

use ddc::{Ddc, Edid};
use ddc_i2c::I2cDeviceDdc;

/// VCP feature code for brightness in the DDC/CI standard.
const VCP_BRIGHTNESS: u8 = 0x10;

pub struct Monitor(I2cDeviceDdc);

/// The monitor name stored in an EDID (descriptor 0xFC).
pub fn edid_name(edid: &[u8]) -> Option<String> {
    edid.get(54..126)?.chunks(18).find_map(|d| {
        (d[..3] == [0, 0, 0] && d[3] == 0xFC).then(|| {
            let text: Vec<u8> = d[5..].iter().copied().take_while(|b| *b != 0x0A).collect();
            String::from_utf8_lossy(&text).trim().to_string()
        })
    })
}

impl Monitor {
    /// Every monitor that answers DDC/CI, with its EDID name.
    pub fn all() -> Vec<(String, Monitor)> {
        let Ok(found) = ddc_i2c::I2cDeviceEnumerator::new() else {
            return Vec::new();
        };
        found
            .filter_map(|mut ddc| {
                let mut edid = [0u8; 128];
                ddc.read_edid(0, &mut edid).ok()?;
                Some((edid_name(&edid).unwrap_or_default(), Monitor(ddc)))
            })
            .collect()
    }

    /// `connector` like "HDMI-A-1" (if known), `model` as in the EDID.
    pub fn find(connector: Option<&str>, model: &str) -> Option<Monitor> {
        if let Some(monitor) = connector.and_then(by_connector) {
            return Some(monitor);
        }
        ddc_i2c::I2cDeviceEnumerator::new().ok()?.find_map(|mut ddc| {
            let mut edid = [0u8; 128];
            ddc.read_edid(0, &mut edid).ok()?;
            (edid_name(&edid).as_deref() == Some(model)).then_some(Monitor(ddc))
        })
    }

    /// `(value, maximum)`
    pub fn brightness(&mut self) -> Option<(u16, u16)> {
        let value = self.0.get_vcp_feature(VCP_BRIGHTNESS).ok()?;
        Some((value.value(), value.maximum()))
    }

    pub fn set_brightness(&mut self, value: u16) -> bool {
        self.0.set_vcp_feature(VCP_BRIGHTNESS, value).is_ok()
    }
}

fn by_connector(connector: &str) -> Option<Monitor> {
    for card in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let name = card.file_name().to_string_lossy().into_owned();
        if name.split_once('-').map(|(_, c)| c) != Some(connector) {
            continue;
        }
        let bus = std::fs::read_link(card.path().join("ddc")).ok()?;
        let device = format!("/dev/{}", bus.file_name()?.to_string_lossy());
        return ddc_i2c::from_i2c_device(device).ok().map(Monitor);
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_monitor_name() {
        let mut edid = [0u8; 128];
        let descriptor = [0, 0, 0, 0xFC, 0, b'O', b'd', b'y', b's', b's', b'e', b'y', b' ', b'G', b'5', 0x0A, b' ', b' '];
        edid[72..90].copy_from_slice(&descriptor);
        assert_eq!(super::edid_name(&edid).as_deref(), Some("Odyssey G5"));
        assert_eq!(super::edid_name(&[0u8; 128]), None);
    }
}
