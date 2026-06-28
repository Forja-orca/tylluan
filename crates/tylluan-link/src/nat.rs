use rand::RngCore;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::Duration;

// RFC 5389 constants
const MAGIC_COOKIE: u32 = 0x2112_A442;
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_RESPONSE: u16 = 0x0101;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

#[derive(Debug, Clone)]
pub struct NatConfig {
    pub stun_servers: Vec<String>,
    pub stun_timeout_secs: u64,
    pub stun_retries: u32,
}

impl Default for NatConfig {
    fn default() -> Self {
        Self {
            stun_servers: vec![
                "stun.l.google.com:19302".to_string(),
                "stun.cloudflare.com:3478".to_string(),
            ],
            stun_timeout_secs: 5,
            stun_retries: 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExternalAddr {
    pub ip: IpAddr,
    pub port: u16,
    pub stun_server: String,
}

pub async fn discover_external_addr(config: &NatConfig) -> anyhow::Result<ExternalAddr> {
    let timeout = Duration::from_secs(config.stun_timeout_secs);
    for server in &config.stun_servers {
        for attempt in 0..=config.stun_retries {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            match stun_binding_request(server, timeout).await {
                Ok((ip, port)) => {
                    return Ok(ExternalAddr {
                        ip,
                        port,
                        stun_server: server.clone(),
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        "STUN attempt {}/{} to '{}' failed: {e}",
                        attempt + 1,
                        config.stun_retries + 1,
                        server
                    );
                }
            }
        }
    }
    anyhow::bail!("all STUN servers failed after retries")
}

async fn stun_binding_request(
    server: &str,
    timeout: Duration,
) -> anyhow::Result<(IpAddr, u16)> {
    let server_addr: SocketAddr = server
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid STUN server address '{server}': {e}"))?;

    let local = if server_addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = UdpSocket::bind(local)?;
    socket.set_read_timeout(Some(timeout))?;
    socket.set_write_timeout(Some(timeout))?;
    socket.connect(server_addr)?;

    let mut tx_id = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut tx_id);
    let request = build_binding_request(tx_id);

    socket.send(&request)?;

    let mut buf = [0u8; 548];
    let n = socket.recv(&mut buf)?;
    if n < 20 {
        anyhow::bail!("STUN response too short: {n} bytes");
    }

    parse_binding_response(&buf[..n], &tx_id)
}

fn build_binding_request(tx_id: [u8; 12]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(20);
    msg.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
    msg.extend_from_slice(&0u16.to_be_bytes()); // length, filled later
    msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    msg.extend_from_slice(&tx_id);

    // FINGERPRINT attribute (optional but helps some STUN servers)
    let attr_type = 0x8028u16;
    let attr_len = 4u16;
    let msg_len = (msg.len() + 4 + attr_len as usize) as u16 - 20;
    msg[2..4].copy_from_slice(&msg_len.to_be_bytes());

    msg.extend_from_slice(&attr_type.to_be_bytes());
    msg.extend_from_slice(&attr_len.to_be_bytes());

    let fingerprint = crc32_fingerprint(&msg);
    msg.extend_from_slice(&fingerprint.to_be_bytes());

    msg
}

fn crc32_fingerprint(msg: &[u8]) -> u32 {
    // Reflected form of IEEE 802.3 CRC-32 polynomial (0x04C11DB7 → 0xEDB88320)
    // for bit-reflected (LSB-first) implementation
    const CRC32_POLY: u32 = 0xEDB8_8320;
    let mut crc = !0u32;
    for &byte in msg {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ CRC32_POLY;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc ^ 0x5354_554e
}

fn parse_binding_response(
    buf: &[u8],
    tx_id: &[u8; 12],
) -> anyhow::Result<(IpAddr, u16)> {
    if buf.len() < 20 {
        anyhow::bail!("response too short");
    }

    let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
    if msg_type != BINDING_RESPONSE {
        anyhow::bail!("unexpected STUN message type: 0x{msg_type:04x}");
    }

    let cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    if cookie != MAGIC_COOKIE {
        anyhow::bail!("invalid magic cookie");
    }

    if &buf[8..20] != tx_id {
        anyhow::bail!("transaction ID mismatch");
    }

    let attr_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let mut offset = 20usize;
    let end = offset + attr_len;

    while offset + 4 <= end && offset + 4 <= buf.len() {
        let atype = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
        let alen = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]) as usize;
        let value_start = offset + 4;
        let value_end = value_start + alen;

        if value_end > buf.len() {
            break;
        }

        if atype == ATTR_XOR_MAPPED_ADDRESS && alen >= 8 {
            let family = buf[value_start + 1];
            let xor_port = u16::from_be_bytes([
                buf[value_start + 2],
                buf[value_start + 3],
            ]);
            let port = xor_port ^ (MAGIC_COOKIE >> 16) as u16;

            let ip: IpAddr = if family == 0x01 {
                let xor_ip = u32::from_be_bytes([
                    buf[value_start + 4],
                    buf[value_start + 5],
                    buf[value_start + 6],
                    buf[value_start + 7],
                ]);
                let ip = xor_ip ^ MAGIC_COOKIE;
                Ipv4Addr::from(ip).into()
            } else if family == 0x02 {
                let mut xor_ip = [0u8; 16];
                for i in 0..16 {
                    let tx_byte = if i < 12 { tx_id[i] } else { 0 };
                    xor_ip[i] = buf[value_start + 4 + i] ^ (MAGIC_COOKIE >> (24 - i * 8)) as u8 ^ tx_byte;
                }
                Ipv6Addr::from(xor_ip).into()
            } else {
                anyhow::bail!("unknown address family: {family}");
            };

            return Ok((ip, port));
        }

        offset = value_end;
        if !offset.is_multiple_of(4) {
            offset += 4 - (offset % 4);
        }
    }

    anyhow::bail!("no XOR-MAPPED-ADDRESS attribute found")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_binding_request_length() {
        let tx_id = [0u8; 12];
        let msg = build_binding_request(tx_id);
        assert!(msg.len() > 20);
        assert_eq!(&msg[0..2], &BINDING_REQUEST.to_be_bytes());
        assert_eq!(&msg[4..8], &MAGIC_COOKIE.to_be_bytes());
    }

    #[test]
    fn test_parse_response_invalid_type() {
        let tx_id = [0u8; 12];
        let mut buf = vec![0u8; 20];
        buf[0..2].copy_from_slice(&0x0000u16.to_be_bytes());
        buf[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        buf[8..20].copy_from_slice(&tx_id);

        let result = parse_binding_response(&buf, &tx_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unexpected STUN message type"));
    }

    #[test]
    fn test_parse_response_invalid_cookie() {
        let tx_id = [0u8; 12];
        let mut buf = vec![0u8; 20];
        buf[0..2].copy_from_slice(&BINDING_RESPONSE.to_be_bytes());
        // bad cookie
        let result = parse_binding_response(&buf, &tx_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid magic cookie"));
    }

    #[test]
    fn test_parse_response_xor_mapped_ipv4() {
        // Simulate a binding response with XOR-MAPPED-ADDRESS for 192.168.1.1:12345
        let mut buf = vec![0u8; 28];
        // Header
        buf[0..2].copy_from_slice(&BINDING_RESPONSE.to_be_bytes());
        let msg_len = 8u16;
        buf[2..4].copy_from_slice(&msg_len.to_be_bytes());
        buf[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        let tx_id = [0xABu8; 12];
        buf[8..20].copy_from_slice(&tx_id);

        // XOR-MAPPED-ADDRESS attribute
        buf[20..22].copy_from_slice(&ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
        buf[22..24].copy_from_slice(&8u16.to_be_bytes());
        buf[24] = 0; // padding
        buf[25] = 0x01; // IPv4

        // port 12345 XOR'd with magic cookie high 16 bits
        let port_xor = 12345u16 ^ (MAGIC_COOKIE >> 16) as u16;
        buf[26..28].copy_from_slice(&port_xor.to_be_bytes());

        // IP: x.x.x.x XOR'd with MAGIC_COOKIE
        // Let's say external IP is 203.0.113.42
        let ext_ip = Ipv4Addr::new(203, 0, 113, 42);
        let ext_u32 = u32::from(ext_ip);
        let xor_ip = ext_u32 ^ MAGIC_COOKIE;
        buf.extend_from_slice(&xor_ip.to_be_bytes());

        let (ip, port) = parse_binding_response(&buf, &tx_id).unwrap();
        assert_eq!(ip, "203.0.113.42".parse::<IpAddr>().unwrap());
        assert_eq!(port, 12345);
    }

    #[test]
    fn test_crc32_fingerprint_known_value() {
        // Standard IEEE CRC32 reflected (zlib) XOR'd with 0x5354554E
        let msg = [0u8; 20];
        let fp = crc32_fingerprint(&msg);
        // Verified: zlib.crc32([0;20]) ^ 0x5354554E = 0x5c81cec3
        assert_eq!(fp, 0x5c81cec3);
    }

    #[test]
    fn test_parse_response_txid_mismatch() {
        let sent_txid = [0xABu8; 12];
        let mut buf = vec![0u8; 20];
        buf[0..2].copy_from_slice(&BINDING_RESPONSE.to_be_bytes());
        buf[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        // Response has zero txid, sent non-zero → mismatch
        let result = parse_binding_response(&buf, &sent_txid);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("transaction ID mismatch"));
    }

    #[test]
    fn test_parse_response_missing_xor_attribute() {
        let tx_id = [0u8; 12];
        let mut buf = vec![0u8; 20];
        buf[0..2].copy_from_slice(&BINDING_RESPONSE.to_be_bytes());
        let msg_len = 0u16;
        buf[2..4].copy_from_slice(&msg_len.to_be_bytes());
        buf[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        buf[8..20].copy_from_slice(&tx_id);
        // No attributes → no XOR-MAPPED-ADDRESS found
        let result = parse_binding_response(&buf, &tx_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no XOR-MAPPED-ADDRESS"));
    }

    #[tokio::test]
    async fn test_discover_with_bad_servers_fails() {
        let config = NatConfig {
            stun_servers: vec!["127.0.0.1:9999".to_string()],
            stun_timeout_secs: 1,
            stun_retries: 0,
        };
        let result = discover_external_addr(&config).await;
        assert!(result.is_err());
    }
}
