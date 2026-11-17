use std::net::IpAddr;

#[derive(Debug, Clone)]
pub struct InterfaceIp {
    pub ip: String,
    pub is_tailscale: bool,
    pub label: String,
}

/// Detects IPv4 addresses on the machine, prioritizing Tailscale (100.64.0.0/10)
pub fn get_local_ips() -> Vec<InterfaceIp> {
    let mut ips = Vec::new();

    // Query Windows network adapters via netsh or powershell fallback, or socket probing
    // First probe default route
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local_addr) = socket.local_addr() {
                let ip_str = local_addr.ip().to_string();
                let is_ts = is_tailscale_ip(&local_addr.ip());
                ips.push(InterfaceIp {
                    ip: ip_str.clone(),
                    is_tailscale: is_ts,
                    label: if is_ts { "Tailscale VPN".to_string() } else { "Default Gateway".to_string() },
                });
            }
        }
    }

    // Try detecting Tailscale CGNAT IP (100.64.0.0/10) specifically via hostname or system info
    if let Ok(hostname) = std::env::var("COMPUTERNAME") {
        if let Ok(addrs) = dns_lookup(&hostname) {
            for addr in addrs {
                let ip_str = addr.to_string();
                let is_ts = is_tailscale_ip(&addr);
                if !ips.iter().any(|item| item.ip == ip_str) && !addr.is_loopback() {
                    ips.push(InterfaceIp {
                        ip: ip_str,
                        is_tailscale: is_ts,
                        label: if is_ts { "Tailscale IP".to_string() } else { "Local Network".to_string() },
                    });
                }
            }
        }
    }

    // Fallback if empty
    if ips.is_empty() {
        ips.push(InterfaceIp {
            ip: "127.0.0.1".to_string(),
            is_tailscale: false,
            label: "Localhost".to_string(),
        });
    }

    // Sort so Tailscale IP is first
    ips.sort_by_key(|item| !item.is_tailscale);
    ips
}

fn dns_lookup(host: &str) -> Result<Vec<IpAddr>, std::io::Error> {
    use std::net::ToSocketAddrs;
    let addrs = format!("{}:0", host).to_socket_addrs()?;
    Ok(addrs.map(|a| a.ip()).filter(|ip| ip.is_ipv4()).collect())
}

fn is_tailscale_ip(ip: &IpAddr) -> bool {
    if let IpAddr::V4(ipv4) = ip {
        let octets = ipv4.octets();
        // Tailscale uses 100.64.0.0 to 100.127.255.255
        octets[0] == 100 && (octets[1] >= 64 && octets[1] <= 127)
    } else {
        false
    }
}
