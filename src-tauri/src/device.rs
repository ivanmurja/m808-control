//! Transporte HID: abre as duas coleções vendor do M808 e troca pacotes com o mouse.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hidapi::{HidApi, HidDevice};

use crate::protocol::{self as p, DeviceEvent};

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("mouse M808 não encontrado")]
    NotFound,
    #[error("o mouse não respondeu (está dormindo ou desligado? mexa o mouse e tente de novo)")]
    Timeout,
    #[error("o mouse recusou o comando 0x{cmd:02x} (status 0x{status:02x})")]
    Rejected { cmd: u8, status: u8 },
    #[error("falha na autenticação do handshake")]
    Handshake,
    #[error("erro HID: {0}")]
    Hid(#[from] hidapi::HidError),
}

pub type Result<T> = std::result::Result<T, DeviceError>;

pub struct Device {
    cmd: HidDevice,
    notify: HidDevice,
    pub pid: u16,
    /// Eventos 0x0A recebidos enquanto se esperava a resposta de um comando.
    pending_events: Vec<DeviceEvent>,
}

const RESPONSE_TIMEOUT: Duration = Duration::from_millis(500);
/// O dongle responde sozinho a 0x01..0x04, mas leitura/escrita da EEPROM vai até o mouse,
/// que pode estar em repouso: o primeiro pacote o acorda e é perdido.
const RETRIES: usize = 8;

impl Device {
    pub fn open(api: &mut HidApi) -> Result<Device> {
        api.refresh_devices()?;
        for &pid in &p::PIDS {
            let find = |page: u16, usage: u16| {
                api.device_list().find(|d| {
                    d.vendor_id() == p::VID
                        && d.product_id() == pid
                        && d.usage_page() == page
                        && d.usage() == usage
                })
            };
            if let (Some(c), Some(n)) = (
                find(p::CMD_USAGE_PAGE, p::CMD_USAGE),
                find(p::NOTIFY_USAGE_PAGE, p::NOTIFY_USAGE),
            ) {
                let cmd = c.open_device(api)?;
                let notify = n.open_device(api)?;
                return Ok(Device { cmd, notify, pid, pending_events: Vec::new() });
            }
        }
        Err(DeviceError::NotFound)
    }

    fn drain(&mut self) -> Result<()> {
        let mut buf = [0u8; 64];
        while self.notify.read_timeout(&mut buf, 0)? > 0 {
            if let Some(e) = p::parse_event(&buf) {
                self.pending_events.push(e);
            }
        }
        Ok(())
    }

    fn send_once(&mut self, cmd: u8, payload: &[u8]) -> Result<Option<[u8; p::PACKET_LEN]>> {
        self.drain()?;
        self.cmd.send_feature_report(&p::packet(cmd, payload))?;
        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        let mut buf = [0u8; 64];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let n = self.notify.read_timeout(&mut buf, left.as_millis().max(1) as i32)?;
            if n < p::PACKET_LEN || buf[0] != p::REPORT_NOTIFY {
                continue;
            }
            if let Some(e) = p::parse_event(&buf) {
                self.pending_events.push(e);
                continue;
            }
            if buf[1] == cmd {
                let mut r = [0u8; p::PACKET_LEN];
                r.copy_from_slice(&buf[..p::PACKET_LEN]);
                return Ok(Some(r));
            }
        }
    }

    /// Envia um comando e devolve a resposta (status já verificado).
    pub fn send(&mut self, cmd: u8, payload: &[u8]) -> Result<[u8; p::PACKET_LEN]> {
        for _ in 0..RETRIES {
            if let Some(r) = self.send_once(cmd, payload)? {
                if r[2] != 0 {
                    return Err(DeviceError::Rejected { cmd, status: r[2] });
                }
                return Ok(r);
            }
        }
        Err(DeviceError::Timeout)
    }

    pub fn handshake(&mut self) -> Result<()> {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
        let rnd = [0, 8, 16, 24].map(|s| ((seed >> s) % 100) as u8 + 1);
        let r = self.send(p::CMD_HANDSHAKE, &p::handshake_payload(rnd))?;
        if r[6..10] != p::handshake_expected(rnd) {
            return Err(DeviceError::Handshake);
        }
        Ok(())
    }

    pub fn read_eeprom(&mut self, addr: usize, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let mut a = addr;
        while out.len() < len {
            let n = (len - out.len()).min(p::EEPROM_CHUNK);
            let r = self.send(p::CMD_READ_EEPROM, &p::read_eeprom_payload(a, n))?;
            out.extend_from_slice(&r[6..6 + n]);
            a += n;
        }
        Ok(out)
    }

    pub fn write_eeprom(&mut self, addr: usize, data: &[u8]) -> Result<()> {
        for (i, chunk) in data.chunks(p::EEPROM_CHUNK).enumerate() {
            let a = addr + i * p::EEPROM_CHUNK;
            match self.send(p::CMD_WRITE_EEPROM, &p::write_eeprom_payload(a, chunk)) {
                Ok(_) => {}
                // Ao acordar, o mouse pode aplicar a escrita sem que a resposta chegue:
                // confirma relendo antes de considerar falha.
                Err(DeviceError::Timeout) if self.read_eeprom(a, chunk.len())? == chunk => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Lê eventos espontâneos sem bloquear.
    pub fn poll_events(&mut self) -> Result<Vec<DeviceEvent>> {
        self.drain()?;
        Ok(std::mem::take(&mut self.pending_events))
    }
}
