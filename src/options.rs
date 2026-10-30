//! DHCP option parsing (RFC 2132).

/// Pad option: a single byte with no length field.
pub const PAD: u8 = 0;
/// End option: terminates the option list.
pub const END: u8 = 255;

pub const SUBNET_MASK: u8 = 1;
pub const ROUTER: u8 = 3;
pub const DNS_SERVER: u8 = 6;
pub const HOST_NAME: u8 = 12;
pub const DOMAIN_NAME: u8 = 15;
pub const BROADCAST_ADDRESS: u8 = 28;
pub const REQUESTED_IP: u8 = 50;
pub const LEASE_TIME: u8 = 51;
pub const MESSAGE_TYPE: u8 = 53;
pub const SERVER_ID: u8 = 54;
pub const RENEWAL_TIME: u8 = 58;
pub const REBINDING_TIME: u8 = 59;

/// Iterates over `(code, value)` pairs of a DHCP option field.
///
/// Pad bytes are skipped, iteration stops at the end option, and an option
/// whose length runs past the end of the data ends the iteration instead of
/// reading out of bounds.
pub struct Options<'a> {
    data: &'a [u8],
}

impl<'a> Options<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }
}

impl<'a> Iterator for Options<'a> {
    type Item = (u8, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (&code, rest) = self.data.split_first()?;
            match code {
                PAD => self.data = rest,
                END => {
                    self.data = &[];
                    return None;
                }
                _ => {
                    let (&len, rest) = rest.split_first()?;
                    let len = usize::from(len);
                    if rest.len() < len {
                        self.data = &[];
                        return None;
                    }
                    let (value, rest) = rest.split_at(len);
                    self.data = rest;
                    return Some((code, value));
                }
            }
        }
    }
}

/// The value of the first option with `code`, if present.
pub fn find(data: &[u8], code: u8) -> Option<&[u8]> {
    Options::new(data).find(|(c, _)| *c == code).map(|(_, v)| v)
}

/// The value of option `code` as an IPv4 address, if present and 4 bytes long.
pub fn find_ipv4(data: &[u8], code: u8) -> Option<core::net::Ipv4Addr> {
    let value: [u8; 4] = find(data, code)?.try_into().ok()?;
    Some(core::net::Ipv4Addr::from(value))
}

/// The DHCP message type (option 53), if present and 1 byte long.
pub fn message_type(data: &[u8]) -> Option<u8> {
    match find(data, MESSAGE_TYPE)? {
        [kind] => Some(*kind),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use core::net::Ipv4Addr;
    use leasehund::options::*;

    #[test]
    fn iterates_options_and_skips_pad() {
        let data = [
            PAD,
            PAD,
            MESSAGE_TYPE,
            1,
            3,
            PAD,
            REQUESTED_IP,
            4,
            10,
            0,
            0,
            7,
            END,
            99,
        ];
        let mut options = Options::new(&data);
        assert_eq!(options.next(), Some((MESSAGE_TYPE, &[3u8][..])));
        assert_eq!(options.next(), Some((REQUESTED_IP, &[10u8, 0, 0, 7][..])));
        // Nothing after the end option is read.
        assert_eq!(options.next(), None);
    }

    #[test]
    fn truncated_option_ends_iteration() {
        let data = [MESSAGE_TYPE, 1, 1, REQUESTED_IP, 4, 10, 0];
        let mut options = Options::new(&data);
        assert_eq!(options.next(), Some((MESSAGE_TYPE, &[1u8][..])));
        assert_eq!(options.next(), None);
        // A code without its length byte.
        assert_eq!(Options::new(&[REQUESTED_IP]).next(), None);
    }

    #[test]
    fn message_type_requires_one_byte() {
        assert_eq!(message_type(&[MESSAGE_TYPE, 1, 1, END]), Some(1));
        assert_eq!(message_type(&[PAD, MESSAGE_TYPE, 1, 3]), Some(3));
        assert_eq!(message_type(&[MESSAGE_TYPE, 0, END]), None);
        assert_eq!(message_type(&[MESSAGE_TYPE, 2, 1, 1]), None);
        assert_eq!(message_type(&[]), None);
        assert_eq!(message_type(&[END, MESSAGE_TYPE, 1, 1]), None);
    }

    #[test]
    fn find_ipv4_checks_length() {
        let data = [SERVER_ID, 4, 192, 168, 1, 1, REQUESTED_IP, 3, 1, 2, 3, END];
        assert_eq!(
            find_ipv4(&data, SERVER_ID),
            Some(Ipv4Addr::new(192, 168, 1, 1))
        );
        assert_eq!(find_ipv4(&data, REQUESTED_IP), None);
        assert_eq!(find_ipv4(&data, ROUTER), None);
    }
}
