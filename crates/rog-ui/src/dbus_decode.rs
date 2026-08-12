//! Small, lossless helpers for decoding the daemon's backwards-compatible
//! `a{sv}` maps. Missing fields and wrong wire types remain `None`; domain
//! decoders decide their own safe defaults explicitly.

use std::collections::HashMap;

use zbus::zvariant::OwnedValue;

pub(crate) type WireMap = HashMap<String, OwnedValue>;

pub(crate) fn boolean(map: &WireMap, key: &str) -> Option<bool> {
    map.get(key).and_then(|value| bool::try_from(value).ok())
}

pub(crate) fn unsigned(map: &WireMap, key: &str) -> Option<u64> {
    map.get(key).and_then(unsigned_value)
}

pub(crate) fn unsigned_value(value: &OwnedValue) -> Option<u64> {
    u64::try_from(value)
        .ok()
        .or_else(|| u32::try_from(value).ok().map(u64::from))
        .or_else(|| u16::try_from(value).ok().map(u64::from))
        .or_else(|| u8::try_from(value).ok().map(u64::from))
}

pub(crate) fn unsigned_u32(map: &WireMap, key: &str) -> Option<u32> {
    unsigned(map, key).and_then(|value| u32::try_from(value).ok())
}

pub(crate) fn float(map: &WireMap, key: &str) -> Option<f64> {
    map.get(key).and_then(|value| f64::try_from(value).ok())
}

pub(crate) fn string(map: &WireMap, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(str::to_string)
}

pub(crate) fn strings(map: &WireMap, key: &str) -> Option<Vec<String>> {
    map.get(key)
        .cloned()
        .and_then(|value| Vec::<String>::try_from(value).ok())
}

pub(crate) fn nested_map(map: &WireMap, key: &str) -> Option<WireMap> {
    map.get(key)
        .cloned()
        .and_then(|value| WireMap::try_from(value).ok())
}

pub(crate) fn rows(map: &WireMap, key: &str) -> Option<Vec<WireMap>> {
    map.get(key)
        .cloned()
        .and_then(|value| Vec::<WireMap>::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use zbus::zvariant::{OwnedValue, Value};

    use super::*;

    fn ov<T>(value: T) -> OwnedValue
    where
        T: Into<Value<'static>>,
    {
        OwnedValue::try_from(value.into()).expect("OwnedValue conversion should succeed")
    }

    #[test]
    fn missing_and_wrong_type_fields_remain_absent() {
        let mut map = WireMap::new();
        map.insert("number".to_string(), ov("not a number".to_string()));
        map.insert("flag".to_string(), OwnedValue::from(1_u64));

        assert_eq!(unsigned(&map, "missing"), None);
        assert_eq!(unsigned(&map, "number"), None);
        assert_eq!(boolean(&map, "flag"), None);
    }

    #[test]
    fn integer_widths_decode_compatibly() {
        let mut map = WireMap::new();
        map.insert("u8".to_string(), OwnedValue::from(8_u8));
        map.insert("u32".to_string(), OwnedValue::from(32_u32));
        map.insert("u64".to_string(), OwnedValue::from(64_u64));

        assert_eq!(unsigned(&map, "u8"), Some(8));
        assert_eq!(unsigned(&map, "u32"), Some(32));
        assert_eq!(unsigned(&map, "u64"), Some(64));
    }
}
