use std::time::Duration;

use regex::Regex;
use rog_core::{RogError, RogResult};
use tokio::time::timeout;
use tracing::{debug, warn};
use zbus::fdo::DBusProxy;
use zbus::Proxy;
use zvariant::OwnedObjectPath;

#[derive(Debug, Clone)]
pub struct DbusIntrospection {
    pub service: String,
    pub path: String,
    pub interfaces: Vec<String>,
    pub children: Vec<String>,
    pub xml: String,
}

pub async fn list_system_bus_names() -> RogResult<Vec<String>> {
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| RogError::DependencyMissing(format!("system dbus unavailable: {e}")))?;
    let proxy = DBusProxy::new(&conn)
        .await
        .map_err(|e| RogError::Unexpected(format!("dbus proxy error: {e}")))?;
    let names = proxy
        .list_names()
        .await
        .map_err(|e| RogError::Unexpected(format!("dbus ListNames failed: {e}")))?;

    Ok(names.into_iter().map(|n| n.to_string()).collect())
}

pub async fn list_system_bus_names_matching(re: &Regex) -> RogResult<Vec<String>> {
    let mut names = list_system_bus_names().await?;
    names.retain(|n| re.is_match(n));
    names.sort();
    Ok(names)
}

pub async fn system_introspect(service: &str, path: &str, timeout_ms: u64) -> RogResult<String> {
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| RogError::DependencyMissing(format!("system dbus unavailable: {e}")))?;

    let proxy = Proxy::new(&conn, service, path, "org.freedesktop.DBus.Introspectable")
        .await
        .map_err(|e| RogError::Unexpected(format!("introspect proxy build failed: {e}")))?;

    let call_fut = proxy.call::<_, _, String>("Introspect", &());
    let xml = timeout(Duration::from_millis(timeout_ms), call_fut)
        .await
        .map_err(|_| RogError::TransientFailure("introspection timed out".to_string()))?
        .map_err(|e| RogError::Unexpected(format!("Introspect failed: {e}")))?;

    Ok(xml)
}

pub fn parse_introspection(xml: &str) -> (Vec<String>, Vec<String>) {
    // This is intentionally lightweight: introspection XML is simple and we only need names.
    let iface_re = Regex::new(r#"interface name="([^"]+)""#).unwrap();
    let node_re = Regex::new(r#"node name="([^"]+)""#).unwrap();

    let mut ifaces: Vec<String> = iface_re
        .captures_iter(xml)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    ifaces.sort();
    ifaces.dedup();

    let mut children: Vec<String> = node_re
        .captures_iter(xml)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    children.sort();
    children.dedup();

    (ifaces, children)
}

pub async fn system_introspect_parsed(
    service: &str,
    path: &str,
    timeout_ms: u64,
) -> RogResult<DbusIntrospection> {
    let xml = system_introspect(service, path, timeout_ms).await?;
    let (interfaces, children) = parse_introspection(&xml);
    Ok(DbusIntrospection {
        service: service.to_string(),
        path: path.to_string(),
        interfaces,
        children,
        xml,
    })
}

pub async fn system_walk_introspection(
    service: &str,
    root: &str,
    max_depth: usize,
    max_nodes: usize,
    timeout_ms: u64,
) -> RogResult<Vec<DbusIntrospection>> {
    let mut out = Vec::new();
    let mut queue: Vec<(String, usize)> = vec![(root.to_string(), 0)];

    while let Some((path, depth)) = queue.pop() {
        if out.len() >= max_nodes {
            warn!(
                service,
                root, "introspection walk hit max_nodes={max_nodes}"
            );
            break;
        }
        if depth > max_depth {
            continue;
        }

        debug!(service, %path, depth, "introspecting");
        match system_introspect_parsed(service, &path, timeout_ms).await {
            Ok(info) => {
                let children = info.children.clone();
                out.push(info);

                for child in children {
                    // child could be relative. Introspection spec uses relative names.
                    let next = if path == "/" {
                        format!("/{child}")
                    } else {
                        format!("{path}/{child}")
                    };
                    queue.push((next, depth + 1));
                }
            }
            Err(e) => {
                warn!(service, %path, "introspection failed: {e}");
            }
        }
    }

    Ok(out)
}

pub async fn system_upower_paths() -> RogResult<Vec<OwnedObjectPath>> {
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| RogError::DependencyMissing(format!("system dbus unavailable: {e}")))?;
    let proxy = Proxy::new(
        &conn,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower",
        "org.freedesktop.UPower",
    )
    .await
    .map_err(|e| RogError::DependencyMissing(format!("UPower not available: {e}")))?;

    proxy
        .call::<_, _, Vec<OwnedObjectPath>>("EnumerateDevices", &())
        .await
        .map_err(|e| RogError::Unexpected(format!("UPower EnumerateDevices failed: {e}")))
}
