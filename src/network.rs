//! Interface discovery and explicit source/interface binding. No route or firewall changes.
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use serde::{Deserialize, Serialize};
use std::{
    io,
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    time::Duration,
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub ip: IpAddr,
    pub index: u32,
    pub mac: Option<String>,
}
impl Binding {
    pub fn label(&self) -> String {
        format!(
            "{} · {} · MAC {}",
            self.name,
            self.ip,
            self.mac.as_deref().unwrap_or("— (virtuell/keine MAC)")
        )
    }
    pub fn address(&self, port: u16) -> SocketAddr {
        match self.ip {
            IpAddr::V4(ip) => SocketAddr::new(ip.into(), port),
            IpAddr::V6(ip) => std::net::SocketAddrV6::new(
                ip,
                port,
                0,
                if ip.is_unicast_link_local() {
                    self.index
                } else {
                    0
                },
            )
            .into(),
        }
    }
}
pub fn interfaces() -> io::Result<Vec<Binding>> {
    let mut result = Vec::new();
    for interface in NetworkInterface::show().map_err(io::Error::other)? {
        for addr in interface.addr {
            let ip = addr.ip();
            if !ip.is_unspecified() && !ip.is_multicast() {
                result.push(Binding {
                    name: interface.name.clone(),
                    ip,
                    index: interface.index,
                    mac: interface.mac_addr.clone(),
                });
            }
        }
    }
    result.sort_by_key(|a| (a.name.clone(), a.ip));
    result.dedup_by(|a, b| a.name == b.name && a.ip == b.ip);
    Ok(result)
}
fn socket(address: SocketAddr) -> io::Result<socket2::Socket> {
    socket2::Socket::new(
        socket2::Domain::for_address(address),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
}
pub fn listeners(automatic: bool, selected: &[Binding], port: u16) -> io::Result<Vec<TcpListener>> {
    if automatic {
        let address = SocketAddr::new(std::net::Ipv6Addr::UNSPECIFIED.into(), port);
        let s = socket(address)?;
        s.set_only_v6(false)?;
        #[cfg(unix)]
        s.set_reuse_address(true)?;
        s.bind(&address.into())?;
        s.listen(16)?;
        s.set_nonblocking(true)?;
        return Ok(vec![s.into()]);
    }
    if selected.is_empty() {
        return Err(io::Error::other("Mindestens eine lokale IP auswählen."));
    }
    let current = interfaces()?;
    let mut listeners = Vec::new();
    for chosen in selected {
        let binding = current
            .iter()
            .find(|b| b.name == chosen.name && b.ip == chosen.ip)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AddrNotAvailable,
                    "Gewählte Schnittstelle ist nicht verfügbar. Auswahl prüfen.",
                )
            })?;
        let address = binding.address(port);
        let s = socket(address)?;
        if address.is_ipv6() {
            s.set_only_v6(true)?;
        }
        #[cfg(unix)]
        s.set_reuse_address(true)?;
        s.bind(&address.into())?;
        s.listen(16)?;
        s.set_nonblocking(true)?;
        listeners.push(s.into());
    }
    Ok(listeners)
}
fn select_interface(s: &socket2::Socket, binding: &Binding) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let index = std::num::NonZeroU32::new(binding.index)
            .ok_or_else(|| io::Error::other("Ungültiger Schnittstellenindex"))?;
        if binding.ip.is_ipv4() {
            s.bind_device_by_index_v4(Some(index))?;
        } else {
            s.bind_device_by_index_v6(Some(index))?;
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let index = binding.index.to_be();
        let (level, option) = if binding.ip.is_ipv4() {
            (libc::IPPROTO_IP, 50)
        } else {
            (libc::IPPROTO_IPV6, 76)
        };
        if unsafe {
            libc::setsockopt(
                s.as_raw_fd(),
                level,
                option,
                (&index as *const u32).cast(),
                4,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(windows)]
    {
        use std::{ffi::c_char, os::windows::io::AsRawSocket};
        #[link(name = "ws2_32")]
        unsafe extern "system" {
            fn setsockopt(s: usize, level: i32, name: i32, value: *const c_char, len: i32) -> i32;
            fn WSAGetLastError() -> i32;
        }
        let (level, index) = if binding.ip.is_ipv4() {
            (0, binding.index.to_be())
        } else {
            (41, binding.index)
        };
        if unsafe {
            setsockopt(
                s.as_raw_socket() as usize,
                level,
                31,
                (&index as *const u32).cast(),
                4,
            )
        } != 0
        {
            return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }));
        }
    }
    s.bind(&binding.address(0).into())
}
pub fn connect(
    remote: SocketAddr,
    automatic: bool,
    selected: &[Binding],
    timeout: Duration,
) -> io::Result<TcpStream> {
    if automatic {
        return TcpStream::connect_timeout(&remote, timeout);
    }
    let current = interfaces()?;
    let started = std::time::Instant::now();
    let mut error = io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        "Keine gewählte Schnittstelle für diese Adressfamilie verfügbar.",
    );
    let candidates: Vec<_> = selected
        .iter()
        .filter(|b| b.ip.is_ipv4() == remote.is_ipv4())
        .filter_map(|chosen| {
            current
                .iter()
                .find(|b| b.ip == chosen.ip && b.name == chosen.name)
        })
        .collect();
    for (index, binding) in candidates.iter().enumerate() {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Schnittstellen-Verbindungszeitlimit",
                )
            })?;
        let budget = remaining / (candidates.len() - index) as u32;
        crate::traffic::local(binding.address(0).to_string());
        crate::traffic::record("OUT", "interface attempt", 0);
        let result = (|| {
            let s = socket(remote)?;
            select_interface(&s, binding)?;
            let mut destination = remote;
            if let SocketAddr::V6(ref mut ip) = destination {
                if ip.ip().is_unicast_link_local() {
                    ip.set_scope_id(binding.index);
                }
            }
            s.connect_timeout(&destination.into(), budget)?;
            Ok(s.into())
        })();
        match result {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                crate::traffic::failure(&e.to_string());
                error = e;
            }
        }
    }
    Err(error)
}
