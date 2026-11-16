//! Native KDE Wallet fallback, keeping one session handle while the app is running.
#![cfg(target_os = "linux")]
use base64::{engine::general_purpose::STANDARD, Engine};
use dbus::blocking::Connection;
use std::{cell::RefCell, io, time::Duration};
use zeroize::Zeroizing;
const APP: &str = "com.github.OnlySecci.EditorTool";
const FOLDER: &str = "EditorTool";
const INTERFACE: &str = "org.kde.KWallet";
fn error(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("KDE Wallet: {e}"))
}
fn bus_error(e: dbus::Error) -> io::Error {
    error(e.name().unwrap_or("D-Bus request failed"))
}
struct Wallet {
    bus: Connection,
    service: String,
    path: String,
    handle: i32,
}
thread_local! { static SESSION: RefCell<Option<Wallet>> = const {RefCell::new(None)}; }
impl Wallet {
    fn open() -> io::Result<Self> {
        let bus = Connection::new_session().map_err(bus_error)?;
        let mut last = String::from("No KDE Wallet service available");
        for version in [6, 5] {
            let service = format!("org.kde.kwalletd{version}");
            let path = format!("/modules/kwalletd{version}");
            let mut proxy = bus.with_proxy(service.as_str(), path.as_str(), Duration::from_secs(3));
            let wallet: Result<(String,), _> = proxy.method_call(INTERFACE, "networkWallet", ());
            let wallet = match wallet {
                Ok((name,)) => name,
                Err(e) => {
                    last = e.name().unwrap_or("D-Bus service unavailable").to_owned();
                    continue;
                }
            };
            proxy.timeout = Duration::from_secs(60);
            let (handle,): (i32,) = proxy
                .method_call(INTERFACE, "open", (wallet, 0i64, APP))
                .map_err(bus_error)?;
            if handle < 0 {
                return Err(error(
                    "Unlock or access was declined; existing protection was kept",
                ));
            }
            drop(proxy);
            return Ok(Self {
                bus,
                service,
                path,
                handle,
            });
        }
        Err(error(last))
    }
    fn access(&self, id: &str, write: Option<&[u8]>) -> io::Result<Zeroizing<Vec<u8>>> {
        let proxy = self.bus.with_proxy(
            self.service.as_str(),
            self.path.as_str(),
            Duration::from_secs(60),
        );
        let handle = self.handle;
        if let Some(secret) = write {
            let (exists,): (bool,) = proxy
                .method_call(INTERFACE, "hasFolder", (handle, FOLDER, APP))
                .map_err(bus_error)?;
            if !exists {
                let (created,): (bool,) = proxy
                    .method_call(INTERFACE, "createFolder", (handle, FOLDER, APP))
                    .map_err(bus_error)?;
                if !created {
                    return Err(error("Could not create the application folder"));
                }
            }
            let encoded = Zeroizing::new(STANDARD.encode(secret));
            let (code,): (i32,) = proxy
                .method_call(
                    INTERFACE,
                    "writePassword",
                    (handle, FOLDER, id, encoded.as_str(), APP),
                )
                .map_err(bus_error)?;
            if code != 0 {
                return Err(error("Could not store the profile key"));
            }
            let _: () = proxy
                .method_call(INTERFACE, "sync", (handle, APP))
                .map_err(bus_error)?;
        }
        let (encoded,): (String,) = proxy
            .method_call(INTERFACE, "readPassword", (handle, FOLDER, id, APP))
            .map_err(bus_error)?;
        let encoded = Zeroizing::new(encoded);
        if encoded.len() != 44 {
            return Err(error(
                "Invalid stored key length; no replacement was generated",
            ));
        }
        let bytes = Zeroizing::new(
            STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| error("Invalid stored key encoding"))?,
        );
        if bytes.len() != 32 {
            return Err(error(
                "The profile key is missing or invalid; no replacement was generated",
            ));
        }
        Ok(bytes)
    }
}
impl Drop for Wallet {
    fn drop(&mut self) {
        let proxy = self.bus.with_proxy(
            self.service.as_str(),
            self.path.as_str(),
            Duration::from_secs(1),
        );
        let _: Result<(i32,), _> = proxy.method_call(INTERFACE, "close", (self.handle, false, APP));
    }
}
pub fn access(id: &str, write: Option<&[u8]>) -> io::Result<Zeroizing<Vec<u8>>> {
    SESSION.with(|slot| {
        let mut session = slot.borrow_mut();
        if session.is_none() {
            *session = Some(Wallet::open()?);
        }
        let result = session.as_ref().unwrap().access(id, write);
        if result.is_err() {
            session.take();
        }
        result
    })
}
