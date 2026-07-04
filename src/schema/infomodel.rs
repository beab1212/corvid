//! A registry of well-known field elements.
//!
//! Templates reference fields by numeric id. The information model maps those
//! ids to human-readable names and canonical types so tooling can render a
//! decoded record without a schema handy, and so a `SCHEMA_DEF` can be
//! validated against expected widths.

use crate::schema::field::FieldType;

/// A well-known element definition.
#[derive(Debug, Clone, Copy)]
pub struct Element {
    pub id: u16,
    pub name: &'static str,
    pub ty: FieldType,
    /// Canonical on-wire width for fixed fields, 0 for variable.
    pub width: u16,
}

macro_rules! elem {
    ($id:expr, $name:expr, $ty:expr, $w:expr) => {
        Element { id: $id, name: $name, ty: $ty, width: $w }
    };
}

/// The built-in element table. Ordered by id for binary search.
pub static ELEMENTS: &[Element] = &[
    elem!(1, "octetCount", FieldType::U64, 8),
    elem!(2, "packetCount", FieldType::U64, 8),
    elem!(3, "flowCount", FieldType::U64, 8),
    elem!(4, "protocolIdentifier", FieldType::U8, 1),
    elem!(5, "ipClassOfService", FieldType::U8, 1),
    elem!(6, "tcpControlBits", FieldType::U16, 2),
    elem!(7, "sourceTransportPort", FieldType::U16, 2),
    elem!(8, "sourceIPv4Address", FieldType::U32, 4),
    elem!(9, "sourceIPv4PrefixLength", FieldType::U8, 1),
    elem!(10, "ingressInterface", FieldType::U32, 4),
    elem!(11, "destinationTransportPort", FieldType::U16, 2),
    elem!(12, "destinationIPv4Address", FieldType::U32, 4),
    elem!(13, "destinationIPv4PrefixLength", FieldType::U8, 1),
    elem!(14, "egressInterface", FieldType::U32, 4),
    elem!(15, "ipNextHopIPv4Address", FieldType::U32, 4),
    elem!(16, "bgpSourceAsNumber", FieldType::U32, 4),
    elem!(17, "bgpDestinationAsNumber", FieldType::U32, 4),
    elem!(21, "flowEndSysUpTime", FieldType::U32, 4),
    elem!(22, "flowStartSysUpTime", FieldType::U32, 4),
    elem!(32, "icmpTypeCodeIPv4", FieldType::U16, 2),
    elem!(34, "samplingInterval", FieldType::U32, 4),
    elem!(35, "samplingAlgorithm", FieldType::U8, 1),
    elem!(36, "flowActiveTimeout", FieldType::U16, 2),
    elem!(37, "flowIdleTimeout", FieldType::U16, 2),
    elem!(40, "exportedOctetTotalCount", FieldType::U64, 8),
    elem!(41, "exportedMessageTotalCount", FieldType::U64, 8),
    elem!(42, "exportedFlowRecordTotalCount", FieldType::U64, 8),
    elem!(52, "minimumTTL", FieldType::U8, 1),
    elem!(53, "maximumTTL", FieldType::U8, 1),
    elem!(56, "sourceMacAddress", FieldType::Fixed, 6),
    elem!(57, "postDestinationMacAddress", FieldType::Fixed, 6),
    elem!(58, "vlanId", FieldType::U16, 2),
    elem!(60, "ipVersion", FieldType::U8, 1),
    elem!(61, "flowDirection", FieldType::U8, 1),
    elem!(82, "interfaceName", FieldType::Utf8, 0),
    elem!(83, "interfaceDescription", FieldType::Utf8, 0),
    elem!(85, "octetTotalCount", FieldType::U64, 8),
    elem!(86, "packetTotalCount", FieldType::U64, 8),
    elem!(136, "flowEndReason", FieldType::U8, 1),
    elem!(150, "flowStartSeconds", FieldType::Timestamp, 8),
    elem!(151, "flowEndSeconds", FieldType::Timestamp, 8),
    elem!(152, "flowStartMilliseconds", FieldType::Timestamp, 8),
    elem!(153, "flowEndMilliseconds", FieldType::Timestamp, 8),
    elem!(210, "paddingOctets", FieldType::Fixed, 0),
    elem!(214, "exportProtocolVersion", FieldType::U8, 1),
    elem!(230, "natEvent", FieldType::U8, 1),
    elem!(233, "firewallEvent", FieldType::U8, 1),
    elem!(234, "ingressVRFID", FieldType::U32, 4),
    elem!(235, "egressVRFID", FieldType::U32, 4),
    elem!(239, "biflowDirection", FieldType::U8, 1),
    elem!(243, "dot1qVlanId", FieldType::U16, 2),
    elem!(256, "sourceIPv6Address", FieldType::Fixed, 16),
    elem!(257, "destinationIPv6Address", FieldType::Fixed, 16),
    elem!(258, "applicationId", FieldType::Fixed, 4),
    elem!(300, "observationDomainName", FieldType::Utf8, 0),
    elem!(315, "dataLinkFrameSize", FieldType::U16, 2),
    elem!(346, "privateEnterpriseNumber", FieldType::U32, 4),
    elem!(400, "layer2FrameSection", FieldType::VarBytes, 0),
];

/// Look up an element by id.
pub fn lookup(id: u16) -> Option<&'static Element> {
    match ELEMENTS.binary_search_by_key(&id, |e| e.id) {
        Ok(i) => Some(&ELEMENTS[i]),
        Err(_) => None,
    }
}

/// Resolve an id to a name, or `"unknown"`.
pub fn name_of(id: u16) -> &'static str {
    lookup(id).map(|e| e.name).unwrap_or("unknown")
}

/// Whether a `SCHEMA_DEF` field's width agrees with the model, if the element
/// is known and fixed-width.
pub fn width_ok(id: u16, ty: FieldType, width: u16) -> bool {
    match lookup(id) {
        Some(e) if e.width != 0 => e.ty == ty && e.width == width,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted() {
        for w in ELEMENTS.windows(2) {
            assert!(w[0].id < w[1].id, "elements out of order at {}", w[0].id);
        }
    }

    #[test]
    fn lookups() {
        assert_eq!(name_of(8), "sourceIPv4Address");
        assert_eq!(name_of(9999), "unknown");
        assert!(lookup(82).unwrap().ty == FieldType::Utf8);
    }
}
